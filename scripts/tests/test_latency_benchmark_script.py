import importlib.util
import io
import json
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock


REPO_ROOT = Path(__file__).resolve().parents[2]
BENCHMARK_SCRIPT = REPO_ROOT / "scripts" / "benchmark" / "latency.py"

_LATENCY_SPEC = importlib.util.spec_from_file_location(
    "latency_benchmark_script", BENCHMARK_SCRIPT
)
assert _LATENCY_SPEC is not None
assert _LATENCY_SPEC.loader is not None
latency_benchmark = importlib.util.module_from_spec(_LATENCY_SPEC)
sys.modules[_LATENCY_SPEC.name] = latency_benchmark
_LATENCY_SPEC.loader.exec_module(latency_benchmark)


class LatencyBenchmarkScriptTests(unittest.TestCase):
    def test_benchmark_script_exists_and_contains_required_latency_stages(self) -> None:
        content = BENCHMARK_SCRIPT.read_text(encoding="utf-8")

        self.assertIn("median(stop->injection) < 1200ms", content)
        self.assertIn("Warm-up run is executed and discarded", content)
        self.assertIn("ipc_ms", content)
        self.assertIn("transcribe_ms", content)
        self.assertIn("postprocess_ms", content)
        self.assertIn("measured_ms", content)
        self.assertIn("inject_budget_ms", content)
        self.assertIn("projected_total_ms", content)
        self.assertIn("return 77", content)
        self.assertNotIn("inject_delay_ms", content)
        self.assertNotIn("simulated host injection delay", content)
        self.assertNotIn("time.sleep(inject", content)
        self.assertIn("numpy unavailable; cannot synthesize benchmark waveform", content)
        self.assertIn("except ModuleNotFoundError as exc", content)
        self.assertIn("raise BenchmarkSkip", content)

    def test_benchmark_script_cli_help(self) -> None:
        result = subprocess.run(
            ["python3", str(BENCHMARK_SCRIPT), "--help"],
            cwd=REPO_ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, msg=result.stderr)
        self.assertIn("--runs", result.stdout)
        self.assertIn("--target-ms", result.stdout)
        self.assertIn("--strict", result.stdout)
        self.assertIn("--json-out", result.stdout)
        self.assertNotIn("--inject-delay-ms", result.stdout)
        self.assertIn("--inject-budget-ms", result.stdout)

    def test_run_iteration_skips_when_numpy_missing_for_audio_generation(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            with mock.patch.object(
                latency_benchmark,
                "generate_sine_wav",
                side_effect=latency_benchmark.BenchmarkSkip(
                    "numpy unavailable; cannot synthesize benchmark waveform"
                ),
            ):
                with self.assertRaises(latency_benchmark.BenchmarkSkip) as ctx:
                    latency_benchmark.run_iteration(
                        client=object(),  # Not used before generation failure.
                        temp_dir=Path(tmp_dir),
                        index=1,
                        duration_s=1.0,
                        inject_budget_ms=50,
                        playback_required=True,
                    )
        self.assertIn("numpy unavailable", str(ctx.exception))

    def test_main_writes_runtime_json_report_contract(self) -> None:
        class FakeClient:
            def __init__(self, sidecar_bin: Path) -> None:
                self.sidecar_bin = sidecar_bin

            def start(self) -> None:
                return None

            def call(self, method: str, params: dict[str, object], timeout_s: float) -> dict[str, str]:
                self._last_call = (method, params, timeout_s)
                return {"protocol": "v1", "server": "mock"}

            def stop(self) -> None:
                return None

        def fake_run_iteration(
            client: object,
            temp_dir: Path,
            index: int,
            duration_s: float,
            inject_budget_ms: int,
            playback_required: bool,
        ) -> latency_benchmark.RunTimings:
            measured_ms = 300 + index
            return latency_benchmark.RunTimings(
                index=index,
                session_id=f"mock-session-{index}",
                duration_s=duration_s,
                ipc_ms=80,
                transcribe_ms=180,
                postprocess_ms=40,
                measured_ms=measured_ms,
                inject_budget_ms=inject_budget_ms,
                projected_total_ms=measured_ms + inject_budget_ms,
                text_preview="mock transcript",
                t0_iso="2026-01-01T00:00:00+00:00",
                t1_iso="2026-01-01T00:00:01+00:00",
                t2_iso="2026-01-01T00:00:02+00:00",
                t3_iso="2026-01-01T00:00:03+00:00",
            )

        with tempfile.TemporaryDirectory() as tmp_dir:
            report_path = Path(tmp_dir) / "latency-report.json"
            stdout = io.StringIO()
            stderr = io.StringIO()

            argv = [
                "latency.py",
                "--runs",
                "3",
                "--target-ms",
                "1200",
                "--inject-budget-ms",
                "50",
                "--json-out",
                str(report_path),
                "--no-playback-required",
            ]

            with mock.patch.object(sys, "argv", argv):
                with mock.patch.object(latency_benchmark, "SidecarClient", FakeClient):
                    with mock.patch.object(latency_benchmark, "ensure_model_available"):
                        with mock.patch.object(latency_benchmark, "initialize_model"):
                            with mock.patch.object(latency_benchmark, "run_iteration", side_effect=fake_run_iteration):
                                with redirect_stdout(stdout), redirect_stderr(stderr):
                                    rc = latency_benchmark.main()

            self.assertEqual(rc, 0, msg=stderr.getvalue())
            self.assertTrue(report_path.is_file())

            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertIn("generated_at", report)
            self.assertIn("summary", report)
            self.assertIn("runs", report)
            self.assertEqual(len(report["runs"]), 3)
            self.assertEqual(report["summary"]["count"], 3)
            self.assertEqual(report["summary"]["inject_budget_ms"], 50)
            self.assertIn("projected_median_ms", report["summary"])
            self.assertIn("median_breakdown_ms", report["summary"])
            self.assertIn("ipc", report["summary"]["median_breakdown_ms"])
            self.assertIn("transcribe", report["summary"]["median_breakdown_ms"])
            self.assertIn("postprocess", report["summary"]["median_breakdown_ms"])
            self.assertIn("Latency benchmark (3 runs, after model warm):", stdout.getvalue())

    def test_main_returns_ci_informational_success_when_threshold_exceeded(self) -> None:
        class FakeClient:
            def __init__(self, sidecar_bin: Path) -> None:
                self.sidecar_bin = sidecar_bin

            def start(self) -> None:
                return None

            def call(self, method: str, params: dict[str, object], timeout_s: float) -> dict[str, str]:
                return {"protocol": "v1", "server": "mock"}

            def stop(self) -> None:
                return None

        def slow_run_iteration(
            client: object,
            temp_dir: Path,
            index: int,
            duration_s: float,
            inject_budget_ms: int,
            playback_required: bool,
        ) -> latency_benchmark.RunTimings:
            measured_ms = 2000 + index
            return latency_benchmark.RunTimings(
                index=index,
                session_id=f"mock-session-{index}",
                duration_s=duration_s,
                ipc_ms=400,
                transcribe_ms=1200,
                postprocess_ms=400,
                measured_ms=measured_ms,
                inject_budget_ms=inject_budget_ms,
                projected_total_ms=measured_ms + inject_budget_ms,
                text_preview="slow transcript",
                t0_iso="2026-01-01T00:00:00+00:00",
                t1_iso="2026-01-01T00:00:01+00:00",
                t2_iso="2026-01-01T00:00:02+00:00",
                t3_iso="2026-01-01T00:00:03+00:00",
            )

        stdout = io.StringIO()
        stderr = io.StringIO()
        argv = [
            "latency.py",
            "--runs",
            "2",
            "--target-ms",
            "1200",
            "--inject-budget-ms",
            "50",
            "--no-playback-required",
        ]

        with mock.patch.object(sys, "argv", argv):
            with mock.patch.dict("os.environ", {"CI": "true"}, clear=False):
                with mock.patch.object(latency_benchmark, "SidecarClient", FakeClient):
                    with mock.patch.object(latency_benchmark, "ensure_model_available"):
                        with mock.patch.object(latency_benchmark, "initialize_model"):
                            with mock.patch.object(latency_benchmark, "run_iteration", side_effect=slow_run_iteration):
                                with redirect_stdout(stdout), redirect_stderr(stderr):
                                    rc = latency_benchmark.main()

        self.assertEqual(rc, 0, msg=stderr.getvalue())
        self.assertIn("[warn] projected median", stdout.getvalue())


if __name__ == "__main__":
    unittest.main()
