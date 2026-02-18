"""Tests for notification system and session tracking."""

from __future__ import annotations

import threading
import time
from unittest.mock import MagicMock, patch

import numpy as np
import pytest

from openvoicy_sidecar.notifications import (
    SessionState,
    SessionTracker,
    calculate_audio_levels,
    emit_audio_level,
    emit_status_changed,
    emit_transcription_complete,
    emit_transcription_error,
    get_session_tracker,
    transcribe_session_async,
)


# === Fixtures ===


@pytest.fixture(autouse=True)
def reset_tracker():
    """Reset the global session tracker before each test."""
    # Create a fresh tracker for each test
    import openvoicy_sidecar.notifications as notifications

    notifications._session_tracker = None
    yield
    notifications._session_tracker = None


@pytest.fixture
def mock_write_notification():
    """Mock write_notification to capture emitted events."""
    with patch("openvoicy_sidecar.notifications.write_notification") as mock:
        yield mock


# === Unit Tests: Session Tracker ===


class TestSessionTracker:
    """Tests for SessionTracker."""

    def test_register_new_session(self):
        """Should register new session as pending."""
        tracker = SessionTracker()
        tracker.register("session-1")

        assert tracker.get_state("session-1") == SessionState.PENDING

    def test_mark_completed(self):
        """Should mark session as completed."""
        tracker = SessionTracker()
        tracker.register("session-1")

        result = tracker.mark_completed("session-1")

        assert result is True
        assert tracker.get_state("session-1") == SessionState.COMPLETED

    def test_mark_completed_prevents_duplicate(self):
        """Should prevent duplicate completion."""
        tracker = SessionTracker()
        tracker.register("session-1")

        tracker.mark_completed("session-1")
        result = tracker.mark_completed("session-1")

        assert result is False

    def test_mark_cancelled(self):
        """Should mark session as cancelled."""
        tracker = SessionTracker()
        tracker.register("session-1")

        result = tracker.mark_cancelled("session-1")

        assert result is True
        assert tracker.get_state("session-1") == SessionState.CANCELLED

    def test_mark_cancelled_prevents_completion(self):
        """Should prevent completion of cancelled session."""
        tracker = SessionTracker()
        tracker.register("session-1")

        tracker.mark_cancelled("session-1")
        result = tracker.mark_completed("session-1")

        assert result is False
        assert tracker.get_state("session-1") == SessionState.CANCELLED

    def test_mark_error(self):
        """Should mark session as error."""
        tracker = SessionTracker()
        tracker.register("session-1")

        result = tracker.mark_error("session-1")

        assert result is True
        assert tracker.get_state("session-1") == SessionState.ERROR

    def test_should_emit_pending(self):
        """Should allow emit for pending session."""
        tracker = SessionTracker()
        tracker.register("session-1")

        assert tracker.should_emit("session-1") is True

    def test_should_emit_completed(self):
        """Should not allow emit for completed session."""
        tracker = SessionTracker()
        tracker.register("session-1")
        tracker.mark_completed("session-1")

        assert tracker.should_emit("session-1") is False

    def test_should_emit_cancelled(self):
        """Should not allow emit for cancelled session."""
        tracker = SessionTracker()
        tracker.register("session-1")
        tracker.mark_cancelled("session-1")

        assert tracker.should_emit("session-1") is False

    def test_should_emit_unknown(self):
        """Should not allow emit for unknown session."""
        tracker = SessionTracker()

        assert tracker.should_emit("unknown-session") is False

    def test_cleanup_old_sessions(self):
        """Should clean up old sessions."""
        tracker = SessionTracker(max_age_seconds=1)
        tracker.register("old-session")

        # Wait for expiration
        time.sleep(1.5)

        # Registering new session triggers cleanup
        tracker.register("new-session")

        # Old session should be cleaned up
        assert tracker.get_state("old-session") is None


class TestExactlyOnceSemantics:
    """Tests for exactly-once delivery semantics."""

    def test_only_one_completion_per_session(self, mock_write_notification):
        """Should emit exactly one completion per session."""
        tracker = get_session_tracker()
        tracker.register("session-1")

        result1 = emit_transcription_complete("session-1", "hello", 100)
        result2 = emit_transcription_complete("session-1", "world", 100)

        assert result1 is True
        assert result2 is False
        assert mock_write_notification.call_count == 1

    def test_only_one_error_per_session(self, mock_write_notification):
        """Should emit exactly one error per session."""
        tracker = get_session_tracker()
        tracker.register("session-1")

        result1 = emit_transcription_error("session-1", "E_ERROR", "failed")
        result2 = emit_transcription_error("session-1", "E_ERROR", "failed again")

        assert result1 is True
        assert result2 is False
        assert mock_write_notification.call_count == 1

    def test_no_error_after_completion(self, mock_write_notification):
        """Should not emit error after completion."""
        tracker = get_session_tracker()
        tracker.register("session-1")

        emit_transcription_complete("session-1", "hello", 100)
        result = emit_transcription_error("session-1", "E_ERROR", "failed")

        assert result is False
        # Only the completion notification
        assert mock_write_notification.call_count == 1

    def test_no_completion_after_error(self, mock_write_notification):
        """Should not emit completion after error."""
        tracker = get_session_tracker()
        tracker.register("session-1")

        emit_transcription_error("session-1", "E_ERROR", "failed")
        result = emit_transcription_complete("session-1", "hello", 100)

        assert result is False
        # Only the error notification
        assert mock_write_notification.call_count == 1

    def test_cancelled_emits_nothing(self, mock_write_notification):
        """Cancelled session should emit nothing."""
        tracker = get_session_tracker()
        tracker.register("session-1")
        tracker.mark_cancelled("session-1")

        result1 = emit_transcription_complete("session-1", "hello", 100)
        result2 = emit_transcription_error("session-1", "E_ERROR", "failed")

        assert result1 is False
        assert result2 is False
        assert mock_write_notification.call_count == 0


# === Unit Tests: Audio Level Calculation ===


class TestAudioLevelCalculation:
    """Tests for audio level calculation."""

    def test_calculate_levels_empty(self):
        """Should return zeros for empty audio."""
        rms, peak = calculate_audio_levels(np.array([]))

        assert rms == 0.0
        assert peak == 0.0

    def test_calculate_levels_silence(self):
        """Should return zeros for silent audio."""
        audio = np.zeros(1600, dtype=np.float32)
        rms, peak = calculate_audio_levels(audio)

        assert rms == 0.0
        assert peak == 0.0

    def test_calculate_levels_sine_wave(self):
        """Should calculate correct levels for sine wave."""
        # Generate 1 second of sine wave at 440Hz
        t = np.linspace(0, 1, 16000, dtype=np.float32)
        audio = 0.5 * np.sin(2 * np.pi * 440 * t)  # amplitude 0.5

        rms, peak = calculate_audio_levels(audio)

        # RMS of sine wave with amplitude A is A/sqrt(2) ≈ 0.354
        assert 0.3 < rms < 0.4
        # Peak should be close to 0.5
        assert 0.45 < peak < 0.55

    def test_calculate_levels_normalized(self):
        """Levels should be in 0-1 range."""
        # Audio with values outside [-1, 1]
        audio = np.array([2.0, -2.0, 0.5], dtype=np.float32)

        rms, peak = calculate_audio_levels(audio)

        # Values should be clamped
        assert 0.0 <= rms <= 1.0
        assert 0.0 <= peak <= 1.0


# === Unit Tests: Event Emission ===


class TestEventEmission:
    """Tests for event emission."""

    def test_emit_status_changed(self, mock_write_notification):
        """Should emit status_changed event."""
        emit_status_changed(
            state="recording",
            detail="Recording...",
            progress={"current": 50, "total": 100, "unit": "bytes"},
        )

        mock_write_notification.assert_called_once()
        call_args = mock_write_notification.call_args[0][0]
        assert call_args.method == "event.status_changed"
        assert call_args.params["state"] == "recording"
        assert call_args.params["detail"] == "Recording..."
        assert call_args.params["progress"]["current"] == 50

    def test_emit_audio_level_meter(self, mock_write_notification):
        """Should emit audio_level for meter."""
        emit_audio_level(rms=0.15, peak=0.42, source="meter")

        mock_write_notification.assert_called_once()
        call_args = mock_write_notification.call_args[0][0]
        assert call_args.method == "event.audio_level"
        assert call_args.params["source"] == "meter"
        assert call_args.params["rms"] == 0.15
        assert call_args.params["peak"] == 0.42
        assert "session_id" not in call_args.params

    def test_emit_audio_level_recording(self, mock_write_notification):
        """Should emit audio_level for recording with session_id."""
        emit_audio_level(
            rms=0.15,
            peak=0.42,
            source="recording",
            session_id="session-123",
        )

        mock_write_notification.assert_called_once()
        call_args = mock_write_notification.call_args[0][0]
        assert call_args.params["source"] == "recording"
        assert call_args.params["session_id"] == "session-123"

    def test_emit_transcription_complete(self, mock_write_notification):
        """Should emit transcription_complete event."""
        tracker = get_session_tracker()
        tracker.register("session-1")

        emit_transcription_complete(
            session_id="session-1",
            text="Hello world",
            duration_ms=250,
            confidence=0.95,
        )

        mock_write_notification.assert_called_once()
        call_args = mock_write_notification.call_args[0][0]
        assert call_args.method == "event.transcription_complete"
        assert call_args.params["session_id"] == "session-1"
        assert call_args.params["text"] == "Hello world"
        assert call_args.params["duration_ms"] == 250
        assert call_args.params["confidence"] == 0.95

    def test_emit_transcription_error(self, mock_write_notification):
        """Should emit transcription_error event."""
        tracker = get_session_tracker()
        tracker.register("session-1")

        emit_transcription_error(
            session_id="session-1",
            kind="E_TRANSCRIBE",
            message="Failed to transcribe",
        )

        mock_write_notification.assert_called_once()
        call_args = mock_write_notification.call_args[0][0]
        assert call_args.method == "event.transcription_error"
        assert call_args.params["session_id"] == "session-1"
        assert call_args.params["kind"] == "E_TRANSCRIBE"
        assert call_args.params["message"] == "Failed to transcribe"


# === Thread Safety Tests ===


class TestThreadSafety:
    """Tests for thread safety."""

    def test_concurrent_completion_attempts(self, mock_write_notification):
        """Only one concurrent completion should succeed."""
        tracker = get_session_tracker()
        tracker.register("session-1")

        results = {"success_count": 0}

        def try_complete():
            if emit_transcription_complete("session-1", "hello", 100):
                results["success_count"] += 1

        threads = [threading.Thread(target=try_complete) for _ in range(10)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        # Only one should succeed
        assert results["success_count"] == 1
        assert mock_write_notification.call_count == 1


class ImmediateThread:
    """Thread stub that runs the target immediately."""

    def __init__(self, target, daemon=True):
        self._target = target

    def start(self):
        self._target()


class TestAsyncTranscriptionPipeline:
    """Tests for transcribe_session_async."""

    def test_replacements_result_text_is_string(self):
        """Tuple return from replacements should be unpacked before emit."""
        audio = np.array([0.1, 0.2], dtype=np.float32)

        fake_engine = MagicMock()
        fake_engine.is_ready.return_value = True
        fake_engine.transcribe.return_value = MagicMock(text="raw text", confidence=0.91)

        with (
            patch("openvoicy_sidecar.notifications.threading.Thread", ImmediateThread),
            patch("openvoicy_sidecar.preprocess.preprocess", return_value=audio),
            patch("openvoicy_sidecar.asr.get_engine", return_value=fake_engine),
            patch("openvoicy_sidecar.postprocess.normalize", side_effect=lambda t: t),
            patch("openvoicy_sidecar.replacements.get_current_rules", return_value=[object()]),
            patch("openvoicy_sidecar.replacements.process_text", return_value=("fixed text", False)),
            patch("openvoicy_sidecar.notifications.emit_transcription_complete") as mock_complete,
            patch("openvoicy_sidecar.notifications.emit_status_changed"),
            patch("openvoicy_sidecar.notifications.emit_transcription_error"),
        ):
            transcribe_session_async("session-1", audio, 16000)

        emitted_text = mock_complete.call_args.kwargs["text"]
        assert isinstance(emitted_text, str)
        assert emitted_text == "fixed text"
