import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
ERROR_RECOVERY_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-error-recovery.sh"


class ErrorRecoveryScriptTests(unittest.TestCase):
    def test_script_checks_scenarios_failed_before_exit(self) -> None:
        """Regression: 23d7 — exit code must account for SCENARIOS_FAILED."""
        content = ERROR_RECOVERY_SCRIPT.read_text()
        self.assertIn("SCENARIOS_FAILED", content)
        # Verify that SCENARIOS_FAILED is checked after assertion_summary
        assertion_summary_pos = content.index("assertion_summary")
        scenarios_failed_check_pos = content.index("SCENARIOS_FAILED > 0")
        self.assertGreater(
            scenarios_failed_check_pos,
            assertion_summary_pos,
            "SCENARIOS_FAILED check must appear after assertion_summary",
        )

    def test_script_covers_all_four_scenarios(self) -> None:
        content = ERROR_RECOVERY_SCRIPT.read_text()
        self.assertIn("scenario_single_crash_recovery", content)
        self.assertIn("scenario_crash_loop_circuit_breaker", content)
        self.assertIn("scenario_manual_restart_after_breaker", content)
        self.assertIn("scenario_ipc_timeout", content)


if __name__ == "__main__":
    unittest.main()
