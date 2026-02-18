#!/usr/bin/env python3
"""Collect packaging evidence for a built sidecar binary."""

from __future__ import annotations

import argparse
import glob
import hashlib
import json
import os
import platform
import shutil
import subprocess
import time
from pathlib import Path
from typing import Any


def run_request(binary_path: str, payload: dict[str, Any], timeout: float = 20.0) -> dict[str, Any]:
    """Run one JSON-RPC request against the sidecar process."""
    request_line = json.dumps(payload, separators=(",", ":")) + "\n"
    proc = subprocess.run(
        [binary_path],
        input=request_line,
        capture_output=True,
        text=True,
        timeout=timeout,
    )

    response_lines = [line.strip() for line in proc.stdout.splitlines() if line.strip()]
    first_line = response_lines[0] if response_lines else ""
    parsed: dict[str, Any] | None = None
    if first_line:
        try:
            loaded = json.loads(first_line)
            if isinstance(loaded, dict):
                parsed = loaded
        except json.JSONDecodeError:
            parsed = None

    return {
        "request": payload,
        "returncode": proc.returncode,
        "raw_response": first_line,
        "parsed_response": parsed,
        "stderr_tail": proc.stderr.splitlines()[-20:],
    }


def measure_cold_startup_ms(binary_path: str) -> tuple[float, dict[str, Any]]:
    """Measure cold startup by timing a system.ping request."""
    start = time.perf_counter()
    ping_result = run_request(binary_path, {"jsonrpc": "2.0", "id": 1, "method": "system.ping"})
    elapsed_ms = (time.perf_counter() - start) * 1000.0
    return elapsed_ms, ping_result


def sha256_file(path: str) -> str:
    """Compute SHA-256 hash for a file."""
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def run_dependency_probe(binary_path: str) -> dict[str, Any]:
    """Attempt to collect native dependency footprint details."""
    system = platform.system().lower()
    candidates: list[list[str]]
    if system == "windows":
        candidates = [
            ["dumpbin", "/DEPENDENTS", binary_path],
            ["llvm-objdump", "-p", binary_path],
            ["objdump", "-p", binary_path],
        ]
    elif system == "darwin":
        candidates = [["otool", "-L", binary_path]]
    else:
        candidates = [["ldd", binary_path]]

    chosen: list[str] | None = None
    for cmd in candidates:
        if shutil.which(cmd[0]):
            chosen = cmd
            break

    if chosen is None:
        return {
            "available": False,
            "command": None,
            "returncode": None,
            "lines": [],
        }

    proc = subprocess.run(chosen, capture_output=True, text=True, timeout=30.0)
    output_lines = [line.rstrip() for line in (proc.stdout + "\n" + proc.stderr).splitlines() if line.strip()]
    return {
        "available": True,
        "command": " ".join(chosen),
        "returncode": proc.returncode,
        "lines": output_lines[:120],
    }


def resolve_binary_path(binary_glob: str) -> str:
    """Resolve built sidecar binary from a glob pattern."""
    matches = [path for path in glob.glob(binary_glob) if os.path.isfile(path)]
    if not matches:
        raise FileNotFoundError(f"No binary matched: {binary_glob}")
    matches.sort()
    return matches[0]


def assert_smoke_ok(smoke: dict[str, dict[str, Any]]) -> None:
    """Raise RuntimeError when smoke responses violate IPC expectations."""

    failures: list[str] = []

    ping = smoke.get("system.ping", {})
    ping_payload = ping.get("parsed_response")
    if ping.get("returncode") != 0:
        failures.append("system.ping process returned non-zero exit code")
    elif not isinstance(ping_payload, dict):
        failures.append("system.ping did not return parseable JSON")
    elif ping_payload.get("result", {}).get("protocol") != "v1":
        failures.append("system.ping response missing result.protocol=v1")

    list_devices = smoke.get("audio.list_devices", {})
    list_payload = list_devices.get("parsed_response")
    if list_devices.get("returncode") != 0:
        failures.append("audio.list_devices process returned non-zero exit code")
    elif not isinstance(list_payload, dict):
        failures.append("audio.list_devices did not return parseable JSON")
    elif not isinstance(list_payload.get("result", {}).get("devices"), list):
        failures.append("audio.list_devices response missing result.devices list")

    meter = smoke.get("audio.meter_start", {})
    meter_payload = meter.get("parsed_response")
    if meter.get("returncode") != 0:
        failures.append("audio.meter_start process returned non-zero exit code")
    elif not isinstance(meter_payload, dict):
        failures.append("audio.meter_start did not return parseable JSON")
    elif "result" not in meter_payload and "error" not in meter_payload:
        failures.append("audio.meter_start response missing result/error payload")

    if failures:
        raise RuntimeError("; ".join(failures))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-glob", required=True, help="Glob for sidecar binary path")
    parser.add_argument("--platform", required=True, help="Target platform label (e.g., windows-x64)")
    parser.add_argument("--output", required=True, help="Output JSON path")
    args = parser.parse_args()

    binary_path = resolve_binary_path(args.binary_glob)
    file_size_bytes = os.path.getsize(binary_path)
    startup_ms, ping_result = measure_cold_startup_ms(binary_path)

    smoke_requests = {
        "system.ping": ping_result,
        "audio.list_devices": run_request(
            binary_path, {"jsonrpc": "2.0", "id": 2, "method": "audio.list_devices"}
        ),
        "audio.meter_start": run_request(
            binary_path, {"jsonrpc": "2.0", "id": 3, "method": "audio.meter_start"}
        ),
    }

    evidence = {
        "platform": args.platform,
        "runner_os": platform.platform(),
        "binary_path": binary_path,
        "binary_name": Path(binary_path).name,
        "binary_size_bytes": file_size_bytes,
        "binary_size_mb": round(file_size_bytes / (1024 * 1024), 2),
        "binary_sha256": sha256_file(binary_path),
        "cold_startup_ms": round(startup_ms, 2),
        "dependency_footprint": run_dependency_probe(binary_path),
        "smoke": smoke_requests,
    }

    assert_smoke_ok(smoke_requests)

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(evidence, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
