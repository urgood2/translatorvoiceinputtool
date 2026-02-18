"""Audio recording with bounded memory buffer.

This module provides audio recording functionality using sounddevice
with a bounded ring buffer to prevent unlimited memory growth during
long recording sessions.

Key Features:
- Bounded buffer: Oldest audio is discarded when max duration is exceeded
- Thread-safe: Audio callback runs in separate thread
- Session tracking: Each recording has a unique session_id
- Clean cancellation: Discard buffered audio without processing

Thread Safety:
- sounddevice runs callbacks in a PortAudio thread
- All buffer access is protected by a Lock
- State transitions are atomic via _lock
"""

from __future__ import annotations

import threading
import time
import uuid
from collections import deque
from dataclasses import dataclass, field
from enum import Enum
from typing import Any

import numpy as np

from .audio import get_active_device_uid, find_device_by_uid, get_default_device
from .protocol import Request, log

# === Constants ===

# Default recording parameters
DEFAULT_SAMPLE_RATE = 16000  # 16kHz - good for ASR
DEFAULT_CHANNELS = 1  # Mono
DEFAULT_MAX_DURATION_SEC = 120  # 2 minutes max recording

# Samples per chunk (callback invocation)
# At 16kHz mono, 1024 samples = 64ms per chunk
CHUNK_SIZE = 1024

# Audio level emission
LEVEL_EMISSION_INTERVAL_MS = 80  # Emit levels every 80ms
LEVEL_BUFFER_SIZE = 1600  # ~100ms of audio at 16kHz


class RecordingState(Enum):
    """Recording session state."""
    IDLE = "idle"
    RECORDING = "recording"
    STOPPING = "stopping"  # Waiting for final callback


@dataclass
class RecordingSession:
    """Represents an active recording session."""

    session_id: str
    started_at: float  # time.monotonic()
    sample_rate: int
    channels: int
    max_samples: int  # Maximum samples to keep (bounded buffer)

    # Audio buffer - bounded deque of numpy arrays
    # Each element is a chunk from the callback
    _buffer: deque = field(default_factory=deque)
    _lock: threading.Lock = field(default_factory=threading.Lock)
    _total_samples: int = 0  # Total samples recorded (including discarded)

    def add_chunk(self, chunk: np.ndarray) -> None:
        """Add an audio chunk to the buffer.

        If the buffer exceeds max_samples, oldest chunks are discarded.
        This operation is thread-safe.
        """
        with self._lock:
            self._buffer.append(chunk.copy())
            self._total_samples += len(chunk)

            # Evict oldest chunks if we exceed max_samples
            current_samples = sum(len(c) for c in self._buffer)
            while current_samples > self.max_samples and len(self._buffer) > 1:
                removed = self._buffer.popleft()
                current_samples -= len(removed)

    def get_audio(self) -> np.ndarray:
        """Get all buffered audio as a single array.

        Returns empty array if no audio buffered.
        This operation is thread-safe.
        """
        with self._lock:
            if not self._buffer:
                return np.array([], dtype=np.float32)
            return np.concatenate(list(self._buffer))

    def get_duration_ms(self) -> int:
        """Get duration of buffered audio in milliseconds."""
        with self._lock:
            current_samples = sum(len(c) for c in self._buffer)
            return int(current_samples * 1000 / self.sample_rate)

    def clear(self) -> None:
        """Clear the audio buffer."""
        with self._lock:
            self._buffer.clear()
            self._total_samples = 0


class AudioRecorder:
    """Thread-safe audio recorder with bounded memory buffer.

    Usage:
        recorder = AudioRecorder()
        session_id = recorder.start()
        # ... wait for user to stop ...
        audio_data, duration_ms = recorder.stop(session_id)
        # Or cancel:
        recorder.cancel(session_id)
    """

    def __init__(
        self,
        sample_rate: int = DEFAULT_SAMPLE_RATE,
        channels: int = DEFAULT_CHANNELS,
        max_duration_sec: float = DEFAULT_MAX_DURATION_SEC,
    ):
        self.sample_rate = sample_rate
        self.channels = channels
        self.max_duration_sec = max_duration_sec
        self.max_samples = int(sample_rate * max_duration_sec)

        self._state = RecordingState.IDLE
        self._session: RecordingSession | None = None
        self._stream: Any = None  # sounddevice.InputStream
        self._lock = threading.Lock()

        # Audio level emission state
        self._level_buffer: deque = deque(maxlen=LEVEL_BUFFER_SIZE)
        self._level_thread: threading.Thread | None = None
        self._emit_levels = False

    @property
    def state(self) -> RecordingState:
        """Get current recording state."""
        return self._state

    @property
    def session_id(self) -> str | None:
        """Get current session ID, or None if not recording."""
        return self._session.session_id if self._session else None

    def start(self, device_uid: str | None = None, session_id: str | None = None) -> str:
        """Start a new recording session.

        Args:
            device_uid: Device to record from, or None for active/default device.
            session_id: Optional externally provided session ID.

        Returns:
            Session ID for this recording.

        Raises:
            RuntimeError: If already recording.
            ValueError: If device not found.
            OSError: If audio device cannot be opened.
        """
        with self._lock:
            if self._state != RecordingState.IDLE:
                raise RuntimeError("Recording already in progress")

            # Resolve device
            if device_uid is None:
                # Use active device, or default if none set
                device_uid = get_active_device_uid()

            device_index = None
            if device_uid is not None:
                device = find_device_by_uid(device_uid)
                if device is None:
                    raise ValueError(f"Device not found: {device_uid}")
                # We need to find the sounddevice index
                device_index = self._get_device_index(device_uid)

            # Create new session
            session_id = session_id or str(uuid.uuid4())
            self._session = RecordingSession(
                session_id=session_id,
                started_at=time.monotonic(),
                sample_rate=self.sample_rate,
                channels=self.channels,
                max_samples=self.max_samples,
            )

            # Start audio stream
            try:
                import sounddevice as sd

                self._stream = sd.InputStream(
                    samplerate=self.sample_rate,
                    channels=self.channels,
                    dtype=np.float32,
                    blocksize=CHUNK_SIZE,
                    device=device_index,
                    callback=self._audio_callback,
                )
                self._stream.start()
                self._state = RecordingState.RECORDING

                # Start level emission thread
                self._emit_levels = True
                self._level_buffer.clear()
                self._level_thread = threading.Thread(target=self._level_emit_loop, daemon=True)
                self._level_thread.start()

                log(f"Recording started: session={session_id}, device={device_uid or 'default'}")
                return session_id

            except Exception as e:
                # Clean up on failure
                self._session = None
                self._stream = None
                log(f"Failed to start recording: {e}")
                raise OSError(f"Failed to open audio device: {e}") from e

    def stop(self, session_id: str) -> tuple[np.ndarray, int]:
        """Stop recording and return the audio data.

        Args:
            session_id: Session ID from start().

        Returns:
            Tuple of (audio_data, duration_ms).

        Raises:
            RuntimeError: If not recording or wrong session.
        """
        with self._lock:
            if self._state != RecordingState.RECORDING:
                raise RuntimeError("Not recording")

            if self._session is None or self._session.session_id != session_id:
                raise RuntimeError(f"Invalid session ID: {session_id}")

            self._state = RecordingState.STOPPING

        # Stop level emission
        self._emit_levels = False
        if self._level_thread is not None:
            self._level_thread.join(timeout=0.5)
            self._level_thread = None

        # Stop stream outside lock to avoid deadlock with callback
        if self._stream is not None:
            self._stream.stop()
            self._stream.close()
            self._stream = None

        with self._lock:
            # Get audio data
            audio_data = self._session.get_audio()
            duration_ms = self._session.get_duration_ms()

            log(f"Recording stopped: session={session_id}, duration={duration_ms}ms, samples={len(audio_data)}")

            # Reset state
            self._session = None
            self._state = RecordingState.IDLE

            return audio_data, duration_ms

    def cancel(self, session_id: str) -> None:
        """Cancel recording and discard audio data.

        Args:
            session_id: Session ID from start().

        Raises:
            RuntimeError: If not recording or wrong session.
        """
        with self._lock:
            if self._state != RecordingState.RECORDING:
                raise RuntimeError("Not recording")

            if self._session is None or self._session.session_id != session_id:
                raise RuntimeError(f"Invalid session ID: {session_id}")

            self._state = RecordingState.STOPPING

        # Stop level emission
        self._emit_levels = False
        if self._level_thread is not None:
            self._level_thread.join(timeout=0.5)
            self._level_thread = None

        # Stop stream outside lock
        if self._stream is not None:
            self._stream.stop()
            self._stream.close()
            self._stream = None

        with self._lock:
            log(f"Recording cancelled: session={session_id}")

            # Discard audio
            self._session = None
            self._state = RecordingState.IDLE

    def get_status(self) -> dict[str, Any]:
        """Get current recording status."""
        with self._lock:
            status = {
                "state": self._state.value,
                "session_id": self._session.session_id if self._session else None,
            }

            if self._session and self._state == RecordingState.RECORDING:
                status["duration_ms"] = self._session.get_duration_ms()
                status["elapsed_sec"] = time.monotonic() - self._session.started_at

            return status

    def _audio_callback(
        self,
        indata: np.ndarray,
        frames: int,
        time_info: Any,
        status: Any,
    ) -> None:
        """sounddevice callback - runs in PortAudio thread."""
        if status:
            log(f"Audio callback status: {status}")

        # Check if we should accept data
        with self._lock:
            if self._state != RecordingState.RECORDING or self._session is None:
                return
            session = self._session

        # Extract mono data
        mono = indata[:, 0] if indata.ndim > 1 else indata.flatten()

        # Add data outside main lock (session has its own lock)
        session.add_chunk(mono)

        # Also add to level buffer for emission
        self._level_buffer.extend(mono)

    def _level_emit_loop(self) -> None:
        """Background loop that emits audio level events during recording."""
        from .notifications import calculate_audio_levels, emit_audio_level

        while self._emit_levels:
            time.sleep(LEVEL_EMISSION_INTERVAL_MS / 1000)

            if not self._emit_levels:
                break

            # Get session ID and buffer snapshot
            with self._lock:
                if self._session is None:
                    continue
                session_id = self._session.session_id

            # Calculate levels from buffer
            if self._level_buffer:
                audio = np.array(list(self._level_buffer), dtype=np.float32)
                rms, peak = calculate_audio_levels(audio)
                emit_audio_level(
                    rms=rms,
                    peak=peak,
                    source="recording",
                    session_id=session_id,
                )

    def _get_device_index(self, device_uid: str) -> int | None:
        """Get sounddevice device index from our UID.

        Returns None to use default device.
        """
        try:
            import sounddevice as sd

            devices = sd.query_devices()
            host_apis = sd.query_hostapis()

            if isinstance(devices, dict):
                devices = [devices]

            from .audio import _generate_stable_uid

            for idx, device in enumerate(devices):
                if device.get("max_input_channels", 0) <= 0:
                    continue

                host_api_index = device.get("hostapi", 0)
                host_api_name = "unknown"
                if isinstance(host_apis, list) and host_api_index < len(host_apis):
                    host_api_name = host_apis[host_api_index].get("name", "unknown")

                uid = _generate_stable_uid(device, host_api_name)
                if uid == device_uid:
                    return idx

            return None
        except Exception as e:
            log(f"Error getting device index: {e}")
            return None


# === Global Recorder Instance ===

_recorder: AudioRecorder | None = None


def get_recorder() -> AudioRecorder:
    """Get the global AudioRecorder instance."""
    global _recorder
    if _recorder is None:
        _recorder = AudioRecorder()
    return _recorder


# === JSON-RPC Handlers ===


class RecordingError(Exception):
    """Recording error with error code."""

    def __init__(self, message: str, code: str = "E_RECORDING"):
        self.message = message
        self.code = code
        super().__init__(message)


class AlreadyRecordingError(RecordingError):
    """Raised when trying to start recording while already recording."""

    def __init__(self, message: str = "Already recording"):
        super().__init__(message, "E_ALREADY_RECORDING")


class NotRecordingError(RecordingError):
    """Raised when trying to stop/cancel while not recording."""

    def __init__(self, message: str = "Not recording"):
        super().__init__(message, "E_NOT_RECORDING")


class InvalidSessionError(RecordingError):
    """Raised when session ID doesn't match."""

    def __init__(self, message: str = "Invalid session ID"):
        super().__init__(message, "E_INVALID_SESSION")


def handle_recording_start(request: Request) -> dict[str, Any]:
    """Handle recording.start request.

    Params:
        device_uid: Optional device UID to record from.
        session_id: Optional externally provided session identifier.

    Returns:
        session_id: Unique session identifier.

    Errors:
        E_ALREADY_RECORDING: Already recording.
        E_DEVICE_NOT_FOUND: Device not found.
        E_AUDIO_IO: Failed to open audio device.
    """
    device_uid = request.params.get("device_uid")
    session_id = request.params.get("session_id")

    recorder = get_recorder()

    try:
        if session_id:
            started_session_id = recorder.start(device_uid, session_id=session_id)
        else:
            started_session_id = recorder.start(device_uid)
        return {"session_id": started_session_id}
    except RuntimeError as e:
        if "already" in str(e).lower():
            raise AlreadyRecordingError(str(e))
        raise RecordingError(str(e))
    except ValueError as e:
        from .audio import DeviceNotFoundError
        raise DeviceNotFoundError(str(e), device_uid)
    except OSError as e:
        raise RecordingError(str(e), "E_AUDIO_IO")


def handle_recording_stop(request: Request) -> dict[str, Any]:
    """Handle recording.stop request.

    Params:
        session_id: Session ID from recording.start.

    Returns:
        audio_duration_ms: Duration of recorded audio in milliseconds.
        sample_rate: Sample rate of the audio.
        channels: Number of channels.

    Errors:
        E_NOT_RECORDING: Not currently recording.
        E_INVALID_SESSION: Session ID doesn't match.
    """
    session_id = request.params.get("session_id")

    if not session_id:
        raise InvalidSessionError("session_id is required")

    recorder = get_recorder()

    try:
        audio_data, duration_ms = recorder.stop(session_id)

        # Emit status change
        from .notifications import emit_status_changed

        emit_status_changed("transcribing", "Processing audio...")

        # Start async transcription (this returns immediately)
        from .notifications import transcribe_session_async

        transcribe_session_async(session_id, audio_data, recorder.sample_rate)

        return {
            "audio_duration_ms": duration_ms,
            "sample_rate": recorder.sample_rate,
            "channels": recorder.channels,
            "session_id": session_id,
        }
    except RuntimeError as e:
        error_msg = str(e).lower()
        if "not recording" in error_msg:
            raise NotRecordingError(str(e))
        if "invalid session" in error_msg or "session id" in error_msg:
            raise InvalidSessionError(str(e))
        raise RecordingError(str(e))


def handle_recording_cancel(request: Request) -> dict[str, Any]:
    """Handle recording.cancel request.

    Params:
        session_id: Session ID from recording.start.

    Returns:
        cancelled: True if successfully cancelled.

    Errors:
        E_NOT_RECORDING: Not currently recording.
        E_INVALID_SESSION: Session ID doesn't match.
    """
    session_id = request.params.get("session_id")

    if not session_id:
        raise InvalidSessionError("session_id is required")

    recorder = get_recorder()

    try:
        recorder.cancel(session_id)
        cleared_pending_audio = clear_pending_audio(session_id)

        # Mark session as cancelled to prevent any notifications
        from .notifications import get_session_tracker

        tracker = get_session_tracker()
        tracker.mark_cancelled(session_id)
        log(
            f"Recording cancel cleanup: session={session_id}, "
            f"pending_audio_cleared={cleared_pending_audio}"
        )

        return {"cancelled": True, "session_id": session_id}
    except RuntimeError as e:
        error_msg = str(e).lower()
        if "not recording" in error_msg:
            raise NotRecordingError(str(e))
        if "invalid session" in error_msg or "session id" in error_msg:
            raise InvalidSessionError(str(e))
        raise RecordingError(str(e))


def handle_recording_status(request: Request) -> dict[str, Any]:
    """Handle recording.status request.

    Returns current recording state and session info.
    """
    recorder = get_recorder()
    return recorder.get_status()


# === Audio Storage for Transcription ===

# Simple storage for audio data pending transcription
# In production, this could be more sophisticated (e.g., temp files)
_pending_audio: dict[str, tuple[np.ndarray, int]] = {}
_pending_audio_lock = threading.Lock()


def _store_audio_for_transcription(session_id: str, audio: np.ndarray, sample_rate: int) -> None:
    """Store audio data for later transcription."""
    with _pending_audio_lock:
        _pending_audio[session_id] = (audio, sample_rate)


def get_pending_audio(session_id: str) -> tuple[np.ndarray, int] | None:
    """Get and consume pending audio for transcription.

    Returns None if no audio for this session.
    """
    with _pending_audio_lock:
        return _pending_audio.pop(session_id, None)


def clear_pending_audio(session_id: str) -> bool:
    """Clear pending audio without consuming it."""
    with _pending_audio_lock:
        if session_id in _pending_audio:
            del _pending_audio[session_id]
            return True
        return False
