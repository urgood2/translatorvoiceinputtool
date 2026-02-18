"""Startup health sequence tests for sidecar bootstrap path."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import pytest

from openvoicy_sidecar.protocol import Request
from openvoicy_sidecar.server import handle_status_get

# Add src to path for subprocess runs
src_path = Path(__file__).parent.parent / "src"
sys.path.insert(0, str(src_path))


def _log(message: str) -> None:
    print(f"[STARTUP_TEST] {message}")


@pytest.fixture
def run_sidecar() -> Any:
    """Run sidecar with NDJSON request lines and return parsed responses."""

    def _run(input_lines: list[str], timeout: float = 10.0) -> tuple[list[dict[str, Any]], list[str]]:
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
        return responses, stderr_lines

    return _run


def _response_time_ms(start: float) -> float:
    return (time.perf_counter() - start) * 1000.0


def _assert_no_error_logs(stderr_lines: list[str]) -> None:
    error_lines = [line for line in stderr_lines if "error" in line.lower()]
    assert not error_lines, f"Unexpected error logs: {error_lines}"


def _assert_startup_status_shape(result: dict[str, Any]) -> None:
    assert result["state"] in {"idle", "recording", "transcribing", "error"}
    if "detail" in result:
        assert isinstance(result["detail"], str)
    if "model" in result and result["model"] is not None:
        assert isinstance(result["model"]["model_id"], str)
        assert result["model"]["status"] in {"ready", "loading", "error"}


def _run_startup_sequence(
    run_sidecar: Any,
) -> tuple[list[dict[str, Any]], list[str], float, list[tuple[str, float]]]:
    methods = [
        ("system.ping", 101),
        ("system.info", 102),
        ("status.get", 103),
    ]
    input_lines = [json.dumps({"jsonrpc": "2.0", "id": req_id, "method": method}) for method, req_id in methods]

    timings: list[tuple[str, float]] = []
    _log("Sending startup sequence: system.ping -> system.info -> status.get")
    seq_start = time.perf_counter()
    responses, stderr_lines = run_sidecar(input_lines, timeout=10.0)
    total_ms = _response_time_ms(seq_start)

    for method, req_id in methods:
        # approximate per-step timing from end-to-end total for trace logging
        timings.append((method, total_ms / len(methods)))
        _log(f"Response id={req_id} method={method} received")

    _log(f"Startup sequence complete: {total_ms:.2f}ms total")
    return responses, stderr_lines, total_ms, timings


def test_startup_ping_response(run_sidecar: Any) -> None:
    """system.ping should return protocol/version quickly."""
    _log("Sending system.ping...")
    start = time.perf_counter()
    responses, _ = run_sidecar(['{"jsonrpc":"2.0","id":1,"method":"system.ping"}'], timeout=5.0)
    elapsed_ms = _response_time_ms(start)
    assert len(responses) == 1
    result = responses[0]["result"]
    assert isinstance(result["version"], str)
    assert result["protocol"] == "v1"
    _log(f"Response: protocol={result['protocol']} ({elapsed_ms:.2f}ms) ✓")


def test_startup_info_fields(run_sidecar: Any) -> None:
    """system.info should include required fields for startup compatibility."""
    _log("Sending system.info...")
    start = time.perf_counter()
    responses, _ = run_sidecar(['{"jsonrpc":"2.0","id":2,"method":"system.info"}'], timeout=5.0)
    elapsed_ms = _response_time_ms(start)
    assert len(responses) == 1
    result = responses[0]["result"]
    assert isinstance(result["capabilities"], list)
    runtime = result["runtime"]
    assert isinstance(runtime["python_version"], str)
    assert isinstance(runtime["platform"], str)
    assert isinstance(runtime["cuda_available"], bool)
    _log(
        "Response: "
        f"capabilities={result['capabilities']}, "
        f"python={runtime['python_version']}, "
        f"platform={runtime['platform']}, "
        f"cuda={runtime['cuda_available']} "
        f"({elapsed_ms:.2f}ms) ✓"
    )


def test_startup_status_get(run_sidecar: Any) -> None:
    """status.get should return valid startup status shape."""
    _log("Sending status.get...")
    start = time.perf_counter()
    responses, _ = run_sidecar(['{"jsonrpc":"2.0","id":3,"method":"status.get"}'], timeout=5.0)
    elapsed_ms = _response_time_ms(start)
    assert len(responses) == 1
    result = responses[0]["result"]
    _assert_startup_status_shape(result)
    _log(f"Response: {result} ({elapsed_ms:.2f}ms) ✓")


def test_startup_sequence_order(run_sidecar: Any) -> None:
    """Sequence ping->info->status.get should complete without errors in order."""
    responses, stderr_lines, _total_ms, _timings = _run_startup_sequence(run_sidecar)
    _assert_no_error_logs(stderr_lines)
    assert len(responses) == 3
    assert [response["id"] for response in responses] == [101, 102, 103]
    assert all("error" not in response for response in responses)


def test_startup_sequence_timing(run_sidecar: Any) -> None:
    """End-to-end startup health sequence should complete under 10 seconds."""
    _responses, stderr_lines, total_ms, _timings = _run_startup_sequence(run_sidecar)
    _assert_no_error_logs(stderr_lines)
    assert total_ms < 10_000.0


def test_startup_info_extra_fields_tolerated(run_sidecar: Any) -> None:
    """Consumers should tolerate additive top-level fields in system.info."""
    responses, _ = run_sidecar(['{"jsonrpc":"2.0","id":6,"method":"system.info"}'])
    result = responses[0]["result"]

    baseline_view = {
        "version": result["version"],
        "protocol": result["protocol"],
        "capabilities": result["capabilities"],
        "runtime": result["runtime"],
    }
    assert isinstance(baseline_view["capabilities"], list)
    assert isinstance(baseline_view["runtime"]["python_version"], str)

    extra_fields = sorted(set(result.keys()) - set(baseline_view.keys()))
    assert isinstance(extra_fields, list)
    _log(f"Additive fields tolerated: {extra_fields}")


def test_startup_status_get_no_model(run_sidecar: Any) -> None:
    """With no loaded model, status.get model payload may be absent or null."""
    responses, _ = run_sidecar(['{"jsonrpc":"2.0","id":7,"method":"status.get"}'])
    result = responses[0]["result"]
    _assert_startup_status_shape(result)
    assert "model" not in result or result["model"] is None


def test_startup_status_get_error_state(monkeypatch: pytest.MonkeyPatch) -> None:
    """If ASR status reports error, status.get should surface error state and detail."""
    fake_engine = SimpleNamespace(get_status=lambda: {"state": "error", "model_id": "mock-model"})
    fake_recorder = SimpleNamespace(state=SimpleNamespace(value="idle"))
    fake_tracker = SimpleNamespace(has_pending=lambda: False)

    monkeypatch.setattr("openvoicy_sidecar.server.get_engine", lambda: fake_engine)
    monkeypatch.setattr("openvoicy_sidecar.server.get_recorder", lambda: fake_recorder)
    monkeypatch.setattr("openvoicy_sidecar.server.get_session_tracker", lambda: fake_tracker)

    result = handle_status_get(Request(method="status.get", id=8))
    assert result["state"] == "error"
    assert isinstance(result.get("detail"), str)
