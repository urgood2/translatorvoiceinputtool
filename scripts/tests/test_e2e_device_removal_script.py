import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-device-removal.sh"


class DeviceRemovalScriptTests(unittest.TestCase):
    def test_contains_required_flow_steps(self) -> None:
        content = SCRIPT.read_text()

        self.assertIn("STEPS_TOTAL=8", content)
        self.assertIn("step_log 1", content)
        self.assertIn("step_log 8", content)
        self.assertIn("system.ping", content)
        self.assertIn("audio.list_devices", content)
        self.assertIn("recording.start", content)
        self.assertIn("recording.stop", content)
        self.assertIn("recording.status", content)
        self.assertIn("system.shutdown", content)

    def test_uses_simulation_policy_tests_and_skip_contract(self) -> None:
        content = SCRIPT.read_text()

        self.assertIn("E2E_DEVICE_REMOVAL_MODE", content)
        self.assertIn("integration::tests::test_device_hot_swap_decision_during_recording_requests_stop_and_fallback", content)
        self.assertIn("integration::tests::test_device_hot_swap_decision_mid_transcription_forces_clipboard_preservation", content)
        self.assertIn("integration::tests::test_device_removed_app_error_includes_required_recovery_details", content)
        self.assertIn("return 77", content)
        self.assertIn("E_DEVICE_REMOVED", content)
        self.assertIn("E_AUDIO_IO", content)

    def test_logs_and_failure_dump_contract(self) -> None:
        content = SCRIPT.read_text()

        self.assertIn("logs/e2e/test-device-removal-", content)
        self.assertIn("Last 5 RPC exchanges", content)
        self.assertIn("Device state history", content)
        self.assertIn("[STEP ${step}/${STEPS_TOTAL}]", content)


if __name__ == "__main__":
    unittest.main()
