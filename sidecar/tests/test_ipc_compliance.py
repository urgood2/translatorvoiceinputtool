"""Comprehensive IPC method compliance checks against IPC protocol contracts."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np
import pytest

from openvoicy_sidecar.asr import ASRError, handle_asr_initialize, handle_asr_status
from openvoicy_sidecar.model_cache import (
    ModelInUseError,
    handle_model_get_status,
    handle_model_purge_cache,
)
from openvoicy_sidecar.audio import (
    AudioDevice,
    DeviceNotFoundError,
    handle_audio_list_devices,
    handle_audio_set_device,
)
from openvoicy_sidecar.audio_meter import (
    handle_audio_meter_start,
    handle_audio_meter_status,
    handle_audio_meter_stop,
)
from openvoicy_sidecar.protocol import ERROR_INTERNAL, ERROR_INVALID_PARAMS, ERROR_METHOD_NOT_FOUND, Request
from openvoicy_sidecar.recording import (
    AlreadyRecordingError,
    NotRecordingError,
    handle_recording_cancel,
    handle_recording_start,
    handle_recording_stop,
)
from openvoicy_sidecar.replacements import (
    ReplacementError,
    handle_replacements_get_presets,
    handle_replacements_get_rules,
    handle_replacements_preview,
    handle_replacements_set_rules,
)
from openvoicy_sidecar.server import (
    HANDLERS,
    handle_status_get,
    handle_system_info,
    handle_system_ping,
    handle_system_shutdown,
)


CONTRACT_PATH = (
    Path(__file__).resolve().parents[2] / "shared" / "contracts" / "sidecar.rpc.v1.json"
)


def _log(message: str) -> None:
    print(f"[IPC_COMPLIANCE] {message}")


def _request(method: str, req_id: int, params: dict[str, Any] | None = None) -> Request:
    return Request(method=method, id=req_id, params=params or {})


def _required_contract_methods() -> set[str]:
    contract = json.loads(CONTRACT_PATH.read_text(encoding="utf-8"))
    return {
        item["name"]
        for item in contract["items"]
        if item.get("type") == "method" and item.get("required") is True
    }


@dataclass
class _StateStub:
    value: str


@dataclass
class _RecorderStub:
    state: _StateStub = field(default_factory=lambda: _StateStub("idle"))
    session_id: str | None = None
    sample_rate: int = 16000
    channels: int = 1
    _preprocess_options: dict[str, Any] = field(
        default_factory=lambda: {"normalize": False, "audio": {"trim_silence": True}}
    )

    @property
    def preprocess_options(self) -> dict[str, Any]:
        return self._preprocess_options.copy()

    def start(
        self,
        _device_uid: str | None = None,
        session_id: str | None = None,
        vad: Any = None,
        preprocess: Any = None,
    ) -> str:
        if self.state.value == "recording":
            raise RuntimeError("already recording")
        self.state = _StateStub("recording")
        self.session_id = session_id or "session-123"
        return self.session_id

    def stop(self, session_id: str) -> tuple[np.ndarray, int]:
        if self.state.value != "recording":
            raise RuntimeError("Not recording")
        if session_id != self.session_id:
            raise RuntimeError("Invalid session ID")
        self.state = _StateStub("idle")
        return np.zeros(160, dtype=np.float32), 10

    def cancel(self, session_id: str) -> None:
        if self.state.value != "recording":
            raise RuntimeError("Not recording")
        if session_id != self.session_id:
            raise RuntimeError("Invalid session ID")
        self.state = _StateStub("idle")
        self.session_id = None

    def get_status(self) -> dict[str, Any]:
        return {"state": self.state.value, "session_id": self.session_id}


@dataclass
class _MeterStub:
    is_running: bool = False
    _interval_ms: int = 80

    def start(self, _device_uid: str | None, interval_ms: int) -> None:
        if self.is_running:
            raise RuntimeError("already running")
        self.is_running = True
        self._interval_ms = interval_ms

    def stop(self) -> None:
        self.is_running = False


@dataclass
class _EngineStub:
    status_payload: dict[str, Any]

    def get_status(self) -> dict[str, Any]:
        return self.status_payload.copy()


@dataclass
class _TrackerStub:
    pending: bool = False

    def has_pending(self) -> bool:
        return self.pending


@pytest.fixture
def run_sidecar() -> Any:
    src_path = Path(__file__).parent.parent / "src"

    def _run(
        input_lines: list[str], timeout: float = 5.0
    ) -> tuple[list[dict[str, Any]], list[str], int]:
        input_text = "\n".join(input_lines) + "\n"
        proc = subprocess.run(
            [sys.executable, "-m", "openvoicy_sidecar"],
            input=input_text,
            capture_output=True,
            text=True,
            cwd=str(src_path.parent),
            env={**dict(os.environ), "PYTHONPATH": str(src_path)},
            timeout=timeout,
        )

        responses = [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]
        stderr_lines = [line for line in proc.stderr.splitlines() if line.strip()]
        return responses, stderr_lines, proc.returncode

    return _run


@pytest.fixture(autouse=True)
def reset_replacements_state() -> Any:
    """Keep replacements global state isolated per test."""
    import openvoicy_sidecar.replacements as replacements_module

    original_presets = replacements_module._presets.copy()
    original_active_rules = replacements_module._active_rules.copy()
    yield
    replacements_module._presets = original_presets
    replacements_module._active_rules = original_active_rules


@pytest.fixture(autouse=True)
def patch_recording_async(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(
        "openvoicy_sidecar.notifications.emit_status_changed",
        lambda *_args, **_kwargs: None,
    )
    monkeypatch.setattr(
        "openvoicy_sidecar.notifications.transcribe_session_async",
        lambda *_args, **_kwargs: None,
    )


def test_required_ipc_methods_exist() -> None:
    required_methods = _required_contract_methods()
    assert required_methods, "Expected non-empty required method set from sidecar contract"

    _log("Testing required method registration in handler dispatch table")
    missing_methods = sorted(method for method in required_methods if method not in HANDLERS)
    assert not missing_methods, (
        "Missing required IPC handlers from contract: " + ", ".join(missing_methods)
    )
    _log("Assertion: all required methods registered -> PASS")


def test_system_ping_shape_and_latency() -> None:
    request = _request("system.ping", 1)
    _log(f"Testing system.ping request={request.params}")
    start = time.perf_counter()
    result = handle_system_ping(request)
    elapsed = time.perf_counter() - start
    _log(f"Response={result}")
    assert isinstance(result["version"], str)
    assert result["protocol"] == "v1"
    assert elapsed < 1.0
    _log("Assertion: ping shape and latency -> PASS")


def test_system_info_required_runtime_fields() -> None:
    request = _request("system.info", 2)
    _log(f"Testing system.info request={request.params}")
    result = handle_system_info(request)
    _log(f"Response={result}")
    assert isinstance(result["capabilities"], list)
    runtime = result["runtime"]
    assert isinstance(runtime["python_version"], str)
    assert isinstance(runtime["platform"], str)
    assert isinstance(runtime["cuda_available"], bool)
    resource_paths = result["resource_paths"]
    assert isinstance(resource_paths, dict)
    assert set(resource_paths.keys()) >= {
        "shared_root",
        "presets",
        "model_manifest",
        "model_catalog",
        "contracts_dir",
    }
    for key in ("presets", "model_manifest", "model_catalog", "contracts_dir"):
        value = resource_paths.get(key)
        assert value is None or isinstance(value, str)
    _log("Assertion: system.info runtime fields -> PASS")


def test_system_info_omits_whisper_capability_when_backend_unavailable(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr("openvoicy_sidecar.server._whisper_backend_available", lambda: False)
    result = handle_system_info(_request("system.info", 21))
    assert "whisper" not in result["capabilities"]
    assert result["capabilities_detail"]["whisper_available"] is False


def test_system_info_includes_whisper_capability_when_backend_available(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr("openvoicy_sidecar.server._whisper_backend_available", lambda: True)
    result = handle_system_info(_request("system.info", 22))
    assert "whisper" in result["capabilities"]
    assert result["capabilities_detail"]["whisper_available"] is True


def test_system_shutdown_shape() -> None:
    request = _request("system.shutdown", 3, {"reason": "ipc-compliance"})
    _log(f"Testing system.shutdown request={request.params}")
    result = handle_system_shutdown(request)
    _log(f"Response={result}")
    assert result == {"status": "shutting_down"}
    _log("Assertion: shutdown response shape -> PASS")


def test_status_get_states_and_model_info(monkeypatch: pytest.MonkeyPatch) -> None:
    _log("Testing status.get idle/transcribing/model mapping")
    monkeypatch.setattr(
        "openvoicy_sidecar.server.get_engine",
        lambda: _EngineStub({"state": "ready", "model_id": "test-model", "ready": True, "device": "cpu"}),
    )
    monkeypatch.setattr("openvoicy_sidecar.server.get_recorder", lambda: _RecorderStub(state=_StateStub("idle")))
    monkeypatch.setattr("openvoicy_sidecar.server.get_session_tracker", lambda: _TrackerStub(pending=True))
    transcribing = handle_status_get(_request("status.get", 10))
    _log(f"Response(transcribing)={transcribing}")
    assert transcribing["state"] == "transcribing"
    assert transcribing["model"]["model_id"] == "test-model"
    assert transcribing["model"]["status"] == "ready"

    monkeypatch.setattr(
        "openvoicy_sidecar.server.get_engine",
        lambda: _EngineStub({"state": "uninitialized", "model_id": None, "ready": False}),
    )
    monkeypatch.setattr("openvoicy_sidecar.server.get_session_tracker", lambda: _TrackerStub(pending=False))
    idle = handle_status_get(_request("status.get", 11))
    _log(f"Response(idle)={idle}")
    assert idle["state"] == "idle"
    assert "model" not in idle, "model must be absent when engine is uninitialized with no model_id"

    # loading_model state (downloading)
    monkeypatch.setattr(
        "openvoicy_sidecar.server.get_engine",
        lambda: _EngineStub({"state": "downloading", "model_id": "dl-model", "ready": False}),
    )
    downloading = handle_status_get(_request("status.get", 12))
    _log(f"Response(downloading)={downloading}")
    assert downloading["state"] == "loading_model"
    assert isinstance(downloading["detail"], str)
    assert downloading["model"]["model_id"] == "dl-model"
    assert downloading["model"]["status"] == "downloading"

    # loading_model state (loading)
    monkeypatch.setattr(
        "openvoicy_sidecar.server.get_engine",
        lambda: _EngineStub({"state": "loading", "model_id": "ld-model", "ready": False}),
    )
    loading = handle_status_get(_request("status.get", 13))
    _log(f"Response(loading)={loading}")
    assert loading["state"] == "loading_model"
    assert isinstance(loading["detail"], str)
    assert loading["model"]["model_id"] == "ld-model"
    assert loading["model"]["status"] == "verifying"

    # error state
    monkeypatch.setattr(
        "openvoicy_sidecar.server.get_engine",
        lambda: _EngineStub({"state": "error", "model_id": "err-model", "ready": False}),
    )
    error_resp = handle_status_get(_request("status.get", 14))
    _log(f"Response(error)={error_resp}")
    assert error_resp["state"] == "error"
    assert isinstance(error_resp["detail"], str)
    assert error_resp["model"]["model_id"] == "err-model"
    assert error_resp["model"]["status"] == "error"

    _log("Assertion: status.get state transitions, model field, and absence -> PASS")


def test_audio_list_devices_shape(monkeypatch: pytest.MonkeyPatch) -> None:
    _log("Testing audio.list_devices response shape")
    devices = [
        AudioDevice(
            uid="dev-1",
            name="Mic 1",
            is_default=True,
            default_sample_rate=48000,
            channels=1,
            host_api="test",
        )
    ]
    monkeypatch.setattr("openvoicy_sidecar.audio.list_audio_devices", lambda: devices)
    result = handle_audio_list_devices(_request("audio.list_devices", 20))
    _log(f"Response={result}")
    assert isinstance(result["devices"], list)
    assert result["devices"][0]["uid"] == "dev-1"
    _log("Assertion: audio.list_devices shape -> PASS")


def test_audio_set_device_valid_and_invalid(monkeypatch: pytest.MonkeyPatch) -> None:
    _log("Testing audio.set_device valid and invalid paths")
    monkeypatch.setattr("openvoicy_sidecar.audio.set_active_device", lambda uid: uid)
    success = handle_audio_set_device(_request("audio.set_device", 21, {"device_uid": "dev-1"}))
    _log(f"Response(valid)={success}")
    assert success["active_device_uid"] == "dev-1"

    def _raise_value_error(_uid: str | None) -> str | None:
        raise ValueError("Device not found: missing")

    monkeypatch.setattr("openvoicy_sidecar.audio.set_active_device", _raise_value_error)
    with pytest.raises(DeviceNotFoundError):
        handle_audio_set_device(_request("audio.set_device", 22, {"device_uid": "missing"}))
    _log("Assertion: audio.set_device valid/invalid handling -> PASS")


def test_audio_meter_start_stop_status_cycle(monkeypatch: pytest.MonkeyPatch) -> None:
    _log("Testing audio.meter_start/stop/status cycle")
    meter = _MeterStub()
    monkeypatch.setattr("openvoicy_sidecar.audio_meter.get_meter", lambda: meter)

    started = handle_audio_meter_start(_request("audio.meter_start", 30, {"interval_ms": 120}))
    _log(f"Response(start)={started}")
    assert started["running"] is True
    assert started["interval_ms"] == 120

    status_running = handle_audio_meter_status(_request("audio.meter_status", 31))
    _log(f"Response(status_running)={status_running}")
    assert status_running["running"] is True
    assert status_running["interval_ms"] == 120

    stopped = handle_audio_meter_stop(_request("audio.meter_stop", 32))
    _log(f"Response(stop)={stopped}")
    assert stopped["stopped"] is True

    status_idle = handle_audio_meter_status(_request("audio.meter_status", 33))
    _log(f"Response(status_idle)={status_idle}")
    assert status_idle == {"running": False}
    _log("Assertion: meter cycle -> PASS")


def test_recording_start_stop_cancel_and_error_paths(monkeypatch: pytest.MonkeyPatch) -> None:
    _log("Testing recording.start/stop/cancel and error paths")
    recorder = _RecorderStub()
    monkeypatch.setattr("openvoicy_sidecar.recording.get_recorder", lambda: recorder)

    start = handle_recording_start(_request("recording.start", 40))
    _log(f"Response(start)={start}")
    assert isinstance(start["session_id"], str)
    assert start["session_id"]

    with pytest.raises(AlreadyRecordingError):
        handle_recording_start(_request("recording.start", 41))

    stop = handle_recording_stop(_request("recording.stop", 42, {"session_id": start["session_id"]}))
    _log(f"Response(stop)={stop}")
    assert set(stop) == {"audio_duration_ms", "sample_rate", "channels", "session_id"}

    with pytest.raises(NotRecordingError):
        handle_recording_stop(_request("recording.stop", 43, {"session_id": start["session_id"]}))

    start2 = handle_recording_start(_request("recording.start", 44))
    cancel = handle_recording_cancel(_request("recording.cancel", 45, {"session_id": start2["session_id"]}))
    _log(f"Response(cancel)={cancel}")
    assert cancel["cancelled"] is True
    _log("Assertion: recording method compliance and error cases -> PASS")


def test_recording_start_accepts_caller_provided_session_id(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _log("Testing recording.start explicit caller-provided session_id path")
    recorder = _RecorderStub()
    monkeypatch.setattr("openvoicy_sidecar.recording.get_recorder", lambda: recorder)

    provided_session_id = "ipc-compliance-session-001"
    start = handle_recording_start(
        _request(
            "recording.start",
            46,
            {"session_id": provided_session_id},
        )
    )
    _log(f"Response(start_with_session)={start}")

    assert start["session_id"] == provided_session_id


def test_recording_cancel_does_not_trigger_transcription(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _log("Testing recording.cancel does not start transcription")
    recorder = _RecorderStub()
    monkeypatch.setattr("openvoicy_sidecar.recording.get_recorder", lambda: recorder)

    transcription_calls: list[tuple[str, int]] = []

    def _spy_transcribe_session_async(session_id: str, audio: np.ndarray, sample_rate: int) -> None:
        transcription_calls.append((session_id, sample_rate))

    monkeypatch.setattr(
        "openvoicy_sidecar.notifications.transcribe_session_async",
        _spy_transcribe_session_async,
    )

    started = handle_recording_start(_request("recording.start", 47))
    cancelled = handle_recording_cancel(
        _request("recording.cancel", 48, {"session_id": started["session_id"]})
    )

    assert cancelled["cancelled"] is True
    assert cancelled["session_id"] == started["session_id"]
    assert transcription_calls == []


def test_recording_cancel_does_not_start_transcription(monkeypatch: pytest.MonkeyPatch) -> None:
    """Regression (3461): recording.cancel must not trigger transcription."""
    _log("Testing recording.cancel does not invoke transcription")
    recorder = _RecorderStub()
    monkeypatch.setattr("openvoicy_sidecar.recording.get_recorder", lambda: recorder)

    transcribe_calls: list[Any] = []
    monkeypatch.setattr(
        "openvoicy_sidecar.notifications.transcribe_session_async",
        lambda *args, **kwargs: transcribe_calls.append((args, kwargs)),
    )

    start = handle_recording_start(_request("recording.start", 47))
    cancel = handle_recording_cancel(_request("recording.cancel", 48, {"session_id": start["session_id"]}))
    assert cancel["cancelled"] is True
    assert len(transcribe_calls) == 0, "recording.cancel must not trigger transcription"
    _log("Assertion: recording.cancel avoids transcription -> PASS")


def test_replacements_rules_presets_preview_and_validation(monkeypatch: pytest.MonkeyPatch) -> None:
    _log("Testing replacements.get_rules/set_rules/get_presets/preview")
    from openvoicy_sidecar.replacements import Preset, ReplacementRule, _active_rules, _presets

    _active_rules.clear()
    _presets.clear()
    _presets["preset-a"] = Preset(
        id="preset-a",
        name="Preset A",
        description="test preset",
        rules=[
            ReplacementRule(
                id="preset-a:r1",
                enabled=True,
                kind="literal",
                pattern="foo",
                replacement="bar",
                case_sensitive=False,
            )
        ],
    )

    set_rules_result = handle_replacements_set_rules(
        _request(
            "replacements.set_rules",
            50,
            {
                "rules": [
                    {
                        "id": "rule-1",
                        "enabled": True,
                        "kind": "literal",
                        "pattern": "hello",
                        "replacement": "hi",
                        "word_boundary": False,
                        "case_sensitive": False,
                    }
                ]
            },
        )
    )
    _log(f"Response(set_rules)={set_rules_result}")
    assert set_rules_result["count"] == 1

    get_rules_result = handle_replacements_get_rules(_request("replacements.get_rules", 51))
    _log(f"Response(get_rules)={get_rules_result}")
    assert isinstance(get_rules_result["rules"], list)
    assert get_rules_result["rules"][0]["pattern"] == "hello"

    presets_result = handle_replacements_get_presets(_request("replacements.get_presets", 52))
    _log(f"Response(get_presets)={presets_result}")
    assert isinstance(presets_result["presets"], list)
    assert presets_result["presets"][0]["id"] == "preset-a"

    preview_result = handle_replacements_preview(
        _request(
            "replacements.preview",
            53,
            {"text": "hello world", "skip_normalize": True, "skip_macros": True},
        )
    )
    _log(f"Response(preview)={preview_result}")
    assert isinstance(preview_result["result"], str)
    assert isinstance(preview_result["applied_rules_count"], int)

    with pytest.raises(ReplacementError):
        handle_replacements_set_rules(
            _request(
                "replacements.set_rules",
                54,
                {"rules": [{"id": "bad", "enabled": True, "kind": "literal", "pattern": "", "replacement": "x"}]},
            )
        )
    _log("Assertion: replacements compliance and invalid params -> PASS")


def test_asr_status_shape(monkeypatch: pytest.MonkeyPatch) -> None:
    _log("Testing asr.status response shape")
    monkeypatch.setattr(
        "openvoicy_sidecar.asr.get_engine",
        lambda: _EngineStub({"state": "ready", "model_id": "m1", "device": "cpu", "ready": True}),
    )
    result = handle_asr_status(_request("asr.status", 60))
    _log(f"Response={result}")
    assert result["state"] == "ready"
    assert result["ready"] is True
    assert result["model_id"] == "m1"
    assert result["device"] == "cpu"
    _log("Assertion: asr.status shape -> PASS")


def test_unknown_method_returns_jsonrpc_method_not_found(run_sidecar: Any) -> None:
    _log("Testing unknown method JSON-RPC error")
    request = '{"jsonrpc":"2.0","id":70,"method":"unknown.method"}'
    shutdown = '{"jsonrpc":"2.0","id":99,"method":"system.shutdown","params":{"reason":"compliance-test"}}'
    responses, _, exit_code = run_sidecar([request, shutdown], timeout=10.0)
    _log(f"Response={responses[0]}")
    error = responses[0]["error"]
    assert error["code"] == ERROR_METHOD_NOT_FOUND
    assert error["data"]["kind"] == "E_METHOD_NOT_FOUND"
    assert exit_code == 0, f"Sidecar should exit cleanly after shutdown, got {exit_code}"
    _log("Assertion: unknown method -> E_METHOD_NOT_FOUND -> PASS")


def test_missing_required_params_returns_error_not_crash(run_sidecar: Any) -> None:
    _log("Testing missing required params for recording.stop")
    request = '{"jsonrpc":"2.0","id":71,"method":"recording.stop","params":{}}'
    shutdown = '{"jsonrpc":"2.0","id":99,"method":"system.shutdown","params":{"reason":"compliance-test"}}'
    responses, _, exit_code = run_sidecar([request, shutdown], timeout=10.0)
    _log(f"Response={responses[0]}")
    assert "error" in responses[0]
    assert responses[0]["error"]["data"]["kind"] == "E_INVALID_SESSION"
    assert exit_code == 0, f"Sidecar should exit cleanly after shutdown, got {exit_code}"
    _log("Assertion: missing required params returns structured error -> PASS")


def test_invalid_params_type_returns_jsonrpc_error(run_sidecar: Any) -> None:
    """Regression (2eev): wrong-type params must return structured error, not crash."""
    _log("Testing invalid params type for replacements.set_rules")
    # Send rules as a number instead of a list — wrong type
    bad_request = '{"jsonrpc":"2.0","id":72,"method":"replacements.set_rules","params":{"rules":42}}'
    # Follow up with a ping to prove the server is still alive after the error
    ping_request = '{"jsonrpc":"2.0","id":73,"method":"system.ping"}'
    # Shutdown to ensure clean process exit
    shutdown_request = '{"jsonrpc":"2.0","id":74,"method":"system.shutdown","params":{"reason":"compliance-test"}}'
    responses, _, exit_code = run_sidecar([bad_request, ping_request, shutdown_request], timeout=10.0)
    assert len(responses) >= 2, "Server must survive bad params and respond to subsequent requests"

    _log(f"Response(bad)={responses[0]}")
    _log(f"Response(ping)={responses[1]}")

    # The bad request must return a structured JSON-RPC error
    assert "error" in responses[0], "Expected error response for wrong-type params"
    error = responses[0]["error"]
    assert isinstance(error["code"], int), "Error code must be an integer"
    assert error["code"] in (ERROR_INTERNAL, ERROR_INVALID_PARAMS), (
        f"Expected error code {ERROR_INTERNAL} or {ERROR_INVALID_PARAMS}, got {error['code']}"
    )
    assert isinstance(error["message"], str), "Error message must be a string"
    assert "data" in error, "Error must include data field"
    assert isinstance(error["data"]["kind"], str), "Error data.kind must be a string"

    # The ping must succeed — proving the server didn't crash
    assert "result" in responses[1], "Ping must succeed after error"
    assert responses[1]["result"]["protocol"] == "v1"

    # Clean exit after explicit shutdown
    assert exit_code == 0, f"Sidecar should exit cleanly after shutdown, got {exit_code}"
    _log("Assertion: invalid params type returns structured error, server survives -> PASS")


def test_system_shutdown_process_exit() -> None:
    """Regression (25dl): system.shutdown must terminate the sidecar process cleanly."""
    _log("Testing system.shutdown subprocess-level clean exit")
    src_path = Path(__file__).parent.parent / "src"
    shutdown_req = '{"jsonrpc":"2.0","id":80,"method":"system.shutdown","params":{"reason":"compliance-test"}}'
    proc = subprocess.run(
        [sys.executable, "-m", "openvoicy_sidecar"],
        input=shutdown_req + "\n",
        capture_output=True,
        text=True,
        cwd=str(src_path.parent),
        env={**dict(os.environ), "PYTHONPATH": str(src_path)},
        timeout=10.0,
    )
    responses = [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]
    assert len(responses) >= 1, "Expected at least one JSON-RPC response"
    shutdown_resp = next((r for r in responses if r.get("id") == 80), None)
    assert shutdown_resp is not None, "Missing response for shutdown request"
    assert shutdown_resp["result"]["status"] == "shutting_down"
    assert proc.returncode == 0, f"Sidecar should exit cleanly after shutdown, got exit code {proc.returncode}"
    _log(f"Response={shutdown_resp}, exit_code={proc.returncode}")
    _log("Assertion: system.shutdown subprocess clean exit -> PASS")


def test_model_get_status_shape(monkeypatch: pytest.MonkeyPatch) -> None:
    """Regression (28gq): model.get_status must return expected status fields."""
    _log("Testing model.get_status response shape")

    class _CacheManagerStub:
        def load_manifest(self, _path):
            return None

        def check_cache(self, _manifest):
            pass

        def get_status(self, manifest=None):
            return {
                "model_id": "test/model",
                "revision": "r1",
                "status": "missing",
                "cache_path": "/tmp/cache",
            }

    monkeypatch.setattr(
        "openvoicy_sidecar.model_cache.get_cache_manager",
        lambda: _CacheManagerStub(),
    )
    monkeypatch.setattr(
        "openvoicy_sidecar.model_cache.resolve_shared_path_optional",
        lambda _rel: None,
    )
    result = handle_model_get_status(_request("model.get_status", 90))
    _log(f"Response={result}")
    assert result["model_id"] == "test/model"
    assert result["status"] in ("missing", "downloading", "verifying", "ready", "error")
    assert "revision" in result
    assert "cache_path" in result
    _log("Assertion: model.get_status shape -> PASS")


def test_model_purge_cache_success_and_in_use_error(monkeypatch: pytest.MonkeyPatch) -> None:
    """Regression (28gq): model.purge_cache success shape and ModelInUseError path."""
    _log("Testing model.purge_cache success and error paths")

    class _PurgeableStub:
        def purge_cache(self, model_id=None):
            return True

    monkeypatch.setattr(
        "openvoicy_sidecar.model_cache.get_cache_manager",
        lambda: _PurgeableStub(),
    )
    result = handle_model_purge_cache(_request("model.purge_cache", 91))
    _log(f"Response(success)={result}")
    assert result == {"purged": True}

    class _InUseStub:
        def purge_cache(self, model_id=None):
            raise ModelInUseError("Model is currently in use")

    monkeypatch.setattr(
        "openvoicy_sidecar.model_cache.get_cache_manager",
        lambda: _InUseStub(),
    )
    with pytest.raises(ModelInUseError):
        handle_model_purge_cache(_request("model.purge_cache", 92))
    _log("Assertion: model.purge_cache success + ModelInUseError -> PASS")


def test_asr_initialize_rejects_invalid_device_pref() -> None:
    """Regression (28gq): asr.initialize must reject invalid device_pref values."""
    _log("Testing asr.initialize invalid device_pref")
    with pytest.raises(ASRError, match="Invalid device_pref"):
        handle_asr_initialize(
            _request("asr.initialize", 93, {"device_pref": "tpu"})
        )
    _log("Assertion: asr.initialize rejects invalid device_pref -> PASS")
