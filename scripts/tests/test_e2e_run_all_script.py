import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
RUN_ALL_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "run-all.sh"


class E2ERunAllScriptTests(unittest.TestCase):
    def test_run_all_declares_required_seven_stage_orchestrator(self) -> None:
        content = RUN_ALL_SCRIPT.read_text()

        self.assertIn('if matches_filter "environment" "Environment checks"; then', content)
        self.assertIn('if matches_filter "startup-health" "Sidecar startup health"; then', content)
        self.assertIn('if matches_filter "ipc-compliance" "IPC compliance self-test"; then', content)
        self.assertIn('if matches_filter "crash-loop-recovery" "Sidecar crash loop recovery"; then', content)
        self.assertIn('if matches_filter "full-dictation-flow" "Full dictation flow"; then', content)
        self.assertIn('if matches_filter "device-removal" "Device removal mid-recording"; then', content)
        self.assertIn('if matches_filter "offline-install" "Offline install behavior"; then', content)
        self.assertIn('run_step "$ordinal" "$total" "${ids[$idx]}" "${labels[$idx]}" ${handlers[$idx]}', content)

    def test_run_all_invokes_expected_scripts_and_ipc_self_test(self) -> None:
        content = RUN_ALL_SCRIPT.read_text()

        self.assertIn("run_logged_command startup-health bash $SCRIPT_DIR/test-startup-health.sh", content)
        self.assertIn("run_logged_command crash-loop-recovery bash $SCRIPT_DIR/test-error-recovery.sh", content)
        self.assertIn("run_logged_command full-dictation-flow bash $SCRIPT_DIR/test-full-flow.sh", content)
        self.assertIn("run_logged_command device-removal bash $SCRIPT_DIR/test-device-removal.sh", content)
        self.assertIn("run_logged_command offline-install bash $SCRIPT_DIR/test-offline-install.sh", content)
        self.assertIn("run_logged_command ipc-compliance python3 -m openvoicy_sidecar.self_test", content)

    def test_run_all_logs_summary_and_exit_contract(self) -> None:
        content = RUN_ALL_SCRIPT.read_text()

        self.assertIn("logs/e2e/run-", content)
        self.assertIn("E2E TEST SUMMARY", content)
        self.assertIn("if [[ \"$TESTS_FAILED\" -gt 0 ]]; then", content)
        self.assertIn("exit 1", content)
        self.assertIn("exit 0", content)


if __name__ == "__main__":
    unittest.main()
