import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
CANCEL_FLOW_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-cancel-flow.sh"


class CancelFlowScriptTests(unittest.TestCase):
    def test_cancel_flow_covers_required_rpc_and_step_sequence(self) -> None:
        content = CANCEL_FLOW_SCRIPT.read_text()

        self.assertIn("STEPS_TOTAL=8", content)
        self.assertIn("Step 1/8", content)
        self.assertIn("Step 8/8", content)
        self.assertIn('sidecar_rpc_session "system.ping"', content)
        self.assertIn('sidecar_rpc_session "status.get"', content)
        self.assertIn('sidecar_rpc_session "recording.start"', content)
        self.assertIn('sidecar_rpc_session "recording.cancel"', content)
        self.assertIn("drain_notifications 3", content)
        self.assertIn('sidecar_rpc_session "system.shutdown"', content)

    def test_cancel_flow_enforces_no_transcription_complete_after_cancel(self) -> None:
        content = CANCEL_FLOW_SCRIPT.read_text()

        self.assertIn("UNEXPECTED_EVENTS=()", content)
        self.assertIn("transcription_complete", content)
        self.assertIn("UNEXPECTED transcription_complete after cancel!", content)
        self.assertIn("unexpected transcription_complete received", content)
        self.assertIn("[ ${#UNEXPECTED_EVENTS[@]} -eq 0 ]", content)

    def test_cancel_flow_summary_includes_expected_shape(self) -> None:
        content = CANCEL_FLOW_SCRIPT.read_text()

        self.assertIn("total_ms:$total_ms", content)
        self.assertIn("steps_passed:$steps_passed", content)
        self.assertIn("steps_total:$steps_total", content)
        self.assertIn("unexpected_events:$unexpected_events", content)
        self.assertIn('log_info "cancel_e2e" "summary" "Test summary"', content)

    def test_cancel_flow_loading_edge_case_avoids_non_hermetic_downloads(self) -> None:
        content = CANCEL_FLOW_SCRIPT.read_text()

        self.assertIn("host_model_cache_dir()", content)
        self.assertIn("model_cache_contains_model()", content)
        self.assertIn('sidecar_rpc_session "asr.initialize" "$initialize_params" 10', content)
        self.assertIn(
            "model not cached; avoiding download side effects",
            content,
        )
        self.assertNotIn('CANCEL_E2E_EDGE_MODEL_ID:-nvidia/parakeet-tdt-0.6b-v3', content)

    def test_cancel_flow_edge_loading_uses_cached_model_guard(self) -> None:
        content = CANCEL_FLOW_SCRIPT.read_text()

        self.assertIn("EDGE_LOADING_MODEL_ID", content)
        self.assertIn("model_cache_contains_model", content)
        self.assertIn("host_model_cache_dir", content)
        self.assertIn("avoiding download side effects", content)
        self.assertIn("CANCEL_E2E_EDGE_MODEL_ID", content)


if __name__ == "__main__":
    unittest.main()
