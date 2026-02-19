import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
STARTUP_HEALTH_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-startup-health.sh"


class StartupHealthScriptTests(unittest.TestCase):
    def test_startup_health_script_covers_required_sequence(self) -> None:
        content = STARTUP_HEALTH_SCRIPT.read_text()

        self.assertIn("start_sidecar", content)
        self.assertIn("system.ping", content)
        self.assertIn("system.info", content)
        self.assertIn("status.get", content)
        self.assertIn("system.shutdown", content)

    def test_startup_health_script_emits_step_logs_and_summary(self) -> None:
        content = STARTUP_HEALTH_SCRIPT.read_text()

        self.assertIn("[STARTUP_E2E] Step", content)
        self.assertIn("test-startup-health-", content)
        self.assertIn("steps_passed", content)
        self.assertIn("steps_total", content)
        self.assertIn("total_ms", content)


if __name__ == "__main__":
    unittest.main()
