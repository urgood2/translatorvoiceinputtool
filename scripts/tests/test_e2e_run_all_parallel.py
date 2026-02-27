import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
RUN_ALL_SRC = REPO_ROOT / "scripts" / "e2e" / "run-all.sh"


class E2ERunAllParallelTests(unittest.TestCase):
    def test_parallel_mode_does_not_use_local_outside_function(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            e2e_dir = tmp / "scripts" / "e2e"
            lib_dir = e2e_dir / "lib"
            e2e_dir.mkdir(parents=True)
            lib_dir.mkdir(parents=True)

            run_all_dst = e2e_dir / "run-all.sh"
            shutil.copy2(RUN_ALL_SRC, run_all_dst)

            # Minimal test scripts so --parallel path executes.
            for name in ("test-alpha.sh", "test-beta.sh"):
                script = e2e_dir / name
                script.write_text("#!/usr/bin/env bash\nset -euo pipefail\nexit 0\n")
                script.chmod(0o755)

            result = subprocess.run(
                ["bash", str(run_all_dst), "--parallel"],
                cwd=tmp,
                capture_output=True,
                text=True,
                check=False,
            )

            combined = result.stdout + "\n" + result.stderr
            self.assertNotIn("can only be used in a function", combined)
            self.assertIn("Running tests in parallel...", result.stdout)
            self.assertIn("Detailed Results:", result.stdout)
            self.assertIn("test-alpha", result.stdout)
            self.assertIn("test-beta", result.stdout)


class E2ERunAllContractTests(unittest.TestCase):
    def test_run_all_includes_required_ordered_suite_and_summary_contract(self) -> None:
        content = RUN_ALL_SRC.read_text()

        self.assertIn("Test ${ordinal}/${total}: ${label}", content)
        self.assertIn("Environment checks", content)
        self.assertIn("Sidecar startup health", content)
        self.assertIn("IPC compliance self-test", content)
        self.assertIn("Sidecar crash loop recovery", content)
        self.assertIn("Full dictation flow", content)
        self.assertIn("Device removal mid-recording", content)
        self.assertIn("Offline install behavior", content)
        self.assertIn("python3 -m openvoicy_sidecar.self_test", content)
        self.assertIn("E2E TEST SUMMARY", content)
        self.assertIn("PASSED, ${TESTS_SKIPPED} SKIPPED, ${TESTS_FAILED} FAILED", content)

    def test_run_all_logs_with_timestamp_and_run_log_file(self) -> None:
        content = RUN_ALL_SRC.read_text()

        self.assertIn('RUN_LOG_FILE="$PROJECT_ROOT/logs/e2e/run-', content)
        self.assertIn('date -u +%Y-%m-%d-%H%M%S', content)
        self.assertIn('[RUN-ALL] Log file: $RUN_LOG_FILE', content)
        self.assertIn('[$(timestamp_human)]', content)
        self.assertIn('log_line "[RUN-ALL][', content)


if __name__ == "__main__":
    unittest.main()
