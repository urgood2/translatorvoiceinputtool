import re
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
ERROR_RECOVERY_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-error-recovery.sh"


class ErrorRecoveryScriptTests(unittest.TestCase):
    def test_single_crash_recovery_uses_policy_tests(self) -> None:
        """Regression (33u9): scenario 1 must assert supervisor auto-restart via policy tests."""
        content = ERROR_RECOVERY_SCRIPT.read_text()
        self.assertIn("run_policy_test", content)
        self.assertIn(
            "handle_crash_stops_lingering_process_before_starting_new_one",
            content,
            "Scenario 1 must test supervisor crash handling via policy test",
        )
        # Scenario 1 body must NOT manually start sidecar — it should use policy tests
        scenario_fn = re.search(
            r"scenario_single_crash_recovery\(\)\s*\{(.*?)\n\}",
            content,
            re.DOTALL,
        )
        self.assertIsNotNone(scenario_fn, "scenario_single_crash_recovery function must exist")
        body = scenario_fn.group(1)
        self.assertNotIn(
            "start_sidecar_session",
            body,
            "Scenario 1 must use policy tests, not manual sidecar restart",
        )


if __name__ == "__main__":
    unittest.main()
