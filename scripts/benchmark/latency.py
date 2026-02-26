#!/usr/bin/env python3
"""Stop-to-injection latency benchmark for sidecar recording flow.

This benchmark targets the phase-0 success metric:
  median(stop->injection) < 1200ms after model warm-up.

It runs recording.start/stop cycles against the sidecar using synthetic
1-3 second tones, captures per-run timings (T0..T4), and prints a summary.

Exit codes:
  0   success (or informational-only failure in CI mode)
  1   benchmark failed threshold / runtime error
  77  skipped (model unavailable or missing audio prerequisites)
"""

from __future__ import annotations

import argparse
import json
import math
import os
import random
import select
import statistics
import subprocess
import sys
import tempfile
import time
import uuid
import wave
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


class BenchmarkSkip(RuntimeError):
    """Raised when benchmark prerequisites are unavailable."""


class BenchmarkFailure(RuntimeError):
    """Raised when benchmark execution fails."""


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def to_ms(seconds: float) -> int:
    return int(round(seconds * 1000.0))


def percentile(values: list[int], p: float) -> int:
    """Compute nearest-rank percentile (1-indexed rank)."""
    if not values:
        raise ValueError("percentile requires non-empty list")
    if p <= 0:
        return min(values)
    if p >= 1:
        return max(values)
    ordered = sorted(values)
    rank = max(1, int(math.ceil(p * len(ordered))))
    return ordered[rank - 1]


def is_ci() -> bool:
    value = os.environ.get("CI", "").strip().lower()
    return value in {"1", "true", "yes", "on"}


def looks_like_model_unavailable(error: dict[str, Any]) -> bool:
    kind = str(error.get("data", {}).get("kind", "")).lower()
    message = str(error.get("message", "")).lower()
    if kind in {"e_model_not_found", "e_model_load", "e_not_ready"}:
        return True
    if "model" in message and any(
        token in message for token in ("not found", "missing", "not initialized")
    ):
        return True
    return False


def looks_like_audio_unavailable(error: dict[str, Any]) -> bool:
    kind = str(error.get("data", {}).get("kind", "")).lower()
    message = str(error.get("message", "")).lower()
    if kind in {"e_audio_io", "e_audio_device"}:
        return True
    if any(token in message for token in ("audio", "microphone", "device")) and any(
        token in message for token in ("unavailable", "no device", "not found", "failed")
    ):
        return True
    return False


@dataclass
class RunTimings:
    index: int
    session_id: str
    duration_s: float
    ipc_ms: int
    transcribe_ms: int
    postprocess_ms: int
    inject_ms: int
    total_ms: int
    text_preview: str
    t0_iso: str
    t1_iso: str
    t2_iso: str
    t3_iso: str
    t4_iso: str


class SidecarClient:
    def __init__(self, sidecar_bin: Path) -> None:
        self.sidecar_bin = sidecar_bin
        self.proc: subprocess.Popen[str] | None = None
        self._next_id = 1
        self._pending_notifications: list[dict[str, Any]] = []

    def start(self) -> None:
        if not self.sidecar_bin.exists():
            raise BenchmarkFailure(f"sidecar binary not found: {self.sidecar_bin}")
        self.proc = subprocess.Popen(
            [str(self.sidecar_bin)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        time.sleep(0.2)
        if self.proc.poll() is not None:
            stderr = ""
            if self.proc.stderr is not None:
                stderr = self.proc.stderr.read().strip()
            raise BenchmarkFailure(f"sidecar failed to start: {stderr}")

    def stop(self) -> None:
        if self.proc is None:
            return
        try:
            self.call("system.shutdown", {}, timeout_s=5.0)
        except Exception:
            pass
        try:
            self.proc.terminate()
            self.proc.wait(timeout=2.0)
        except Exception:
            self.proc.kill()
        self.proc = None

    def _require_live_proc(self) -> subprocess.Popen[str]:
        if self.proc is None or self.proc.stdin is None or self.proc.stdout is None:
            raise BenchmarkFailure("sidecar process is not running")
        if self.proc.poll() is not None:
            raise BenchmarkFailure("sidecar process exited unexpectedly")
        return self.proc

    def _send_json(self, payload: dict[str, Any]) -> None:
        proc = self._require_live_proc()
        assert proc.stdin is not None
        proc.stdin.write(json.dumps(payload) + "\n")
        proc.stdin.flush()

    def _read_line(self, timeout_s: float) -> str | None:
        proc = self._require_live_proc()
        assert proc.stdout is not None
        deadline = time.monotonic() + timeout_s
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return None
            ready, _, _ = select.select([proc.stdout], [], [], remaining)
            if not ready:
                return None
            line = proc.stdout.readline()
            if not line:
                if proc.poll() is not None:
                    raise BenchmarkFailure("sidecar exited while waiting for response")
                continue
            stripped = line.strip()
            if not stripped:
                continue
            return stripped

    def call(self, method: str, params: dict[str, Any], timeout_s: float) -> dict[str, Any]:
        req_id = self._next_id
        self._next_id += 1
        self._send_json(
            {"jsonrpc": "2.0", "id": req_id, "method": method, "params": params}
        )
        deadline = time.monotonic() + timeout_s
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise BenchmarkFailure(f"timeout waiting for response: {method}")
            line = self._read_line(remaining)
            if line is None:
                raise BenchmarkFailure(f"timeout waiting for response: {method}")
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            if "method" in message and "id" not in message:
                self._pending_notifications.append(message)
                continue
            if message.get("id") != req_id:
                # Ignore unrelated response IDs (should not happen in sequential flow).
                continue
            if "error" in message:
                raise BenchmarkFailure(
                    f"{method} returned error: {json.dumps(message['error'])}"
                )
            return message.get("result", {})

    def call_raw(
        self, method: str, params: dict[str, Any], timeout_s: float
    ) -> dict[str, Any]:
        """Call and return full JSON-RPC response envelope."""
        req_id = self._next_id
        self._next_id += 1
        self._send_json(
            {"jsonrpc": "2.0", "id": req_id, "method": method, "params": params}
        )
        deadline = time.monotonic() + timeout_s
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise BenchmarkFailure(f"timeout waiting for response: {method}")
            line = self._read_line(remaining)
            if line is None:
                raise BenchmarkFailure(f"timeout waiting for response: {method}")
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            if "method" in message and "id" not in message:
                self._pending_notifications.append(message)
                continue
            if message.get("id") != req_id:
                continue
            return message

    def wait_notification(
        self, method: str, session_id: str, timeout_s: float
    ) -> dict[str, Any]:
        deadline = time.monotonic() + timeout_s
        while True:
            # Consume queued notifications first.
            for idx, message in enumerate(list(self._pending_notifications)):
                if message.get("method") != method:
                    continue
                params = message.get("params", {})
                if params.get("session_id") == session_id:
                    del self._pending_notifications[idx]
                    return message

            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise BenchmarkFailure(
                    f"timeout waiting for notification: {method} session={session_id}"
                )
            line = self._read_line(remaining)
            if line is None:
                raise BenchmarkFailure(
                    f"timeout waiting for notification: {method} session={session_id}"
                )
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            if "method" in message and "id" not in message:
                self._pending_notifications.append(message)
                continue
            # Ignore response envelopes while waiting for async event.


def generate_sine_wav(path: Path, duration_s: float, frequency_hz: float = 440.0) -> None:
    import numpy as np

    sample_rate = 16000
    amplitude = 0.35
    sample_count = int(sample_rate * duration_s)
    t = np.arange(sample_count, dtype=np.float32) / sample_rate
    audio = (amplitude * np.sin(2.0 * math.pi * frequency_hz * t)).astype(np.float32)
    pcm16 = (audio * 32767.0).clip(-32768, 32767).astype(np.int16)
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(sample_rate)
        wav.writeframes(pcm16.tobytes())


def play_audio(path: Path) -> None:
    import numpy as np
    from scipy.io import wavfile
    import sounddevice as sd

    sample_rate, audio = wavfile.read(str(path))
    if audio.dtype == np.int16:
        audio_f = audio.astype(np.float32) / 32768.0
    elif audio.dtype == np.int32:
        audio_f = audio.astype(np.float32) / 2147483648.0
    else:
        audio_f = audio.astype(np.float32)
    if audio_f.ndim > 1:
        audio_f = np.mean(audio_f, axis=1)
    sd.play(audio_f, sample_rate)
    sd.wait()


def format_text_preview(text: str) -> str:
    cleaned = " ".join(text.split())
    if len(cleaned) <= 60:
        return cleaned
    return f"{cleaned[:57]}..."


def run_iteration(
    client: SidecarClient,
    temp_dir: Path,
    index: int,
    duration_s: float,
    inject_delay_ms: int,
    playback_required: bool,
) -> RunTimings:
    session_id = f"latency-{int(time.time())}-{index}-{uuid.uuid4().hex[:8]}"
    wav_path = temp_dir / f"latency-{index:02d}.wav"
    frequency_hz = 340.0 + (index * 17.0)
    generate_sine_wav(wav_path, duration_s=duration_s, frequency_hz=frequency_hz)

    start_response = client.call_raw(
        "recording.start",
        {"session_id": session_id, "device_uid": None},
        timeout_s=15.0,
    )
    if "error" in start_response:
        error = start_response["error"]
        if looks_like_audio_unavailable(error):
            raise BenchmarkSkip(
                f"audio capture unavailable for recording.start: {json.dumps(error)}"
            )
        raise BenchmarkFailure(
            f"recording.start failed for session {session_id}: {json.dumps(error)}"
        )
    if start_response.get("result", {}).get("session_id") != session_id:
        raise BenchmarkFailure("recording.start returned mismatched session_id")

    try:
        play_audio(wav_path)
    except ModuleNotFoundError as exc:
        if playback_required:
            raise BenchmarkSkip(
                f"audio playback dependency unavailable ({exc}); install sounddevice/scipy"
            ) from exc
    except Exception as exc:  # pragma: no cover - depends on host audio stack
        if playback_required:
            raise BenchmarkSkip(f"audio playback failed: {exc}") from exc

    t0_mono = time.monotonic()
    t0_iso = utc_now_iso()
    stop_response = client.call_raw(
        "recording.stop", {"session_id": session_id}, timeout_s=20.0
    )
    t1_mono = time.monotonic()
    t1_iso = utc_now_iso()
    if "error" in stop_response:
        raise BenchmarkFailure(
            f"recording.stop failed for session {session_id}: {json.dumps(stop_response['error'])}"
        )
    if stop_response.get("result", {}).get("session_id") != session_id:
        raise BenchmarkFailure("recording.stop returned mismatched session_id")

    notification = client.wait_notification(
        "event.transcription_complete", session_id=session_id, timeout_s=25.0
    )
    t2_mono = time.monotonic()
    t2_iso = utc_now_iso()
    params = notification.get("params", {})
    text = str(params.get("final_text") or params.get("text") or "")
    if not text.strip():
        raise BenchmarkFailure(f"empty transcription text for session {session_id}")

    # Post-process stage proxy: use replacements.preview if available.
    postprocess_start = time.monotonic()
    preview_response = client.call_raw(
        "replacements.preview",
        {"text": text, "rules": []},
        timeout_s=5.0,
    )
    if "error" in preview_response:
        # Older sidecar builds may not implement preview; keep timing budget with 0ms.
        t3_mono = postprocess_start
    else:
        t3_mono = time.monotonic()
    t3_iso = utc_now_iso()

    # Injection stage proxy: emulate host injection delay budget.
    if inject_delay_ms > 0:
        time.sleep(inject_delay_ms / 1000.0)
    t4_mono = time.monotonic()
    t4_iso = utc_now_iso()

    ipc_ms = to_ms(t1_mono - t0_mono)
    transcribe_ms = to_ms(t2_mono - t1_mono)
    postprocess_ms = to_ms(t3_mono - t2_mono)
    inject_ms = to_ms(t4_mono - t3_mono)
    total_ms = to_ms(t4_mono - t0_mono)

    return RunTimings(
        index=index,
        session_id=session_id,
        duration_s=duration_s,
        ipc_ms=ipc_ms,
        transcribe_ms=transcribe_ms,
        postprocess_ms=postprocess_ms,
        inject_ms=inject_ms,
        total_ms=total_ms,
        text_preview=format_text_preview(text),
        t0_iso=t0_iso,
        t1_iso=t1_iso,
        t2_iso=t2_iso,
        t3_iso=t3_iso,
        t4_iso=t4_iso,
    )


def summarize(runs: list[RunTimings], target_ms: int) -> dict[str, Any]:
    totals = [run.total_ms for run in runs]
    return {
        "count": len(runs),
        "median_ms": int(statistics.median(totals)),
        "p95_ms": percentile(totals, 0.95),
        "min_ms": min(totals),
        "max_ms": max(totals),
        "target_ms": target_ms,
        "median_breakdown_ms": {
            "ipc": int(statistics.median([run.ipc_ms for run in runs])),
            "transcribe": int(statistics.median([run.transcribe_ms for run in runs])),
            "postprocess": int(statistics.median([run.postprocess_ms for run in runs])),
            "inject": int(statistics.median([run.inject_ms for run in runs])),
        },
    }


def print_run(run: RunTimings) -> None:
    print(
        f"[run {run.index:02d}] session={run.session_id} duration={run.duration_s:.2f}s "
        f"total={run.total_ms}ms ipc={run.ipc_ms}ms transcribe={run.transcribe_ms}ms "
        f"post={run.postprocess_ms}ms inject={run.inject_ms}ms text='{run.text_preview}'"
    )
    print(
        f"           T0={run.t0_iso} T1={run.t1_iso} T2={run.t2_iso} T3={run.t3_iso} T4={run.t4_iso}"
    )


def print_summary(summary: dict[str, Any]) -> None:
    median_ok = summary["median_ms"] < summary["target_ms"]
    marker = "✓" if median_ok else "✗"
    b = summary["median_breakdown_ms"]
    print()
    print(f"Latency benchmark ({summary['count']} runs, after model warm):")
    print(
        f"  Median: {summary['median_ms']}ms (TARGET: <{summary['target_ms']}ms) {marker}"
    )
    print(f"  P95: {summary['p95_ms']}ms")
    print(f"  Min: {summary['min_ms']}ms, Max: {summary['max_ms']}ms")
    print(
        "  Breakdown (median): "
        f"IPC={b['ipc']}ms, Transcribe={b['transcribe']}ms, "
        f"PostProcess={b['postprocess']}ms, Inject={b['inject']}ms"
    )


def write_json_report(path: Path, runs: list[RunTimings], summary: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "generated_at": utc_now_iso(),
        "summary": summary,
        "runs": [run.__dict__ for run in runs],
    }
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def resolve_sidecar_binary(repo_root: Path) -> Path:
    if sys.platform.startswith("win"):
        return repo_root / "sidecar" / "dist" / "openvoicy-sidecar.exe"
    return repo_root / "sidecar" / "dist" / "openvoicy-sidecar"


def ensure_model_available(client: SidecarClient, model_id: str | None) -> None:
    status_params: dict[str, Any] = {}
    if model_id:
        status_params["model_id"] = model_id
    try:
        status_response = client.call_raw("model.get_status", status_params, timeout_s=8.0)
    except BenchmarkFailure:
        status_response = {"result": {}}
    if "error" in status_response and looks_like_model_unavailable(status_response["error"]):
        raise BenchmarkSkip("model not installed (model.get_status)")
    status = str(status_response.get("result", {}).get("status", "")).lower()
    if status in {"missing", "not_found", "error"}:
        raise BenchmarkSkip(f"model not ready: status={status}")


def initialize_model(client: SidecarClient, model_id: str | None, device_pref: str) -> None:
    params: dict[str, Any] = {"device_pref": device_pref}
    if model_id:
        params["model_id"] = model_id
    init = client.call_raw("asr.initialize", params, timeout_s=1200.0)
    if "error" in init:
        if looks_like_model_unavailable(init["error"]):
            raise BenchmarkSkip(f"model unavailable during asr.initialize: {init['error']}")
        raise BenchmarkFailure(f"asr.initialize failed: {json.dumps(init['error'])}")
    status = str(init.get("result", {}).get("status", "")).lower()
    if status != "ready":
        raise BenchmarkFailure(f"asr.initialize returned unexpected status={status!r}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", type=int, default=10, help="number of measured runs")
    parser.add_argument(
        "--target-ms",
        type=int,
        default=1200,
        help="median latency threshold in milliseconds",
    )
    parser.add_argument(
        "--model-id",
        type=str,
        default=None,
        help="optional model id for model.get_status/asr.initialize",
    )
    parser.add_argument(
        "--device-pref",
        type=str,
        default="auto",
        choices=("auto", "cpu", "cuda"),
        help="device preference passed to asr.initialize",
    )
    parser.add_argument(
        "--inject-delay-ms",
        type=int,
        default=50,
        help="simulated host injection delay in milliseconds",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="always fail non-zero when median exceeds target (even in CI)",
    )
    parser.add_argument(
        "--no-playback-required",
        action="store_true",
        help="allow benchmark to continue when playback dependency/device is unavailable",
    )
    parser.add_argument(
        "--json-out",
        type=str,
        default=None,
        help="optional output path for JSON benchmark report",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.runs <= 0:
        print("runs must be > 0", file=sys.stderr)
        return 1

    script_path = Path(__file__).resolve()
    repo_root = script_path.parents[2]
    sidecar_bin = resolve_sidecar_binary(repo_root)
    print(f"[setup] repo={repo_root}")
    print(f"[setup] sidecar={sidecar_bin}")

    client = SidecarClient(sidecar_bin)
    runs: list[RunTimings] = []

    try:
        client.start()
        ping = client.call("system.ping", {}, timeout_s=5.0)
        print(
            "[setup] system.ping protocol="
            f"{ping.get('protocol', 'unknown')} server={ping.get('server', 'unknown')}"
        )
        ensure_model_available(client, args.model_id)
        initialize_model(client, args.model_id, args.device_pref)

        # Warm-up run is executed and discarded.
        with tempfile.TemporaryDirectory(prefix="openvoicy-latency-warm-") as warm_dir:
            warm_run = run_iteration(
                client,
                Path(warm_dir),
                index=0,
                duration_s=1.25,
                inject_delay_ms=args.inject_delay_ms,
                playback_required=not args.no_playback_required,
            )
            print("[warmup] completed")
            print_run(warm_run)

        rng = random.Random(42)
        with tempfile.TemporaryDirectory(prefix="openvoicy-latency-runs-") as tmp:
            temp_dir = Path(tmp)
            for idx in range(1, args.runs + 1):
                duration_s = rng.uniform(1.0, 3.0)
                run = run_iteration(
                    client,
                    temp_dir,
                    index=idx,
                    duration_s=duration_s,
                    inject_delay_ms=args.inject_delay_ms,
                    playback_required=not args.no_playback_required,
                )
                runs.append(run)
                print_run(run)

        summary = summarize(runs, target_ms=args.target_ms)
        print_summary(summary)

        json_out = (
            Path(args.json_out)
            if args.json_out
            else repo_root
            / "logs"
            / "benchmark"
            / f"latency-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}.json"
        )
        write_json_report(json_out, runs, summary)
        print(f"[artifact] wrote report: {json_out}")

        if summary["median_ms"] >= args.target_ms:
            message = (
                f"median latency {summary['median_ms']}ms exceeds target "
                f"{args.target_ms}ms"
            )
            if is_ci() and not args.strict:
                print(f"[warn] {message} (informational in CI mode)")
                return 0
            print(f"[error] {message}", file=sys.stderr)
            return 1

        return 0
    except BenchmarkSkip as exc:
        print(f"[skip] {exc}")
        return 77
    except BenchmarkFailure as exc:
        print(f"[error] {exc}", file=sys.stderr)
        return 1
    finally:
        client.stop()


if __name__ == "__main__":
    sys.exit(main())
