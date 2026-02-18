"""Regression tests for status.get response shape."""

from __future__ import annotations

import json
import logging
import os
import subprocess
import sys
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import pytest

# Add src to path for imports
src_path = Path(__file__).parent.parent / "src"
sys.path.insert(0, str(src_path))

from openvoicy_sidecar.protocol import Request
from openvoicy_sidecar.server import handle_status_get

logger = logging.getLogger(__name__)

VALID_STATES = {"idle", "loading_model", "recording", "transcribing", "error"}
VALID_MODEL_STATES = {"missing", "downloading", "verifying", "ready", "error"}


@pytest.fixture
def run_sidecar():
    """Run the sidecar with input lines and capture responses/stderr."""

    def _run(input_lines: list[str], timeout: float = 5.0) -> tuple[list[dict], list[str]]:
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

        responses: list[dict] = []
        for line in proc.stdout.strip().split("\n"):
            if line.strip():
                responses.append(json.loads(line))

        stderr_lines = [line for line in proc.stderr.strip().split("\n") if line.strip()]
        return responses, stderr_lines

    return _run


def _assert_status_shape(result: dict) -> None:
    state = result.get("state")
    assert isinstance(state, str), f"status.get state must be string; got {type(state).__name__}"
    assert state in VALID_STATES, f"status.get state invalid; expected {VALID_STATES}, got {state!r}"

    if "detail" in result:
        detail = result["detail"]
        assert isinstance(detail, str), f"status.get detail must be string; got {type(detail).__name__}"

    if "model" in result:
        model = result["model"]
        assert isinstance(model, dict), f"status.get model must be object; got {type(model).__name__}"
        model_id = model.get("model_id")
        model_status = model.get("status")
        assert isinstance(
            model_id, str
        ), f"status.get model.model_id must be string; got {type(model_id).__name__}"
        assert (
            model_status in VALID_MODEL_STATES
        ), f"status.get model.status invalid; expected {VALID_MODEL_STATES}, got {model_status!r}"


def test_status_get_shape_no_model_loaded(run_sidecar):
    """With no model loaded, status.get should be idle and model may be absent."""
    responses, _ = run_sidecar(['{"jsonrpc":"2.0","id":10,"method":"status.get"}'])
    assert len(responses) == 1, f"expected 1 response, got {len(responses)}"

    response = responses[0]
    result = response["result"]
    logger.info("status.get response: %s", json.dumps(response))

    _assert_status_shape(result)
    assert result["state"] == "idle", f"expected idle with no model loaded, got {result['state']!r}"


def test_status_get_shape_with_mock_model_loaded():
    """When model status is mocked as ready, status.get should include model shape."""
    fake_engine = SimpleNamespace(get_status=lambda: {"state": "ready", "model_id": "mock-model"})
    fake_recorder = SimpleNamespace(state=SimpleNamespace(value="idle"))
    fake_tracker = SimpleNamespace(has_pending=lambda: False)

    with (
        patch("openvoicy_sidecar.server.get_engine", return_value=fake_engine),
        patch("openvoicy_sidecar.server.get_recorder", return_value=fake_recorder),
        patch("openvoicy_sidecar.server.get_session_tracker", return_value=fake_tracker),
    ):
        result = handle_status_get(Request(method="status.get", id=11))

    logger.info("status.get response: %s", json.dumps(result))
    _assert_status_shape(result)
    assert "model" in result, f"expected model field when model is loaded; got {result}"


def test_status_get_loading_model_state_and_status_mapping():
    """ASR loading/downloading should map to status.get loading_model with protocol model status."""
    fake_recorder = SimpleNamespace(state=SimpleNamespace(value="idle"))
    fake_tracker = SimpleNamespace(has_pending=lambda: False)

    with (
        patch(
            "openvoicy_sidecar.server.get_engine",
            return_value=SimpleNamespace(
                get_status=lambda: {"state": "downloading", "model_id": "mock-model"}
            ),
        ),
        patch("openvoicy_sidecar.server.get_recorder", return_value=fake_recorder),
        patch("openvoicy_sidecar.server.get_session_tracker", return_value=fake_tracker),
    ):
        downloading = handle_status_get(Request(method="status.get", id=12))

    _assert_status_shape(downloading)
    assert downloading["state"] == "loading_model"
    assert downloading["model"]["status"] == "downloading"

    with (
        patch(
            "openvoicy_sidecar.server.get_engine",
            return_value=SimpleNamespace(get_status=lambda: {"state": "loading", "model_id": "mock-model"}),
        ),
        patch("openvoicy_sidecar.server.get_recorder", return_value=fake_recorder),
        patch("openvoicy_sidecar.server.get_session_tracker", return_value=fake_tracker),
    ):
        loading = handle_status_get(Request(method="status.get", id=13))

    _assert_status_shape(loading)
    assert loading["state"] == "loading_model"
    assert loading["model"]["status"] == "verifying"
