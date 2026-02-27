"""Fast sidecar self-test for CI and local sanity checks."""

from __future__ import annotations

import json
import os
import queue
import shlex
import subprocess
import sys
import threading
import time
from pathlib import Path
from typing import Any

from .resources import (
    CONTRACTS_DIR_REL,
    MODEL_CATALOG_REL,
    MODEL_MANIFEST_REL,
    MODEL_MANIFESTS_DIR_REL,
    PRESETS_REL,
    resolve_shared_path,
    resolve_shared_path_optional,
)

DEFAULT_RPC_TIMEOUT_SECONDS = 15.0
INITIAL_PING_MAX_ATTEMPTS = 3
INITIAL_PING_BACKOFF_SECONDS = 1.0
VALID_STATUS_STATES = {"idle", "loading_model", "recording", "transcribing", "error"}
VALID_STATUS_MODEL_STATES = {"missing", "downloading", "verifying", "ready", "error"}


class SelfTestError(RuntimeError):
    """Raised when a self-test step fails."""


def _log(message: str) -> None:
    print(f"[SELF_TEST] {message}", flush=True)


def _format_tail(lines: list[str], max_lines: int = 12) -> str:
    if not lines:
        return ""
    tail = "\n".join(lines[-max_lines:])
    return f"\n--- sidecar stderr tail ---\n{tail}"


def rpc_timeout_seconds() -> float:
    """Resolve RPC timeout from env with validation and safe fallback."""
    raw = os.environ.get("OPENVOICY_SELF_TEST_TIMEOUT_S", "").strip()
    if not raw:
        return DEFAULT_RPC_TIMEOUT_SECONDS
    try:
        value = float(raw)
    except ValueError:
        _log(
            "Invalid OPENVOICY_SELF_TEST_TIMEOUT_S value "
            f"{raw!r}; using default {DEFAULT_RPC_TIMEOUT_SECONDS:.1f}s"
        )
        return DEFAULT_RPC_TIMEOUT_SECONDS
    if value <= 0:
        _log(
            "OPENVOICY_SELF_TEST_TIMEOUT_S must be > 0; "
            f"using default {DEFAULT_RPC_TIMEOUT_SECONDS:.1f}s"
        )
        return DEFAULT_RPC_TIMEOUT_SECONDS
    return value


def build_sidecar_command() -> tuple[list[str], dict[str, str]]:
    """Build the sidecar launch command for dev and packaged environments."""
    env = os.environ.copy()

    override_cmd = env.get("OPENVOICY_SIDECAR_COMMAND", "").strip()
    if override_cmd:
        return shlex.split(override_cmd), env

    if getattr(sys, "frozen", False):
        return [sys.executable], env

    src_dir = Path(__file__).resolve().parents[1]
    src_path = str(src_dir)
    current_pythonpath = env.get("PYTHONPATH", "")

    if not current_pythonpath:
        env["PYTHONPATH"] = src_path
    else:
        parts = current_pythonpath.split(os.pathsep)
        if src_path not in parts:
            env["PYTHONPATH"] = src_path + os.pathsep + current_pythonpath

    return [sys.executable, "-m", "openvoicy_sidecar"], env


class SidecarRpcProcess:
    """Manage sidecar subprocess lifecycle and JSON-RPC calls."""

    def __init__(self, command: list[str], env: dict[str, str]):
        self._command = command
        self._env = env
        self._proc: subprocess.Popen[str] | None = None
        self._queue: queue.Queue[dict[str, Any]] = queue.Queue()
        self._stderr_lines: list[str] = []
        self._next_id = 1

    def start(self) -> None:
        self._proc = subprocess.Popen(
            self._command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
            env=self._env,
        )
        if self._proc.stdin is None or self._proc.stdout is None or self._proc.stderr is None:
            raise SelfTestError("Failed to initialize sidecar stdio pipes")

        threading.Thread(target=self._stdout_reader, daemon=True).start()
        threading.Thread(target=self._stderr_reader, daemon=True).start()

    def _stdout_reader(self) -> None:
        assert self._proc is not None and self._proc.stdout is not None
        for line in self._proc.stdout:
            text = line.strip()
            if not text:
                continue

            try:
                payload = json.loads(text)
            except json.JSONDecodeError:
                self._queue.put({"_parse_error": text})
                continue

            if isinstance(payload, dict):
                self._queue.put(payload)
            else:
                self._queue.put({"_invalid_payload": payload})

    def _stderr_reader(self) -> None:
        assert self._proc is not None and self._proc.stderr is not None
        for line in self._proc.stderr:
            text = line.strip()
            if text:
                self._stderr_lines.append(text)

    def call(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        if self._proc is None or self._proc.stdin is None:
            raise SelfTestError("Sidecar process is not started")
        if self._proc.poll() is not None:
            raise SelfTestError(
                f"Sidecar exited before request {method}" + _format_tail(self._stderr_lines)
            )

        request_id = self._next_id
        self._next_id += 1
        request: dict[str, Any] = {
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
        }
        if params is not None:
            request["params"] = params

        self._proc.stdin.write(json.dumps(request) + "\n")
        self._proc.stdin.flush()

        deadline = time.monotonic() + rpc_timeout_seconds()
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise SelfTestError(
                    f"Timed out waiting for response to {method}" + _format_tail(self._stderr_lines)
                )

            try:
                payload = self._queue.get(timeout=remaining)
            except queue.Empty as exc:
                raise SelfTestError(
                    f"Timed out waiting for response to {method}" + _format_tail(self._stderr_lines)
                ) from exc

            if "_parse_error" in payload:
                raise SelfTestError(
                    f"Received non-JSON line from sidecar stdout: {payload['_parse_error']}"
                    + _format_tail(self._stderr_lines)
                )
            if "_invalid_payload" in payload:
                raise SelfTestError(
                    f"Received non-object JSON payload from sidecar stdout: {payload['_invalid_payload']}"
                    + _format_tail(self._stderr_lines)
                )

            if payload.get("id") != request_id:
                continue

            if "error" in payload:
                raise SelfTestError(
                    f"{method} returned error: {payload['error']}" + _format_tail(self._stderr_lines)
                )

            result = payload.get("result")
            if not isinstance(result, dict):
                raise SelfTestError(
                    f"{method} result must be an object, got: {result!r}"
                    + _format_tail(self._stderr_lines)
                )
            return result

    def shutdown(self) -> int | None:
        """Send shutdown and wait for clean exit.

        Returns the process exit code, or None if no process was running.
        """
        if self._proc is None:
            return None
        if self._proc.poll() is not None:
            return self._proc.returncode

        try:
            if self._proc.stdin is not None:
                shutdown_req = {
                    "jsonrpc": "2.0",
                    "id": "self-test-shutdown",
                    "method": "system.shutdown",
                    "params": {"reason": "self_test"},
                }
                self._proc.stdin.write(json.dumps(shutdown_req) + "\n")
                self._proc.stdin.flush()
        except OSError:
            pass

        try:
            self._proc.wait(timeout=2.0)
        except subprocess.TimeoutExpired:
            self._proc.terminate()
            try:
                self._proc.wait(timeout=2.0)
            except subprocess.TimeoutExpired:
                self._proc.kill()
                self._proc.wait(timeout=2.0)

        return self._proc.returncode


def validate_ping_result(result: dict[str, Any]) -> None:
    if not isinstance(result.get("version"), str):
        raise SelfTestError("system.ping result.version must be a string")
    if result.get("protocol") != "v1":
        raise SelfTestError("system.ping result.protocol must be 'v1'")


def validate_system_info_result(result: dict[str, Any]) -> None:
    capabilities = result.get("capabilities")
    if not isinstance(capabilities, list) or not all(isinstance(cap, str) for cap in capabilities):
        raise SelfTestError("system.info result.capabilities must be string[]")

    runtime = result.get("runtime")
    if not isinstance(runtime, dict):
        raise SelfTestError("system.info result.runtime must be an object")

    if not isinstance(runtime.get("python_version"), str):
        raise SelfTestError("system.info runtime.python_version must be a string")
    if not isinstance(runtime.get("platform"), str):
        raise SelfTestError("system.info runtime.platform must be a string")
    if not isinstance(runtime.get("cuda_available"), bool):
        raise SelfTestError("system.info runtime.cuda_available must be a boolean")


def validate_status_get_result(result: dict[str, Any]) -> None:
    state = result.get("state")
    if state not in VALID_STATUS_STATES:
        raise SelfTestError(f"status.get result.state invalid: {state!r}")

    if "detail" in result and not isinstance(result["detail"], str):
        raise SelfTestError("status.get result.detail must be a string when present")

    if "model" in result:
        model = result["model"]
        if not isinstance(model, dict):
            raise SelfTestError("status.get result.model must be an object when present")
        if not isinstance(model.get("model_id"), str):
            raise SelfTestError("status.get model.model_id must be a string")
        if model.get("status") not in VALID_STATUS_MODEL_STATES:
            raise SelfTestError(
                "status.get model.status must be one of missing/downloading/verifying/ready/error"
            )


def validate_replacements_get_rules_result(result: dict[str, Any]) -> None:
    rules = result.get("rules")
    if not isinstance(rules, list):
        raise SelfTestError("replacements.get_rules result.rules must be an array")


def validate_clean_exit_code(exit_code: int | None) -> None:
    """Require graceful sidecar shutdown with exit code 0."""
    if exit_code is None:
        raise SelfTestError("Sidecar process was not running at shutdown")
    if exit_code != 0:
        raise SelfTestError(
            f"Sidecar did not exit cleanly after shutdown (expected 0, got {exit_code})"
        )


def validate_shared_resources() -> None:
    """Verify that essential shared resources are resolvable."""
    required_files = [
        (PRESETS_REL, "Replacement presets"),
        (MODEL_MANIFEST_REL, "Model manifest"),
        (MODEL_CATALOG_REL, "Model catalog"),
    ]
    for rel, label in required_files:
        path = resolve_shared_path_optional(rel)
        if path is None:
            raise SelfTestError(
                f"Required shared resource not found: {label} ({rel})"
            )
        if not path.is_file():
            raise SelfTestError(f"Required shared resource is not a file: {label} ({path})")
        _log(f"  {label}: {path}")

    required_dirs = [
        (CONTRACTS_DIR_REL, "Contracts"),
        (MODEL_MANIFESTS_DIR_REL, "Model manifests"),
    ]
    for rel, label in required_dirs:
        path = resolve_shared_path_optional(rel)
        if path is None:
            raise SelfTestError(f"Required shared resource directory not found: {label} ({rel})")
        if not path.is_dir():
            raise SelfTestError(f"Required shared resource is not a directory: {label} ({path})")
        _log(f"  {label}: {path}")


def validate_presets_loadable() -> None:
    """Verify that the presets file is valid JSON with expected structure."""
    path = resolve_shared_path(PRESETS_REL)
    try:
        with open(path) as f:
            data = json.load(f)
    except (json.JSONDecodeError, OSError) as e:
        raise SelfTestError(f"Failed to parse presets file {path}: {e}") from e

    if not isinstance(data, (list, dict)):
        raise SelfTestError(
            f"Presets file must be a JSON object or array, got {type(data).__name__}"
        )


def validate_model_manifest_loadable() -> None:
    """Verify that MODEL_MANIFEST.json is valid JSON with expected keys."""
    path = resolve_shared_path(MODEL_MANIFEST_REL)
    try:
        with open(path) as f:
            data = json.load(f)
    except (json.JSONDecodeError, OSError) as e:
        raise SelfTestError(f"Failed to parse model manifest {path}: {e}") from e

    if not isinstance(data, dict):
        raise SelfTestError("Model manifest must be a JSON object")

    if "model_id" not in data:
        raise SelfTestError("Model manifest missing required 'model_id' field")


def validate_model_catalog_loadable() -> None:
    """Verify that MODEL_CATALOG.json is valid JSON with expected structure."""
    path = resolve_shared_path(MODEL_CATALOG_REL)
    try:
        with open(path) as f:
            data = json.load(f)
    except (json.JSONDecodeError, OSError) as e:
        raise SelfTestError(f"Failed to parse model catalog {path}: {e}") from e

    if not isinstance(data, dict):
        raise SelfTestError("Model catalog must be a JSON object")

    models = data.get("models")
    if not isinstance(models, list):
        raise SelfTestError("Model catalog must contain a 'models' array")


def _run_step(label: str, fn) -> None:
    _log(f"Testing {label}...")
    try:
        fn()
    except Exception:
        _log(f"Testing {label}... FAIL")
        raise
    _log(f"Testing {label}... OK")


def _call_initial_ping_with_retry(sidecar: SidecarRpcProcess) -> dict[str, Any]:
    """Retry initial ping on cold-start timeout with bounded backoff."""
    for attempt in range(1, INITIAL_PING_MAX_ATTEMPTS + 1):
        try:
            return sidecar.call("system.ping")
        except SelfTestError as exc:
            error_text = str(exc)
            # True startup failures should fail fast.
            if "Sidecar exited before request system.ping" in error_text:
                raise
            if "Timed out waiting for response to system.ping" not in error_text:
                raise
            if attempt >= INITIAL_PING_MAX_ATTEMPTS:
                raise
            delay_s = INITIAL_PING_BACKOFF_SECONDS * attempt
            _log(
                "system.ping startup attempt "
                f"{attempt}/{INITIAL_PING_MAX_ATTEMPTS} failed ({exc}); "
                f"retrying in {delay_s:.1f}s"
            )
            time.sleep(delay_s)
    raise SelfTestError("initial system.ping failed unexpectedly")


def run_self_test() -> None:
    # Phase 1: Static resource resolution (no subprocess needed)
    _run_step("shared resource resolution", validate_shared_resources)
    _run_step("presets loadable", validate_presets_loadable)
    _run_step("model manifest loadable", validate_model_manifest_loadable)
    _run_step("model catalog loadable", validate_model_catalog_loadable)

    # Phase 2: Live sidecar process validation
    command, env = build_sidecar_command()
    _log(f"Starting sidecar process: {' '.join(command)}")

    sidecar = SidecarRpcProcess(command, env)
    sidecar.start()

    try:
        _run_step("system.ping", lambda: validate_ping_result(_call_initial_ping_with_retry(sidecar)))
        _run_step("system.info", lambda: validate_system_info_result(sidecar.call("system.info")))
        _run_step("status.get", lambda: validate_status_get_result(sidecar.call("status.get")))
        _run_step(
            "replacements.get_rules",
            lambda: validate_replacements_get_rules_result(sidecar.call("replacements.get_rules")),
        )
    finally:
        exit_code = sidecar.shutdown()

    # Phase 3: Verify clean exit (graceful shutdown only).
    _run_step("clean exit after shutdown", lambda: validate_clean_exit_code(exit_code))


def main() -> int:
    try:
        run_self_test()
        _log("PASS: All checks passed")
        return 0
    except SelfTestError as exc:
        _log(f"FAIL: {exc}")
        return 1
    except Exception as exc:
        _log(f"FAIL: Unexpected error: {exc}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
