"""Compliance tests for asr.status handler, docs, and contract entries."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import pytest

from openvoicy_sidecar.asr import handle_asr_status
from openvoicy_sidecar.protocol import Request
from openvoicy_sidecar.server import HANDLERS


@dataclass
class _EngineStatusStub:
    """Minimal ASR engine stub exposing get_status()."""

    payload: dict[str, Any]

    def get_status(self) -> dict[str, Any]:
        return self.payload.copy()


def _request(req_id: int) -> Request:
    return Request(method="asr.status", id=req_id, params={})


def test_asr_status_handler_in_dispatch_table() -> None:
    assert "asr.status" in HANDLERS
    assert HANDLERS["asr.status"] is handle_asr_status


def test_asr_status_uninitialized_shape(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(
        "openvoicy_sidecar.asr.get_engine",
        lambda: _EngineStatusStub({"state": "uninitialized", "ready": False}),
    )

    request = _request(1)
    print(f"rpc_call method={request.method} params={request.params}")
    result = handle_asr_status(request)
    print(f"rpc_response method={request.method} result={result}")

    assert result["state"] == "uninitialized"
    assert result["ready"] is False
    assert "model_id" not in result or result["model_id"] is None


def test_asr_status_ready_shape(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(
        "openvoicy_sidecar.asr.get_engine",
        lambda: _EngineStatusStub(
            {
                "state": "ready",
                "model_id": "nvidia/parakeet-tdt-0.6b-v3",
                "device": "cuda",
                "ready": True,
            }
        ),
    )

    request = _request(2)
    print(f"rpc_call method={request.method} params={request.params}")
    result = handle_asr_status(request)
    print(f"rpc_response method={request.method} result={result}")

    assert result["state"] == "ready"
    assert result["model_id"] == "nvidia/parakeet-tdt-0.6b-v3"
    assert result["device"] == "cuda"
    assert result["ready"] is True


def test_asr_status_optional_contract_entry() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    contract_path = repo_root / "shared" / "contracts" / "sidecar.rpc.v1.json"
    contract = json.loads(contract_path.read_text())
    method = next(item for item in contract["items"] if item.get("name") == "asr.status")

    assert method["required"] is False
    assert method["params_schema"]["type"] == "object"
    assert method["params_schema"]["additionalProperties"] is False
    assert set(method["result_schema"]["required"]) == {"state", "ready"}
    assert set(method["result_schema"]["properties"]["state"]["enum"]) == {
        "uninitialized",
        "downloading",
        "loading",
        "ready",
        "error",
    }
    assert method["result_schema"]["properties"]["ready"]["type"] == "boolean"


def test_asr_status_documented_in_ipc_protocol() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    protocol_path = repo_root / "shared" / "ipc" / "IPC_PROTOCOL_V1.md"
    protocol_text = protocol_path.read_text()

    assert "#### `asr.status`" in protocol_text
    assert "Optional diagnostic method" in protocol_text
    assert "\"state\": \"ready\"" in protocol_text
