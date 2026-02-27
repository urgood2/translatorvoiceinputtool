import importlib.util
import subprocess
import sys
import tempfile
import unittest
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


if __name__ == "__main__":
    unittest.main()
