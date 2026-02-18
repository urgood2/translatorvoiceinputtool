"""Notification system for status, completion, and error events.

This module provides:
- Session tracking for exactly-once delivery semantics
- Event emission helpers
- Async transcription pipeline that emits results

Key Invariants:
- Each session_id receives exactly ONE of: complete OR error (never both)
- Cancelled sessions receive NO notification
- Session tracking is bounded (old sessions auto-expire)
"""

from __future__ import annotations

import threading
import time
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Callable, Optional

import numpy as np

from .protocol import Notification, log, write_notification


class SessionState(Enum):
    """State of a transcription session."""

    PENDING = "pending"
    COMPLETED = "completed"
    CANCELLED = "cancelled"
    ERROR = "error"


@dataclass
class SessionRecord:
    """Tracks a single session's state."""

    session_id: str
    created_at: float  # time.monotonic()
    state: SessionState = SessionState.PENDING
    result_emitted: bool = False


class SessionTracker:
    """Tracks transcription sessions to ensure exactly-once delivery.

    Thread-safe implementation that prevents:
    - Duplicate completion events
    - Completion events for cancelled sessions
    - Both completion AND error for the same session
    """

    def __init__(self, max_age_seconds: int = 300):
        self._sessions: dict[str, SessionRecord] = {}
        self._lock = threading.Lock()
        self._max_age = max_age_seconds

    def register(self, session_id: str) -> None:
        """Register a new session as pending."""
        with self._lock:
            self._cleanup_old()
            self._sessions[session_id] = SessionRecord(
                session_id=session_id,
                created_at=time.monotonic(),
            )

    def mark_cancelled(self, session_id: str) -> bool:
        """Mark a session as cancelled.

        Returns True if successfully cancelled, False if not found.
        """
        with self._lock:
            session = self._sessions.get(session_id)
            if session is None:
                return False

            if session.state == SessionState.PENDING:
                session.state = SessionState.CANCELLED
                return True

            return False

    def should_emit(self, session_id: str) -> bool:
        """Check if we should emit a result for this session.

        Returns True only if:
        - Session exists
        - Session is PENDING
        - No result has been emitted yet
        """
        with self._lock:
            session = self._sessions.get(session_id)
            if session is None:
                return False

            return session.state == SessionState.PENDING and not session.result_emitted

    def mark_completed(self, session_id: str) -> bool:
        """Mark a session as completed (result emitted).

        Returns True if successfully marked, False if not allowed.
        """
        with self._lock:
            session = self._sessions.get(session_id)
            if session is None:
                return False

            if session.state != SessionState.PENDING or session.result_emitted:
                return False

            session.state = SessionState.COMPLETED
            session.result_emitted = True
            return True

    def mark_error(self, session_id: str) -> bool:
        """Mark a session as error (error emitted).

        Returns True if successfully marked, False if not allowed.
        """
        with self._lock:
            session = self._sessions.get(session_id)
            if session is None:
                return False

            if session.state != SessionState.PENDING or session.result_emitted:
                return False

            session.state = SessionState.ERROR
            session.result_emitted = True
            return True

    def get_state(self, session_id: str) -> Optional[SessionState]:
        """Get the state of a session."""
        with self._lock:
            session = self._sessions.get(session_id)
            return session.state if session else None

    def has_pending(self) -> bool:
        """Return True if any transcription session is still pending."""
        with self._lock:
            return any(
                session.state == SessionState.PENDING and not session.result_emitted
                for session in self._sessions.values()
            )

    def _cleanup_old(self) -> None:
        """Remove sessions older than max_age (internal, called with lock)."""
        now = time.monotonic()
        expired = [
            sid
            for sid, record in self._sessions.items()
            if now - record.created_at > self._max_age
        ]
        for sid in expired:
            del self._sessions[sid]


# Global session tracker
_session_tracker: Optional[SessionTracker] = None


def get_session_tracker() -> SessionTracker:
    """Get the global session tracker."""
    global _session_tracker
    if _session_tracker is None:
        _session_tracker = SessionTracker()
    return _session_tracker


# === Event Emission Helpers ===


def emit_status_changed(
    state: str,
    detail: str = "",
    progress: Optional[dict[str, Any]] = None,
    model: Optional[dict[str, Any]] = None,
) -> None:
    """Emit a status_changed event.

    Args:
        state: Current state (idle, loading_model, recording, transcribing, error)
        detail: Human-readable detail message
        progress: Progress info {current, total, unit}
        model: Model info {model_id, status}
    """
    params: dict[str, Any] = {"state": state}

    if detail:
        params["detail"] = detail
    if progress:
        params["progress"] = progress
    if model:
        params["model"] = model

    notification = Notification(method="event.status_changed", params=params)
    write_notification(notification)
    log(f"Event: status_changed state={state}")


def emit_audio_level(
    rms: float,
    peak: float,
    source: str = "meter",
    session_id: Optional[str] = None,
) -> None:
    """Emit an audio_level event.

    Args:
        rms: RMS level, normalized 0-1
        peak: Peak level, normalized 0-1
        source: "meter" for testing, "recording" during active recording
        session_id: Required when source="recording"
    """
    params: dict[str, Any] = {
        "source": source,
        "rms": round(rms, 4),
        "peak": round(peak, 4),
    }

    if session_id:
        params["session_id"] = session_id

    notification = Notification(method="event.audio_level", params=params)
    write_notification(notification)


def emit_transcription_complete(
    session_id: str,
    text: str,
    duration_ms: int,
    confidence: Optional[float] = None,
) -> bool:
    """Emit a transcription_complete event.

    Returns True if emitted, False if blocked by session tracker.

    Args:
        session_id: Session that was transcribed
        text: Transcribed text (post-processed)
        duration_ms: Transcription compute time
        confidence: Optional confidence score 0-1
    """
    tracker = get_session_tracker()

    if not tracker.should_emit(session_id):
        log(f"Blocked transcription_complete for session {session_id} (already handled)")
        return False

    if not tracker.mark_completed(session_id):
        log(f"Failed to mark session {session_id} as completed")
        return False

    params: dict[str, Any] = {
        "session_id": session_id,
        "text": text,
        "duration_ms": duration_ms,
    }

    if confidence is not None:
        params["confidence"] = round(confidence, 3)

    notification = Notification(method="event.transcription_complete", params=params)
    write_notification(notification)
    log(f"Event: transcription_complete session={session_id}, text_len={len(text)}")
    return True


def emit_transcription_error(
    session_id: str,
    kind: str,
    message: str,
) -> bool:
    """Emit a transcription_error event.

    Returns True if emitted, False if blocked by session tracker.

    Args:
        session_id: Session that failed
        kind: Error kind code (e.g., E_TRANSCRIBE)
        message: Human-readable error message
    """
    tracker = get_session_tracker()

    if not tracker.should_emit(session_id):
        log(f"Blocked transcription_error for session {session_id} (already handled)")
        return False

    if not tracker.mark_error(session_id):
        log(f"Failed to mark session {session_id} as error")
        return False

    params = {
        "session_id": session_id,
        "kind": kind,
        "message": message,
    }

    notification = Notification(method="event.transcription_error", params=params)
    write_notification(notification)
    log(f"Event: transcription_error session={session_id}, kind={kind}")
    return True


# === Audio Level Calculation ===


def calculate_audio_levels(audio: np.ndarray) -> tuple[float, float]:
    """Calculate RMS and peak levels from audio samples.

    Args:
        audio: Audio samples as float32 in range [-1, 1]

    Returns:
        Tuple of (rms, peak), both normalized to 0-1 range.
    """
    if len(audio) == 0:
        return 0.0, 0.0

    # Ensure float type for calculation
    audio = audio.astype(np.float32)

    # Calculate RMS (root mean square)
    rms = float(np.sqrt(np.mean(audio**2)))

    # Calculate peak (absolute maximum)
    peak = float(np.abs(audio).max())

    # Clamp to 0-1 range
    rms = min(1.0, max(0.0, rms))
    peak = min(1.0, max(0.0, peak))

    return rms, peak


# === Async Transcription Pipeline ===


def transcribe_session_async(
    session_id: str,
    audio: np.ndarray,
    sample_rate: int,
) -> None:
    """Start async transcription for a session.

    This runs transcription in a background thread and emits
    either transcription_complete or transcription_error when done.

    Args:
        session_id: Session to transcribe
        audio: Audio data as float32
        sample_rate: Sample rate of audio
    """
    tracker = get_session_tracker()
    tracker.register(session_id)

    def run_transcription():
        try:
            emit_status_changed("transcribing", "Processing audio...")

            # Import here to avoid circular imports
            from .preprocess import preprocess
            from .asr import get_engine, NotInitializedError
            from .postprocess import normalize
            from .replacements import get_current_rules, process_text

            # Preprocess audio
            processed_audio = preprocess(audio, sample_rate)

            if len(processed_audio) == 0:
                # Empty audio after processing (all silence)
                emit_transcription_complete(session_id, "", 0)
                emit_status_changed("idle")
                return

            # Get ASR engine
            engine = get_engine()
            if not engine.is_ready():
                raise NotInitializedError("ASR model not initialized")

            # Transcribe
            import time as time_module

            start_time = time_module.time()
            result = engine.transcribe(processed_audio)
            compute_ms = int((time_module.time() - start_time) * 1000)

            # Post-process
            text = result.text
            text = normalize(text)

            # Apply replacements
            rules = get_current_rules()
            if rules:
                text, _ = process_text(text, rules)

            # Emit result
            emit_transcription_complete(
                session_id=session_id,
                text=text,
                duration_ms=compute_ms,
                confidence=result.confidence,
            )
            emit_status_changed("idle")

        except Exception as e:
            log(f"Transcription error for session {session_id}: {e}")
            error_kind = getattr(e, "code", "E_TRANSCRIBE")
            emit_transcription_error(session_id, error_kind, str(e))
            emit_status_changed("error", str(e))

    # Start background thread
    thread = threading.Thread(target=run_transcription, daemon=True)
    thread.start()
