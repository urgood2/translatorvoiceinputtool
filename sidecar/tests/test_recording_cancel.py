"""Regression tests for recording.cancel behavior and cleanup guarantees."""

from __future__ import annotations

import threading
from dataclasses import dataclass
from typing import Any

import numpy as np
import pytest

from openvoicy_sidecar.notifications import (
    SessionTracker,
    emit_transcription_complete,
    get_session_tracker,
)
from openvoicy_sidecar.protocol import Request
from openvoicy_sidecar.recording import (
    InvalidSessionError,
    NotRecordingError,
    _store_audio_for_transcription,
    handle_recording_cancel,
    handle_recording_start,
    handle_recording_status,
    handle_recording_stop,
)


@dataclass
class _RecorderStub:
    """Controllable recorder stub for cancel-path behavior tests."""

    active_session_id: str | None = None
    next_session_idx: int = 1
    phase: str = "idle"  # idle | recording | transcribing
    # Production AudioRecorder only allows cancel while actively recording.
    allow_cancel_while_transcribing: bool = False
    sample_rate: int = 16000
    channels: int = 1

    def start(self, device_uid: str | None = None) -> str:
        if self.phase != "idle":
            raise RuntimeError("Recording already in progress")
        session_id = f"session-{self.next_session_idx}"
        self.next_session_idx += 1
        self.active_session_id = session_id
        self.phase = "recording"
        return session_id

    def stop(self, session_id: str) -> tuple[np.ndarray, int]:
        if self.phase != "recording":
            raise RuntimeError("Not recording")
        if session_id != self.active_session_id:
            raise RuntimeError(f"Invalid session ID: {session_id}")
        self.phase = "transcribing"
        return np.zeros(160, dtype=np.float32), 10

    def cancel(self, session_id: str) -> None:
        if self.phase == "idle":
            raise RuntimeError("Not recording")
        if session_id != self.active_session_id:
            raise RuntimeError(f"Invalid session ID: {session_id}")
        if self.phase == "transcribing" and not self.allow_cancel_while_transcribing:
            raise RuntimeError("Not recording")
        self.active_session_id = None
        self.phase = "idle"

    def get_status(self) -> dict[str, Any]:
        return {
            "state": self.phase,
            "session_id": self.active_session_id,
        }


def _request(method: str, req_id: int, params: dict[str, Any] | None = None) -> Request:
    return Request(method=method, id=req_id, params=params or {})


def _rpc_call(handler: Any, request: Request) -> dict[str, Any]:
    print(f"rpc_call method={request.method} params={request.params}")
    result = handler(request)
    print(f"rpc_response method={request.method} result={result}")
    return result


@pytest.fixture(autouse=True)
def reset_recording_state(monkeypatch: pytest.MonkeyPatch) -> None:
    """Reset global recorder, tracker, and pending audio between tests."""
    import openvoicy_sidecar.notifications as notifications_module
    import openvoicy_sidecar.recording as recording_module

    monkeypatch.setattr(notifications_module, "_session_tracker", SessionTracker())
    monkeypatch.setattr(recording_module, "_recorder", None)

    with recording_module._pending_audio_lock:
        recording_module._pending_audio.clear()


@pytest.fixture
def recorder_stub(monkeypatch: pytest.MonkeyPatch) -> _RecorderStub:
    recorder = _RecorderStub()
    monkeypatch.setattr("openvoicy_sidecar.recording.get_recorder", lambda: recorder)
    return recorder


def test_start_cancel_never_emits_transcription_complete(
    monkeypatch: pytest.MonkeyPatch,
    recorder_stub: _RecorderStub,
) -> None:
    """Start -> cancel should never yield event.transcription_complete."""
    notification_methods: list[str] = []
    transcription_complete_seen = threading.Event()

    def fake_write(notification: Any) -> None:
        print(f"notification method={notification.method} params={notification.params}")
        notification_methods.append(notification.method)
        if notification.method == "event.transcription_complete":
            transcription_complete_seen.set()

    monkeypatch.setattr("openvoicy_sidecar.notifications.write_notification", fake_write)

    started = _rpc_call(handle_recording_start, _request("recording.start", 1))
    session_id = started["session_id"]
    cancelled = _rpc_call(
        handle_recording_cancel,
        _request("recording.cancel", 2, {"session_id": session_id}),
    )
    assert cancelled["cancelled"] is True
    assert cancelled["session_id"] == session_id

    emitted = emit_transcription_complete(session_id, "should-not-emit", 1)
    print(f"post-cancel emit attempt session={session_id} emitted={emitted}")
    assert emitted is False
    assert not transcription_complete_seen.wait(timeout=5.0)
    assert "event.transcription_complete" not in notification_methods


def test_cancel_when_idle_returns_not_recording(recorder_stub: _RecorderStub) -> None:
    """Cancel while idle should raise NotRecordingError."""
    request = _request("recording.cancel", 1, {"session_id": "no-session"})
    print(f"rpc_call method={request.method} params={request.params}")
    with pytest.raises(NotRecordingError) as exc_info:
        handle_recording_cancel(request)
    print(f"rpc_error method={request.method} error={exc_info.value}")


def test_cancel_wrong_session_id_returns_invalid_session(
    recorder_stub: _RecorderStub,
) -> None:
    """Cancel with mismatched session_id should raise InvalidSessionError."""
    started = _rpc_call(handle_recording_start, _request("recording.start", 1))
    request = _request("recording.cancel", 2, {"session_id": "wrong-session-id"})

    print(f"rpc_call method={request.method} params={request.params}")
    with pytest.raises(InvalidSessionError) as exc_info:
        handle_recording_cancel(request)
    print(f"rpc_error method={request.method} error={exc_info.value}")

    status = _rpc_call(handle_recording_status, _request("recording.status", 3))
    assert status["state"] == "recording"
    assert status["session_id"] == started["session_id"]


def test_cancel_during_active_transcription_aborts_completion(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Cancel during transcribing should mark session cancelled and block completion."""
    recorder = _RecorderStub()
    monkeypatch.setattr("openvoicy_sidecar.recording.get_recorder", lambda: recorder)
    monkeypatch.setattr(
        "openvoicy_sidecar.notifications.emit_status_changed",
        lambda *args, **kwargs: print(f"status_changed args={args} kwargs={kwargs}"),
    )

    def fake_transcribe(session_id: str, _audio: np.ndarray, _sample_rate: int) -> None:
        print(f"transcribe_session_async session_id={session_id}")
        get_session_tracker().register(session_id)

    monkeypatch.setattr("openvoicy_sidecar.notifications.transcribe_session_async", fake_transcribe)

    started = _rpc_call(handle_recording_start, _request("recording.start", 1))
    session_id = started["session_id"]
    _rpc_call(handle_recording_stop, _request("recording.stop", 2, {"session_id": session_id}))
    assert get_session_tracker().should_emit(session_id) is True

    cancelled = _rpc_call(
        handle_recording_cancel,
        _request("recording.cancel", 3, {"session_id": session_id}),
    )
    assert cancelled["cancelled"] is True
    assert cancelled["session_id"] == session_id
    assert get_session_tracker().should_emit(session_id) is False

    emitted = emit_transcription_complete(session_id, "late-result", 1)
    print(f"transcribing-cancel emit attempt session={session_id} emitted={emitted}")
    assert emitted is False


def test_cancel_cleans_pending_audio_for_session(
    monkeypatch: pytest.MonkeyPatch,
    recorder_stub: _RecorderStub,
) -> None:
    """Cancel should clean up any pending in-memory audio for the session."""
    import openvoicy_sidecar.recording as recording_module

    started = _rpc_call(handle_recording_start, _request("recording.start", 1))
    session_id = started["session_id"]

    _store_audio_for_transcription(session_id, np.ones(32, dtype=np.float32), 16000)
    with recording_module._pending_audio_lock:
        assert session_id in recording_module._pending_audio
        print(f"pending_audio before cancel sessions={list(recording_module._pending_audio)}")

    cancelled = _rpc_call(
        handle_recording_cancel,
        _request("recording.cancel", 2, {"session_id": session_id}),
    )
    assert cancelled["cancelled"] is True

    with recording_module._pending_audio_lock:
        print(f"pending_audio after cancel sessions={list(recording_module._pending_audio)}")
        assert session_id not in recording_module._pending_audio
