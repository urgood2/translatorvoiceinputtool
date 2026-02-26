import subprocess
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
BENCHMARK_SCRIPT = REPO_ROOT / "scripts" / "benchmark" / "latency.py"


class LatencyBenchmarkScriptTests(unittest.TestCase):
    def test_benchmark_script_exists_and_contains_required_latency_stages(self) -> None:
        content = BENCHMARK_SCRIPT.read_text(encoding="utf-8")

        self.assertIn("median(stop->injection) < 1200ms", content)
        self.assertIn("Warm-up run is executed and discarded", content)
        self.assertIn("ipc_ms", content)
        self.assertIn("transcribe_ms", content)
        self.assertIn("postprocess_ms", content)
        self.assertIn("inject_ms", content)
        self.assertIn("total_ms", content)
        self.assertIn("return 77", content)

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


if __name__ == "__main__":
    unittest.main()
