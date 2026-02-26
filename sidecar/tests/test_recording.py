"""Tests for audio recording with bounded memory buffer."""

from __future__ import annotations

import json
import subprocess
import sys
import threading
import time
from contextlib import suppress
from typing import Any
from unittest.mock import MagicMock, patch

import numpy as np
import pytest

from openvoicy_sidecar.preprocess import TARGET_SAMPLE_RATE
from openvoicy_sidecar.protocol import Request
from openvoicy_sidecar.recording import (
    AlreadyRecordingError,
    AudioRecorder,
    InvalidSessionError,
    NotRecordingError,
    RecordingError,
    RecordingSession,
    RecordingState,
    clear_pending_audio,
    get_pending_audio,
    get_recorder,
    handle_recording_cancel,
    handle_recording_start,
    handle_recording_status,
    handle_recording_stop,
    LEVEL_THREAD_JOIN_TIMEOUT_SEC,
)

# === Fixtures ===


@pytest.fixture
def mock_sounddevice():
    """Mock sounddevice module for testing."""
    mock_sd = MagicMock()

    class MockInputStream:
        """Mock InputStream that simulates audio capture."""

        def __init__(self, **kwargs):
            self.kwargs = kwargs
            self.callback = kwargs.get("callback")
            self.running = False

        def start(self):
            self.running = True

        def stop(self):
            self.running = False

        def close(self):
            pass

    mock_sd.InputStream = MockInputStream
    mock_sd.query_devices.return_value = [
        {
            "name": "Test Microphone",
            "hostapi": 0,
            "max_input_channels": 2,
            "max_output_channels": 0,
            "default_samplerate": 48000.0,
        }
    ]
    mock_sd.query_hostapis.return_value = [{"name": "TestAPI"}]
    mock_sd.default.device = (0, None)

    return mock_sd


@pytest.fixture
def recorder(mock_sounddevice):
    """Create a fresh AudioRecorder with mocked sounddevice."""
    with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
        r = AudioRecorder(
            sample_rate=16000,
            channels=1,
            max_duration_sec=10,  # 10 seconds max for testing
        )
        yield r

        # Cleanup: make sure recording is stopped
        if r.state == RecordingState.RECORDING:
            with suppress(Exception):
                r.cancel(r.session_id)


@pytest.fixture
def reset_global_recorder():
    """Reset the global recorder after each test."""
    import openvoicy_sidecar.recording as rec_module

    original = rec_module._recorder
    rec_module._recorder = None
    yield
    rec_module._recorder = original


# === Unit Tests: RecordingSession ===


class TestRecordingSession:
    """Tests for RecordingSession class."""

    def test_create_session(self):
        """Should create session with initial state."""
        session = RecordingSession(
            session_id="test-123",
            started_at=time.monotonic(),
            sample_rate=16000,
            channels=1,
            max_samples=160000,  # 10 seconds
        )

        assert session.session_id == "test-123"
        assert session.sample_rate == 16000
        assert session.channels == 1

    def test_add_and_get_audio(self):
        """Should store and retrieve audio chunks."""
        session = RecordingSession(
            session_id="test",
            started_at=time.monotonic(),
            sample_rate=16000,
            channels=1,
            max_samples=160000,
        )

        # Add some audio
        chunk1 = np.array([0.1, 0.2, 0.3], dtype=np.float32)
        chunk2 = np.array([0.4, 0.5, 0.6], dtype=np.float32)

        session.add_chunk(chunk1)
        session.add_chunk(chunk2)

        audio = session.get_audio()
        expected = np.array([0.1, 0.2, 0.3, 0.4, 0.5, 0.6], dtype=np.float32)

        np.testing.assert_array_almost_equal(audio, expected)

    def test_get_duration_ms(self):
        """Should calculate duration correctly."""
        session = RecordingSession(
            session_id="test",
            started_at=time.monotonic(),
            sample_rate=16000,
            channels=1,
            max_samples=160000,
        )

        # Add 1 second of audio (16000 samples at 16kHz)
        chunk = np.zeros(16000, dtype=np.float32)
        session.add_chunk(chunk)

        assert session.get_duration_ms() == 1000

    def test_bounded_buffer_evicts_oldest(self):
        """Should evict oldest chunks when max_samples exceeded."""
        session = RecordingSession(
            session_id="test",
            started_at=time.monotonic(),
            sample_rate=16000,
            channels=1,
            max_samples=100,  # Very small buffer for testing
        )

        # Add chunks that exceed the buffer
        for i in range(20):
            chunk = np.full(10, i, dtype=np.float32)
            session.add_chunk(chunk)

        audio = session.get_audio()

        # Should have at most 100 samples
        assert len(audio) <= 100

        # Oldest chunks should be evicted, newest should remain
        # The last chunk (value 19) should still be present
        assert 19.0 in audio

    def test_clear_buffer(self):
        """Should clear all audio data."""
        session = RecordingSession(
            session_id="test",
            started_at=time.monotonic(),
            sample_rate=16000,
            channels=1,
            max_samples=160000,
        )

        chunk = np.array([0.1, 0.2, 0.3], dtype=np.float32)
        session.add_chunk(chunk)
        assert len(session.get_audio()) > 0

        session.clear()
        assert len(session.get_audio()) == 0

    def test_thread_safety(self):
        """Should handle concurrent access safely."""
        session = RecordingSession(
            session_id="test",
            started_at=time.monotonic(),
            sample_rate=16000,
            channels=1,
            max_samples=16000,  # 1 second
        )

        errors = []

        def writer():
            """Write chunks in a loop."""
            try:
                for _ in range(100):
                    chunk = np.random.random(160).astype(np.float32)
                    session.add_chunk(chunk)
                    time.sleep(0.001)
            except Exception as e:
                errors.append(e)

        def reader():
            """Read audio in a loop."""
            try:
                for _ in range(100):
                    _ = session.get_audio()
                    _ = session.get_duration_ms()
                    time.sleep(0.001)
            except Exception as e:
                errors.append(e)

        threads = [
            threading.Thread(target=writer),
            threading.Thread(target=reader),
        ]

        for t in threads:
            t.start()

        for t in threads:
            t.join()

        assert len(errors) == 0


# === Unit Tests: AudioRecorder ===


class TestAudioRecorder:
    """Tests for AudioRecorder class."""

    def test_initial_state(self, recorder):
        """Should start in IDLE state."""
        assert recorder.state == RecordingState.IDLE
        assert recorder.session_id is None

    def test_start_recording(self, recorder, mock_sounddevice):
        """Should start recording and return session ID."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            session_id = recorder.start()

            assert session_id is not None
            assert recorder.state == RecordingState.RECORDING
            assert recorder.session_id == session_id

    def test_start_already_recording_raises_error(self, recorder, mock_sounddevice):
        """Should raise error if already recording."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            recorder.start()

            with pytest.raises(RuntimeError, match="already"):
                recorder.start()

    def test_stop_recording(self, recorder, mock_sounddevice):
        """Should stop recording and return audio data."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            session_id = recorder.start()

            # Simulate some audio data (1600 samples = 100ms at 16kHz)
            if recorder._session:
                chunk = np.zeros(1600, dtype=np.float32)
                recorder._session.add_chunk(chunk)

            audio, duration_ms = recorder.stop(session_id)

            assert recorder.state == RecordingState.IDLE
            assert recorder.session_id is None
            assert len(audio) == 1600
            assert duration_ms == int(1600 * 1000 / recorder.sample_rate)

    def test_start_uses_native_device_sample_rate_and_channels(
        self, recorder, mock_sounddevice
    ):
        """Recorder should capture at selected/default device native format."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            recorder.start()

            assert recorder.sample_rate == 48000
            assert recorder.channels == 2
            assert recorder._stream is not None
            assert recorder._stream.kwargs["samplerate"] == 48000
            assert recorder._stream.kwargs["channels"] == 2

    def test_stop_not_recording_raises_error(self, recorder):
        """Should raise error if not recording."""
        with pytest.raises(RuntimeError, match="Not recording"):
            recorder.stop("some-session")

    def test_stop_wrong_session_raises_error(self, recorder, mock_sounddevice):
        """Should raise error if session ID doesn't match."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            recorder.start()

            with pytest.raises(RuntimeError, match="Invalid session"):
                recorder.stop("wrong-session-id")

    def test_stop_level_thread_join_timeout_is_bounded(self, recorder):
        """Stop should not block on level thread join beyond async-stop target."""
        session_id = "session-stop-timeout"
        recorder._state = RecordingState.RECORDING
        recorder._session = RecordingSession(
            session_id=session_id,
            started_at=time.monotonic(),
            sample_rate=recorder.sample_rate,
            channels=recorder.channels,
            max_samples=recorder.max_samples,
        )
        recorder._session.add_chunk(np.zeros(1600, dtype=np.float32))
        recorder._stream = MagicMock()

        fake_level_thread = MagicMock()
        fake_level_thread.is_alive.return_value = False
        recorder._level_thread = fake_level_thread
        recorder._emit_levels = True

        audio, duration_ms = recorder.stop(session_id)

        assert len(audio) == 1600
        assert duration_ms == 100
        fake_level_thread.join.assert_called_once()
        join_call = fake_level_thread.join.call_args
        timeout = join_call.kwargs.get("timeout")
        if timeout is None and join_call.args:
            timeout = join_call.args[0]
        assert timeout is not None
        assert timeout == LEVEL_THREAD_JOIN_TIMEOUT_SEC
        assert timeout < 0.25

    def test_cancel_recording(self, recorder, mock_sounddevice):
        """Should cancel recording and discard audio."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            session_id = recorder.start()
            recorder.cancel(session_id)

            assert recorder.state == RecordingState.IDLE
            assert recorder.session_id is None

    def test_stop_propagates_audio_callback_error(self, recorder, mock_sounddevice):
        """Stop should raise OSError when callback reported audio I/O failure."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            session_id = recorder.start()
            recorder._audio_callback(
                np.zeros((64, recorder.channels), dtype=np.float32),
                64,
                None,
                "Input overflowed / device disconnected",
            )

            with pytest.raises(OSError, match="Audio I/O error"):
                recorder.stop(session_id)

            assert recorder.state == RecordingState.IDLE
            assert recorder.session_id is None

    def test_cancel_not_recording_raises_error(self, recorder):
        """Should raise error if not recording."""
        with pytest.raises(RuntimeError, match="Not recording"):
            recorder.cancel("some-session")

    def test_get_status_idle(self, recorder):
        """Should return idle status."""
        status = recorder.get_status()
        assert status["state"] == "idle"
        assert status["session_id"] is None

    def test_get_status_recording(self, recorder, mock_sounddevice):
        """Should return recording status with duration."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            session_id = recorder.start()

            status = recorder.get_status()
            assert status["state"] == "recording"
            assert status["session_id"] == session_id
            assert "duration_ms" in status


# === Unit Tests: JSON-RPC Handlers ===


class TestRecordingHandlers:
    """Tests for JSON-RPC handler functions."""

    @pytest.fixture
    def mock_recorder(self):
        """Create a mock recorder."""
        mock = MagicMock()
        mock.state = RecordingState.IDLE
        mock.session_id = None
        mock.sample_rate = 16000
        mock.channels = 1
        return mock

    def test_handle_recording_start(self, mock_sounddevice, reset_global_recorder):
        """Should start recording and return session ID."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            request = Request(method="recording.start", id=1)
            result = handle_recording_start(request)

            assert "session_id" in result
            assert result["session_id"] is not None

            # Cleanup
            recorder = get_recorder()
            if recorder.state == RecordingState.RECORDING:
                recorder.cancel(recorder.session_id)

    def test_handle_recording_start_with_provided_session_id(
        self, mock_sounddevice, reset_global_recorder
    ):
        """Should honor externally provided session ID."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            request = Request(
                method="recording.start",
                id=1,
                params={"session_id": "rust-session-123"},
            )
            result = handle_recording_start(request)

            assert result["session_id"] == "rust-session-123"

            # Cleanup
            recorder = get_recorder()
            if recorder.state == RecordingState.RECORDING:
                recorder.cancel(recorder.session_id)

    def test_handle_recording_start_emits_recording_status_changed(
        self, mock_sounddevice, reset_global_recorder
    ):
        """Should emit recording status when start succeeds."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            with patch("openvoicy_sidecar.notifications.emit_status_changed") as mock_status_changed:
                result = handle_recording_start(Request(method="recording.start", id=1))

            assert "session_id" in result
            mock_status_changed.assert_called_once_with(
                "recording",
                "Recording in progress...",
            )

            # Cleanup
            recorder = get_recorder()
            if recorder.state == RecordingState.RECORDING:
                recorder.cancel(recorder.session_id)

    def test_handle_recording_start_already_recording(
        self, mock_sounddevice, reset_global_recorder
    ):
        """Should raise error if already recording."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            request1 = Request(method="recording.start", id=1)
            handle_recording_start(request1)

            request2 = Request(method="recording.start", id=2)
            with pytest.raises(AlreadyRecordingError):
                handle_recording_start(request2)

            # Cleanup
            recorder = get_recorder()
            if recorder.state == RecordingState.RECORDING:
                recorder.cancel(recorder.session_id)

    def test_handle_recording_stop(self, mock_sounddevice, reset_global_recorder):
        """Should stop recording and return duration."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            start_request = Request(method="recording.start", id=1)
            start_result = handle_recording_start(start_request)
            session_id = start_result["session_id"]

            stop_request = Request(
                method="recording.stop",
                id=2,
                params={"session_id": session_id},
            )
            result = handle_recording_stop(stop_request)

            assert "audio_duration_ms" in result
            assert "sample_rate" in result
            assert result["session_id"] == session_id

    def test_handle_recording_stop_preprocesses_before_transcribe(
        self, mock_sounddevice, reset_global_recorder
    ):
        """Stop should preprocess audio and pass processed audio to transcription."""
        processed_audio = np.array([0.42, -0.42], dtype=np.float32)
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            start_request = Request(method="recording.start", id=1)
            start_result = handle_recording_start(start_request)
            session_id = start_result["session_id"]

            with (
                patch(
                    "openvoicy_sidecar.preprocess.preprocess_audio",
                    return_value=processed_audio,
                ) as mock_preprocess,
                patch("openvoicy_sidecar.notifications.emit_status_changed"),
                patch("openvoicy_sidecar.notifications.transcribe_session_async") as mock_transcribe,
            ):
                handle_recording_stop(
                    Request(
                        method="recording.stop",
                        id=2,
                        params={"session_id": session_id},
                    )
                )

        preprocess_args, preprocess_kwargs = mock_preprocess.call_args
        assert preprocess_args[0].dtype == np.float32
        assert preprocess_kwargs == {}
        assert preprocess_args[1]["input_sample_rate"] == 48000
        assert preprocess_args[1]["target_sample_rate"] == TARGET_SAMPLE_RATE
        assert preprocess_args[1]["normalize"] is False
        assert preprocess_args[1]["audio"]["trim_silence"] is True

        mock_transcribe.assert_called_once_with(
            session_id,
            processed_audio,
            TARGET_SAMPLE_RATE,
        )

    def test_handle_recording_stop_reports_audio_io_from_callback(
        self, mock_sounddevice, reset_global_recorder
    ):
        """Handler should map callback I/O failures to E_AUDIO_IO."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            start_request = Request(method="recording.start", id=1)
            start_result = handle_recording_start(start_request)
            session_id = start_result["session_id"]

            recorder = get_recorder()
            recorder._audio_callback(
                np.zeros((64, recorder.channels), dtype=np.float32),
                64,
                None,
                "Device disconnected",
            )

            with pytest.raises(RecordingError) as exc_info:
                handle_recording_stop(
                    Request(
                        method="recording.stop",
                        id=2,
                        params={"session_id": session_id},
                    )
                )

            assert exc_info.value.code == "E_AUDIO_IO"

    def test_handle_recording_stop_not_recording(self, reset_global_recorder):
        """Should raise error if not recording."""
        request = Request(
            method="recording.stop",
            id=1,
            params={"session_id": "some-session"},
        )

        with pytest.raises(NotRecordingError):
            handle_recording_stop(request)

    def test_handle_recording_stop_missing_session_id(
        self, mock_sounddevice, reset_global_recorder
    ):
        """Should raise error if session_id missing."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            start_request = Request(method="recording.start", id=1)
            handle_recording_start(start_request)

            stop_request = Request(method="recording.stop", id=2)

            with pytest.raises(InvalidSessionError):
                handle_recording_stop(stop_request)

            # Cleanup
            recorder = get_recorder()
            if recorder.state == RecordingState.RECORDING:
                recorder.cancel(recorder.session_id)

    def test_handle_recording_cancel(self, mock_sounddevice, reset_global_recorder):
        """Should cancel recording."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            start_request = Request(method="recording.start", id=1)
            start_result = handle_recording_start(start_request)
            session_id = start_result["session_id"]

            cancel_request = Request(
                method="recording.cancel",
                id=2,
                params={"session_id": session_id},
            )
            result = handle_recording_cancel(cancel_request)

            assert result["cancelled"] is True
            assert result["session_id"] == session_id

    def test_handle_recording_cancel_when_idle_raises_not_recording(
        self, reset_global_recorder
    ):
        """Cancel while idle should return NotRecordingError, not crash."""
        request = Request(
            method="recording.cancel",
            id=1,
            params={"session_id": "nonexistent"},
        )

        with pytest.raises(NotRecordingError):
            handle_recording_cancel(request)

    def test_handle_recording_cancel_mismatched_session_id(
        self, mock_sounddevice, reset_global_recorder
    ):
        """Cancel with wrong session_id should raise InvalidSessionError."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            start_request = Request(method="recording.start", id=1)
            start_result = handle_recording_start(start_request)

            with pytest.raises(InvalidSessionError):
                handle_recording_cancel(
                    Request(
                        method="recording.cancel",
                        id=2,
                        params={"session_id": "wrong-session-id"},
                    )
                )

            recorder = get_recorder()
            # Active recording should still be associated with the original session.
            assert recorder.session_id == start_result["session_id"]

            # Cleanup
            if recorder.state == RecordingState.RECORDING:
                recorder.cancel(recorder.session_id)

    def test_handle_recording_cancel_does_not_trigger_transcription(
        self, mock_sounddevice, reset_global_recorder
    ):
        """Cancel should not transcribe and should emit idle status change."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            start_request = Request(method="recording.start", id=1)
            start_result = handle_recording_start(start_request)
            session_id = start_result["session_id"]

            with (
                patch("openvoicy_sidecar.notifications.transcribe_session_async") as mock_transcribe,
                patch("openvoicy_sidecar.notifications.emit_status_changed") as mock_status_changed,
            ):
                result = handle_recording_cancel(
                    Request(
                        method="recording.cancel",
                        id=2,
                        params={"session_id": session_id},
                    )
                )

            assert result["cancelled"] is True
            assert result["session_id"] == session_id
            assert get_pending_audio(session_id) is None
            mock_transcribe.assert_not_called()
            mock_status_changed.assert_called_once_with("idle", "Ready")

    def test_handle_recording_status(self, mock_sounddevice, reset_global_recorder):
        """Should return current status."""
        with patch.dict("sys.modules", {"sounddevice": mock_sounddevice}):
            # Idle status
            request = Request(method="recording.status", id=1)
            result = handle_recording_status(request)
            assert result["state"] == "idle"

            # Recording status
            start_request = Request(method="recording.start", id=2)
            start_result = handle_recording_start(start_request)

            result = handle_recording_status(request)
            assert result["state"] == "recording"
            assert result["session_id"] == start_result["session_id"]

            # Cleanup
            recorder = get_recorder()
            if recorder.state == RecordingState.RECORDING:
                recorder.cancel(recorder.session_id)


# === Unit Tests: Pending Audio Storage ===


class TestPendingAudioStorage:
    """Tests for pending audio storage functions."""

    def test_store_and_get_audio(self):
        """Should store and retrieve pending audio."""
        from openvoicy_sidecar.recording import _store_audio_for_transcription

        audio = np.array([0.1, 0.2, 0.3], dtype=np.float32)
        _store_audio_for_transcription("test-session", audio, 16000)

        result = get_pending_audio("test-session")
        assert result is not None
        stored_audio, sample_rate = result
        np.testing.assert_array_almost_equal(stored_audio, audio)
        assert sample_rate == 16000

        # Should be consumed (no longer available)
        assert get_pending_audio("test-session") is None

    def test_clear_pending_audio(self):
        """Should clear pending audio without consuming."""
        from openvoicy_sidecar.recording import _store_audio_for_transcription

        audio = np.array([0.1, 0.2, 0.3], dtype=np.float32)
        _store_audio_for_transcription("test-session-2", audio, 16000)

        assert clear_pending_audio("test-session-2") is True
        assert get_pending_audio("test-session-2") is None
        assert clear_pending_audio("test-session-2") is False


# === Integration Tests ===


class TestRecordingIntegration:
    """Integration tests that run the actual sidecar."""

    @pytest.fixture
    def sidecar_process(self):
        """Start a sidecar process for integration testing."""
        proc = subprocess.Popen(
            [sys.executable, "-m", "openvoicy_sidecar"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        yield proc
        # Cleanup
        if proc.poll() is None:
            proc.terminate()
            proc.wait(timeout=5)

    def _send_request(
        self, proc, method: str, params: dict[str, Any] | None = None, req_id: int = 1
    ) -> dict[str, Any]:
        """Send a JSON-RPC request and get the response."""
        request: dict[str, Any] = {"jsonrpc": "2.0", "id": req_id, "method": method}
        if params:
            request["params"] = params

        proc.stdin.write(json.dumps(request) + "\n")
        proc.stdin.flush()

        response_line = proc.stdout.readline()
        return json.loads(response_line)

    def test_recording_status_integration(self, sidecar_process):
        """Integration test: recording.status should work."""
        response = self._send_request(sidecar_process, "recording.status")

        assert response.get("jsonrpc") == "2.0"
        assert "result" in response
        assert response["result"]["state"] == "idle"

    def test_recording_stop_without_start_integration(self, sidecar_process):
        """Integration test: stop without start should error."""
        response = self._send_request(
            sidecar_process,
            "recording.stop",
            {"session_id": "nonexistent"},
        )

        assert response.get("jsonrpc") == "2.0"
        assert "error" in response
        assert response["error"]["data"]["kind"] == "E_NOT_RECORDING"

    def test_recording_cancel_without_start_integration(self, sidecar_process):
        """Integration test: cancel without start should error."""
        response = self._send_request(
            sidecar_process,
            "recording.cancel",
            {"session_id": "nonexistent"},
        )

        assert response.get("jsonrpc") == "2.0"
        assert "error" in response
        assert response["error"]["data"]["kind"] == "E_NOT_RECORDING"

    @pytest.mark.skipif(
        not any([
            sys.platform == "darwin",  # macOS usually has audio
            # Linux CI typically doesn't have audio devices
        ]),
        reason="Audio devices may not be available in CI",
    )
    def test_full_recording_cycle_integration(self, sidecar_process):
        """Integration test: full start/stop cycle."""
        # Start recording
        start_response = self._send_request(
            sidecar_process, "recording.start", req_id=1
        )

        if "error" in start_response:
            # Audio device might not be available - this is okay in CI
            pytest.skip(f"Could not start recording: {start_response['error']}")

        assert "result" in start_response
        session_id = start_response["result"]["session_id"]

        # Check status
        status_response = self._send_request(
            sidecar_process, "recording.status", req_id=2
        )
        assert status_response["result"]["state"] == "recording"

        # Stop recording
        stop_response = self._send_request(
            sidecar_process,
            "recording.stop",
            {"session_id": session_id},
            req_id=3,
        )

        assert "result" in stop_response
        assert "audio_duration_ms" in stop_response["result"]

        # Status should be idle again
        status_response = self._send_request(
            sidecar_process, "recording.status", req_id=4
        )
        assert status_response["result"]["state"] == "idle"
