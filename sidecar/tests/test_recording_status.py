"""Compliance tests for recording.status handler, docs, and contract entries."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import pytest

from openvoicy_sidecar.protocol import Request
from openvoicy_sidecar.recording import handle_recording_status
from openvoicy_sidecar.server import HANDLERS


@dataclass
class _RecorderStatusStub:
    """Minimal recorder stub exposing get_status shape for handler tests."""

    status_payload: dict[str, Any]

    def get_status(self) -> dict[str, Any]:
        return self.status_payload.copy()


def _request(method: str = "recording.status", req_id: int = 1) -> Request:
    return Request(method=method, id=req_id, params={})


def _rpc_call(request: Request) -> dict[str, Any]:
    print(f"rpc_call method={request.method} params={request.params}")
    result = handle_recording_status(request)
    print(f"rpc_response method={request.method} result={result}")
    return result


@pytest.fixture
def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def test_recording_status_handler_in_dispatch_table() -> None:
    assert "recording.status" in HANDLERS
    assert HANDLERS["recording.status"] is handle_recording_status


def test_recording_status_idle_shape(monkeypatch: pytest.MonkeyPatch) -> None:
    recorder = _RecorderStatusStub({"state": "idle", "session_id": None})
    monkeypatch.setattr("openvoicy_sidecar.recording.get_recorder", lambda: recorder)

    result = _rpc_call(_request(req_id=1))
    assert result["state"] == "idle"
    assert result["session_id"] is None
    assert "duration_ms" not in result


def test_recording_status_recording_shape(monkeypatch: pytest.MonkeyPatch) -> None:
    recorder = _RecorderStatusStub(
        {
            "state": "recording",
            "session_id": "session-123",
            "duration_ms": 850,
            "elapsed_sec": 0.85,
        }
    )
    monkeypatch.setattr("openvoicy_sidecar.recording.get_recorder", lambda: recorder)

    result = _rpc_call(_request(req_id=2))
    assert result["state"] == "recording"
    assert result["session_id"] == "session-123"
    assert result["duration_ms"] == 850
    assert result["elapsed_sec"] == pytest.approx(0.85)


def test_recording_status_optional_contract_entry(repo_root: Path) -> None:
    contract_path = repo_root / "shared" / "contracts" / "sidecar.rpc.v1.json"
    contract = json.loads(contract_path.read_text())
    method = next(item for item in contract["items"] if item.get("name") == "recording.status")

    assert method["required"] is False
    assert method["params_schema"]["type"] == "object"
    assert method["params_schema"]["additionalProperties"] is False
    assert set(method["result_schema"]["required"]) == {"state", "session_id"}
    assert set(method["result_schema"]["properties"]["state"]["enum"]) == {
        "idle",
        "recording",
        "stopping",
    }


def test_recording_status_documented_in_ipc_protocol(repo_root: Path) -> None:
    protocol_path = repo_root / "shared" / "ipc" / "IPC_PROTOCOL_V1.md"
    protocol_text = protocol_path.read_text()

    assert "#### `recording.status`" in protocol_text
    assert "Optional diagnostic method" in protocol_text
    assert "\"state\": \"recording\"" in protocol_text
