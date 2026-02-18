"""Regression tests for system.info baseline contract fields."""

from __future__ import annotations

import json
import logging
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

# Add src to path for imports
src_path = Path(__file__).parent.parent / "src"
sys.path.insert(0, str(src_path))

logger = logging.getLogger(__name__)

EXPECTED_CORE_CAPABILITIES = {"asr", "replacements", "meter"}


@pytest.fixture
def run_sidecar():
    """Run the sidecar with input lines and capture responses/stderr."""

    def _run(input_lines: list[str], timeout: float = 5.0) -> tuple[list[dict[str, Any]], list[str]]:
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

        responses: list[dict[str, Any]] = []
        for line in proc.stdout.strip().split("\n"):
            if line.strip():
                responses.append(json.loads(line))

        stderr_lines = [line for line in proc.stderr.strip().split("\n") if line.strip()]
        return responses, stderr_lines

    return _run


def _assert_with_logging(name: str, passed: bool, *, expected: Any, actual: Any) -> None:
    """Log assertion status and expected/actual details."""
    logger.info("%s: %s", name, passed)
    if not passed:
        logger.error("%s failed; expected=%r actual=%r", name, expected, actual)
    assert passed, f"{name} failed; expected={expected!r} actual={actual!r}"


def test_system_info_baseline_fields(run_sidecar):
    """system.info should provide required baseline fields with additive compatibility."""
    responses, stderr_lines = run_sidecar(['{"jsonrpc":"2.0","id":2,"method":"system.info"}'])
    _assert_with_logging(
        "single response returned",
        len(responses) == 1,
        expected=1,
        actual=len(responses),
    )

    response = responses[0]
    result = response["result"]
    logger.info("system.info full response JSON: %s", json.dumps(result, sort_keys=True))
    if stderr_lines:
        logger.info("system.info stderr lines: %s", stderr_lines)

    capabilities = result.get("capabilities")
    logger.info(
        "capabilities present: %s, type: %s, length: %s",
        "capabilities" in result,
        type(capabilities).__name__,
        len(capabilities) if isinstance(capabilities, list) else "n/a",
    )
    _assert_with_logging(
        "capabilities is list",
        isinstance(capabilities, list),
        expected="list[str]",
        actual=type(capabilities).__name__,
    )
    _assert_with_logging(
        "capabilities list items are strings",
        all(isinstance(capability, str) for capability in capabilities),
        expected="all items are str",
        actual=capabilities,
    )

    runtime = result.get("runtime")
    logger.info("runtime present: %s, type: %s", "runtime" in result, type(runtime).__name__)
    _assert_with_logging(
        "runtime is object",
        isinstance(runtime, dict),
        expected="object",
        actual=type(runtime).__name__,
    )

    python_version = runtime.get("python_version") if isinstance(runtime, dict) else None
    platform_name = runtime.get("platform") if isinstance(runtime, dict) else None
    cuda_available = runtime.get("cuda_available") if isinstance(runtime, dict) else None

    _assert_with_logging(
        "runtime.python_version is string",
        isinstance(python_version, str),
        expected="str",
        actual=python_version,
    )
    _assert_with_logging(
        "runtime.platform is string",
        isinstance(platform_name, str),
        expected="str",
        actual=platform_name,
    )
    _assert_with_logging(
        "runtime.cuda_available is boolean",
        isinstance(cuda_available, bool),
        expected="bool",
        actual=cuda_available,
    )

    baseline_consumer_view = {
        "capabilities": capabilities,
        "runtime": {
            "python_version": python_version,
            "platform": platform_name,
            "cuda_available": cuda_available,
        },
    }
    logger.info("baseline consumer view JSON: %s", json.dumps(baseline_consumer_view, sort_keys=True))
    logger.info(
        "additional top-level fields allowed: %s",
        sorted(set(result.keys()) - {"version", "protocol", "capabilities", "runtime"}),
    )

    missing_core_capabilities = sorted(EXPECTED_CORE_CAPABILITIES - set(capabilities))
    logger.info(
        "core capabilities expected=%s actual=%s missing=%s",
        sorted(EXPECTED_CORE_CAPABILITIES),
        capabilities,
        missing_core_capabilities,
    )
    _assert_with_logging(
        "core capabilities present",
        not missing_core_capabilities,
        expected=sorted(EXPECTED_CORE_CAPABILITIES),
        actual=capabilities,
    )
