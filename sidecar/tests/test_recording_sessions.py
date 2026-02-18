"""Regression tests for single-active recording session behavior."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

import numpy as np
import pytest

from openvoicy_sidecar.protocol import Request
from openvoicy_sidecar.recording import (
    AlreadyRecordingError,
    InvalidSessionError,
    RecordingError,
    handle_recording_cancel,
    handle_recording_start,
    handle_recording_status,
    handle_recording_stop,
)


@dataclass
class _RecorderStub:
    """Controllable in-memory recorder stub for session lifecycle tests."""

    active_session_id: str | None = None
    next_session_idx: int = 1
    fail_next_start: bool = False
    sample_rate: int = 16000
    channels: int = 1

    def start(
        self,
        device_uid: str | None = None,
        session_id: str | None = None,
    ) -> str:
        if self.fail_next_start:
            self.fail_next_start = False
            raise OSError("simulated start failure")

        if self.active_session_id is not None:
            raise RuntimeError(
                f"Recording already in progress for session {self.active_session_id}"
            )

        session_id = session_id or f"session-{self.next_session_idx}"
        if session_id.startswith("session-"):
            self.next_session_idx += 1
        self.active_session_id = session_id
        return session_id

    def stop(self, session_id: str) -> tuple[np.ndarray, int]:
        if self.active_session_id is None:
            raise RuntimeError("Not recording")
        if session_id != self.active_session_id:
            raise RuntimeError(f"Invalid session ID: {session_id}")

        self.active_session_id = None
        return np.zeros(160, dtype=np.float32), 10

    def cancel(self, session_id: str) -> None:
        if self.active_session_id is None:
            raise RuntimeError("Not recording")
        if session_id != self.active_session_id:
            raise RuntimeError(f"Invalid session ID: {session_id}")
        self.active_session_id = None

    def get_status(self) -> dict[str, Any]:
        if self.active_session_id is None:
            return {"state": "idle", "session_id": None}
        return {"state": "recording", "session_id": self.active_session_id}


@pytest.fixture
def recorder_stub(monkeypatch: pytest.MonkeyPatch) -> _RecorderStub:
    recorder = _RecorderStub()
    monkeypatch.setattr("openvoicy_sidecar.recording.get_recorder", lambda: recorder)

    # recording.stop triggers async notification wiring; stub those side effects.
    monkeypatch.setattr(
        "openvoicy_sidecar.notifications.emit_status_changed",
        lambda *_args, **_kwargs: None,
    )
    monkeypatch.setattr(
        "openvoicy_sidecar.notifications.transcribe_session_async",
        lambda *_args, **_kwargs: None,
    )
    return recorder


def _request(method: str, req_id: int, params: dict[str, Any] | None = None) -> Request:
    return Request(method=method, id=req_id, params=params or {})


def test_second_recording_start_rejected(recorder_stub: _RecorderStub) -> None:
    first = handle_recording_start(_request("recording.start", 1))
    assert first["session_id"] == "session-1"

    with pytest.raises(AlreadyRecordingError) as exc_info:
        handle_recording_start(_request("recording.start", 2))

    assert exc_info.value.code == "E_ALREADY_RECORDING"
    assert "already in progress" in str(exc_info.value).lower()


def test_active_session_continues_after_rejection(recorder_stub: _RecorderStub) -> None:
    first = handle_recording_start(_request("recording.start", 1))

    with pytest.raises(AlreadyRecordingError):
        handle_recording_start(_request("recording.start", 2))

    status = handle_recording_status(_request("recording.status", 3))
    assert status["state"] == "recording"
    assert status["session_id"] == first["session_id"]


def test_start_after_stop_succeeds(recorder_stub: _RecorderStub) -> None:
    first = handle_recording_start(_request("recording.start", 1))
    stopped = handle_recording_stop(
        _request("recording.stop", 2, {"session_id": first["session_id"]})
    )
    assert stopped["session_id"] == first["session_id"]

    second = handle_recording_start(_request("recording.start", 3))
    assert second["session_id"] == "session-2"


def test_start_after_cancel_succeeds(recorder_stub: _RecorderStub) -> None:
    first = handle_recording_start(_request("recording.start", 1))
    cancelled = handle_recording_cancel(
        _request("recording.cancel", 2, {"session_id": first["session_id"]})
    )
    assert cancelled["cancelled"] is True

    second = handle_recording_start(_request("recording.start", 3))
    assert second["session_id"] == "session-2"


def test_start_after_error_succeeds_no_leaked_state(recorder_stub: _RecorderStub) -> None:
    recorder_stub.fail_next_start = True

    with pytest.raises(RecordingError) as exc_info:
        handle_recording_start(_request("recording.start", 1))

    assert exc_info.value.code == "E_AUDIO_IO"

    second = handle_recording_start(_request("recording.start", 2))
    assert second["session_id"] == "session-1"

    status = handle_recording_status(_request("recording.status", 3))
    assert status["state"] == "recording"
    assert status["session_id"] == second["session_id"]


def test_same_session_id_start_twice_is_rejected(recorder_stub: _RecorderStub) -> None:
    first = handle_recording_start(_request("recording.start", 1))
    assert first["session_id"] == "session-1"

    with pytest.raises(AlreadyRecordingError) as exc_info:
        handle_recording_start(_request("recording.start", 2))

    assert exc_info.value.code == "E_ALREADY_RECORDING"


def test_start_honors_provided_session_id(recorder_stub: _RecorderStub) -> None:
    provided_session_id = "rust-session-authoritative"
    first = handle_recording_start(
        _request("recording.start", 1, {"session_id": provided_session_id})
    )
    assert first["session_id"] == provided_session_id

    status = handle_recording_status(_request("recording.status", 2))
    assert status["session_id"] == provided_session_id


def test_stop_wrong_session_rejected(recorder_stub: _RecorderStub) -> None:
    first = handle_recording_start(_request("recording.start", 1))
    assert first["session_id"] == "session-1"

    with pytest.raises(InvalidSessionError) as exc_info:
        handle_recording_stop(
            _request("recording.stop", 2, {"session_id": "session-does-not-match"})
        )

    assert exc_info.value.code == "E_INVALID_SESSION"
