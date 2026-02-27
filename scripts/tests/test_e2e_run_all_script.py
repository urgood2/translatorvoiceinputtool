import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
RUN_ALL_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "run-all.sh"


class E2ERunAllScriptTests(unittest.TestCase):
    def test_run_all_declares_required_seven_stage_orchestrator(self) -> None:
        content = RUN_ALL_SCRIPT.read_text()

        self.assertIn("TOTAL_STAGES=7", content)
        self.assertIn("run_stage 1 \"Environment checks\"", content)
        self.assertIn("run_stage 2 \"Sidecar startup health\"", content)
        self.assertIn("run_stage 3 \"IPC compliance\"", content)
        self.assertIn("run_stage 4 \"Crash loop recovery\"", content)
        self.assertIn("run_stage 5 \"Full dictation flow\"", content)
        self.assertIn("run_stage 6 \"Device removal\"", content)
        self.assertIn("run_stage 7 \"Offline install\"", content)

    def test_run_all_invokes_expected_scripts_and_ipc_self_test(self) -> None:
        content = RUN_ALL_SCRIPT.read_text()

        self.assertIn("test-startup-health.sh", content)
        self.assertIn("test-error-recovery.sh", content)
        self.assertIn("test-full-flow.sh", content)
        self.assertIn("test-device-removal.sh", content)
        self.assertIn("test-offline-install.sh", content)
        self.assertIn("openvoicy_sidecar.self_test", content)
        self.assertIn("sidecar/self-test", content)

    def test_run_all_logs_summary_and_exit_contract(self) -> None:
        content = RUN_ALL_SCRIPT.read_text()

        self.assertIn("logs/e2e/run-", content)
        self.assertIn("E2E TEST SUMMARY", content)
        self.assertIn("if [[ \"$TESTS_FAILED\" -gt 0 ]]; then", content)
        self.assertIn("exit 1", content)
        self.assertIn("exit 0", content)


if __name__ == "__main__":
    unittest.main()
