import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
FULL_FLOW_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-full-flow.sh"


class FullFlowScriptTests(unittest.TestCase):
    def test_full_flow_exercises_record_transcribe_inject_calls(self) -> None:
        content = FULL_FLOW_SCRIPT.read_text()

        self.assertIn("start_sidecar", content)
        self.assertIn("recording.start", content)
        self.assertIn("recording.stop", content)
        self.assertIn("asr.transcribe", content)
        self.assertIn("replacements.preview", content)


if __name__ == "__main__":
    unittest.main()
