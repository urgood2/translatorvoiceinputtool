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

    def test_full_flow_supports_playback_unavailable_skip(self) -> None:
        content = FULL_FLOW_SCRIPT.read_text()

        self.assertIn("Synthetic audio playback unavailable on this host", content)
        self.assertIn("set +e", content)
        self.assertIn("set -e", content)
        self.assertIn('synth_output=$(generate_and_play_synthetic_audio "$SYNTH_AUDIO_FILE" 2>&1 >/dev/null)', content)
        self.assertIn('if [[ "$synth_status" -ne 0 ]]; then', content)
        self.assertIn('if [[ "$synth_status" -eq 2 ]] || [[ "$synth_status" -eq 3 ]]; then', content)
        self.assertIn('sidecar_rpc_session "recording.cancel"', content)
        self.assertIn("Skipped: recording.stop + wait transcription_complete (playback unavailable)", content)
        self.assertIn("[RESULT] SKIPPED (exit 77)", content)
        self.assertNotIn('if ! generate_and_play_synthetic_audio "$SYNTH_AUDIO_FILE" >/dev/null; then', content)

    def test_full_flow_supports_recording_start_unavailable_skip(self) -> None:
        content = FULL_FLOW_SCRIPT.read_text()

        self.assertIn('if echo "$start_response" | jq -e \'.error\'', content)
        self.assertIn('[[ "$start_kind" == "E_AUDIO_IO" ]]', content)
        self.assertIn('[[ "$start_kind" == "E_DEVICE_NOT_FOUND" ]]', content)
        self.assertIn('[[ "$start_kind" == "E_DEVICE_UNAVAILABLE" ]]', content)
        self.assertIn("recording.start unavailable on host audio stack", content)
        self.assertIn("Skipped: synthetic audio playback (recording unavailable)", content)
        self.assertIn("recording.start returned non-skip error kind", content)


if __name__ == "__main__":
    unittest.main()
