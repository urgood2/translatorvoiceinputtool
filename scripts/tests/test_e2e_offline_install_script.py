import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
OFFLINE_INSTALL_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-offline-install.sh"


class OfflineInstallScriptTests(unittest.TestCase):
    def test_offline_install_flow_covers_required_six_steps(self) -> None:
        content = OFFLINE_INSTALL_SCRIPT.read_text()

        self.assertIn("STEPS_TOTAL=6", content)
        self.assertIn("step_log 1", content)
        self.assertIn("step_log 6", content)
        self.assertIn("set_offline_network", content)
        self.assertIn("set_online_network", content)
        self.assertIn("asr.initialize", content)
        self.assertIn("model.download", content)
        self.assertIn("status.get", content)
        self.assertIn("system.ping", content)
        self.assertIn("[network=${NETWORK_STATE}]", content)

    def test_offline_install_enforces_actionable_network_errors_and_atomic_cache(self) -> None:
        content = OFFLINE_INSTALL_SCRIPT.read_text()

        self.assertIn("assert_network_error_actionable", content)
        self.assertIn("E_NETWORK", content)
        self.assertIn("missing retry/check guidance", content)
        self.assertIn('local partial_dir="$isolated_cache_dir/.partial/$OFFLINE_MODEL_ID"', content)
        self.assertIn("partial staging directory still exists", content)
        self.assertIn("corrupt final model directory detected without manifest", content)

    def test_offline_install_logs_to_expected_file_and_supports_skip_contract(self) -> None:
        content = OFFLINE_INSTALL_SCRIPT.read_text()

        self.assertIn('source "$SCRIPT_DIR/lib/log.sh"', content)
        self.assertIn("logs/e2e/test-offline-install-", content)
        self.assertIn("return 77", content)
        self.assertIn("[RPC][REQ]", content)
        self.assertIn("[RPC][RES]", content)
        self.assertIn("dump_failure_context", content)

    def test_offline_install_uses_single_persistent_sidecar_session(self) -> None:
        content = OFFLINE_INSTALL_SCRIPT.read_text()

        self.assertIn("start_sidecar || return 1", content)
        self.assertIn('exec 4<"$E2E_SIDECAR_STDOUT"', content)
        self.assertIn("printf '%s\\n' \"$request\" >&3", content)
        self.assertIn("read -r -u 4 -t", content)
        self.assertNotIn("e2e_timeout_run \"$timeout\" \"$E2E_SIDECAR_BIN\"", content)

    def test_offline_install_uses_single_persistent_sidecar_session(self) -> None:
        content = OFFLINE_INSTALL_SCRIPT.read_text()

        self.assertIn("start_sidecar || return 1", content)
        self.assertIn('exec 4<"$E2E_SIDECAR_STDOUT"', content)
        self.assertIn("printf '%s\\n' \"$request\" >&3", content)
        self.assertIn("read -r -u 4 -t", content)
        self.assertIn("set_offline_network", content)
        self.assertIn("set_online_network || return 1", content)


if __name__ == "__main__":
    unittest.main()
