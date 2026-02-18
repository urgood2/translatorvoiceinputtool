"""Compliance tests for audio.meter_status handler, docs, and contract entries."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

from openvoicy_sidecar.audio_meter import handle_audio_meter_status
from openvoicy_sidecar.protocol import Request
from openvoicy_sidecar.server import HANDLERS


@dataclass
class _MeterStub:
    """Minimal meter stub for status response tests."""

    is_running: bool
    interval_ms: int = 80

    @property
    def _interval_ms(self) -> int:
        return self.interval_ms


def _request(req_id: int) -> Request:
    return Request(method="audio.meter_status", id=req_id, params={})


def test_audio_meter_status_handler_in_dispatch_table() -> None:
    assert "audio.meter_status" in HANDLERS
    assert HANDLERS["audio.meter_status"] is handle_audio_meter_status


def test_audio_meter_status_idle_shape(monkeypatch) -> None:
    monkeypatch.setattr("openvoicy_sidecar.audio_meter.get_meter", lambda: _MeterStub(False))

    request = _request(1)
    print(f"rpc_call method={request.method} params={request.params}")
    result = handle_audio_meter_status(request)
    print(f"rpc_response method={request.method} result={result}")

    assert result == {"running": False}


def test_audio_meter_status_running_shape(monkeypatch) -> None:
    monkeypatch.setattr("openvoicy_sidecar.audio_meter.get_meter", lambda: _MeterStub(True, 125))

    request = _request(2)
    print(f"rpc_call method={request.method} params={request.params}")
    result = handle_audio_meter_status(request)
    print(f"rpc_response method={request.method} result={result}")

    assert result["running"] is True
    assert result["interval_ms"] == 125


def test_audio_meter_status_optional_contract_entry() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    contract_path = repo_root / "shared" / "contracts" / "sidecar.rpc.v1.json"
    contract = json.loads(contract_path.read_text())
    method = next(item for item in contract["items"] if item.get("name") == "audio.meter_status")

    assert method["required"] is False
    assert method["params_schema"]["type"] == "object"
    assert method["params_schema"]["additionalProperties"] is False
    assert set(method["result_schema"]["required"]) == {"running"}
    assert method["result_schema"]["properties"]["running"]["type"] == "boolean"
    assert method["result_schema"]["properties"]["interval_ms"]["type"] == "integer"


def test_audio_meter_status_documented_in_ipc_protocol() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    protocol_path = repo_root / "shared" / "ipc" / "IPC_PROTOCOL_V1.md"
    protocol_text = protocol_path.read_text()

    assert "#### `audio.meter_status`" in protocol_text
    assert "Optional" in protocol_text
    assert "\"running\": true" in protocol_text
