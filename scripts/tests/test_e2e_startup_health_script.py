import re
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
STARTUP_HEALTH_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-startup-health.sh"


def _extract_jq_validation_blocks(content: str) -> dict[str, str]:
    """Extract jq validation expressions keyed by the RPC method they follow."""
    blocks: dict[str, str] = {}
    # Pattern: sidecar_rpc_session "method.name" ... followed by jq -e '...' validation
    method_re = re.compile(r'sidecar_rpc_session\s+"([^"]+)"')
    jq_re = re.compile(r"jq\s+-e\s+'(.*?)'", re.DOTALL)

    for method_match in method_re.finditer(content):
        method = method_match.group(1)
        # Look for the next jq -e validation after this method call
        remaining = content[method_match.end():]
        jq_match = jq_re.search(remaining)
        if jq_match:
            blocks[method] = jq_match.group(1)
    return blocks


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

    def test_status_get_validation_accepts_loading_model_state(self) -> None:
        """Regression: 18ci — loading_model is a valid startup state."""
        content = STARTUP_HEALTH_SCRIPT.read_text()
        self.assertIn("loading_model", content)

    def test_system_ping_validates_response_shape(self) -> None:
        """Regression (l3vw): system.ping jq validation must check pong or protocol."""
        content = STARTUP_HEALTH_SCRIPT.read_text()
        blocks = _extract_jq_validation_blocks(content)
        self.assertIn("system.ping", blocks)
        ping_jq = blocks["system.ping"]
        self.assertIn(".result.pong", ping_jq, "system.ping must validate pong field")
        self.assertIn(".result.protocol", ping_jq, "system.ping must validate protocol field")

    def test_system_info_validates_required_fields(self) -> None:
        """Regression (l3vw): system.info jq validation must check capabilities + runtime fields."""
        content = STARTUP_HEALTH_SCRIPT.read_text()
        blocks = _extract_jq_validation_blocks(content)
        self.assertIn("system.info", blocks)
        info_jq = blocks["system.info"]
        self.assertIn(".result.capabilities", info_jq, "system.info must validate capabilities")
        self.assertIn("python_version", info_jq, "system.info must validate python_version")
        self.assertIn("platform", info_jq, "system.info must validate platform")
        self.assertIn("cuda_available", info_jq, "system.info must validate cuda_available")

    def test_status_get_validates_state_enum(self) -> None:
        """Regression (l3vw): status.get jq validation must check state against valid enum."""
        content = STARTUP_HEALTH_SCRIPT.read_text()
        blocks = _extract_jq_validation_blocks(content)
        self.assertIn("status.get", blocks)
        status_jq = blocks["status.get"]
        for expected_state in ("idle", "recording", "transcribing", "error", "loading_model"):
            self.assertIn(
                expected_state,
                status_jq,
                f"status.get must validate state '{expected_state}' in enum",
            )

    def test_system_shutdown_validates_response(self) -> None:
        """Regression (l3vw): system.shutdown jq validation must check result shape."""
        content = STARTUP_HEALTH_SCRIPT.read_text()
        blocks = _extract_jq_validation_blocks(content)
        self.assertIn("system.shutdown", blocks)
        shutdown_jq = blocks["system.shutdown"]
        self.assertIn("shutting_down", shutdown_jq, "system.shutdown must validate shutting_down status")


if __name__ == "__main__":
    unittest.main()
