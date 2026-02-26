import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
FULL_FLOW_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-full-flow.sh"


class FullFlowScriptTests(unittest.TestCase):
    def test_full_flow_implements_required_11_step_dictation_sequence(self) -> None:
        content = FULL_FLOW_SCRIPT.read_text()

        self.assertIn("STEPS_TOTAL=11", content)
        self.assertIn("step_log 1", content)
        self.assertIn("step_log 11", content)
        self.assertIn("system.ping", content)
        self.assertIn("asr.initialize", content)
        self.assertIn("start_sidecar", content)
        self.assertIn("recording.start", content)
        self.assertIn("recording.stop", content)
        self.assertIn("event.transcription_complete", content)
        self.assertIn("system.shutdown", content)

    def test_full_flow_supports_model_unavailable_skip_and_failure_context(self) -> None:
        content = FULL_FLOW_SCRIPT.read_text()

        self.assertIn("return 77", content)
        self.assertIn("Last 5 JSON-RPC exchanges", content)
        self.assertIn("status.get", content)
        self.assertIn("logs/e2e/test-full-flow-", content)


if __name__ == "__main__":
    unittest.main()
