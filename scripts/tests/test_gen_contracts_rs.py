import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "gen_contracts_rs.py"
SPEC = importlib.util.spec_from_file_location("gen_contracts_rs", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class GenContractsRsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.repo_root = Path(__file__).resolve().parents[2]

    def test_main_generates_deterministic_output(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            out_path = Path(tmpdir) / "contracts.rs"
            args = ["--repo-root", str(self.repo_root), "--out", str(out_path)]

            first_exit = MODULE.main(args)
            first = out_path.read_text(encoding="utf-8")

            second_exit = MODULE.main(args)
            second = out_path.read_text(encoding="utf-8")

            self.assertEqual(first_exit, 0)
            self.assertEqual(second_exit, 0)
            self.assertEqual(first, second)

    def test_output_contains_required_constants_and_types(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            out_path = Path(tmpdir) / "contracts.rs"
            MODULE.main(["--repo-root", str(self.repo_root), "--out", str(out_path)])
            output = out_path.read_text(encoding="utf-8")

            required_snippets = [
                "pub const CMD_GET_APP_STATE: &str = \"get_app_state\";",
                "pub const EVENT_STATE_CHANGED: &str = \"state:changed\";",
                "pub const EVENT_STATE_CHANGED_LEGACY: &str = \"state_changed\";",
                "pub const RPC_STATUS_GET: &str = \"status.get\";",
                "pub const RPC_NOTIFY_EVENT_STATUS_CHANGED: &str = \"event.status_changed\";",
                "pub type EventStateChangedPayload = ",
                "pub struct RpcStatusGetParams",
                "pub struct RpcStatusGetResult",
                "pub struct RpcNotificationEventStatusChangedParams",
            ]

            for snippet in required_snippets:
                self.assertIn(snippet, output)

    def test_output_omits_timestamps_and_absolute_paths(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            out_path = Path(tmpdir) / "contracts.rs"
            MODULE.main(["--repo-root", str(self.repo_root), "--out", str(out_path)])
            output = out_path.read_text(encoding="utf-8")

            self.assertNotIn(str(self.repo_root), output)
            self.assertNotIn("Generated at", output)
            self.assertIn("AUTO-GENERATED from shared/contracts/*.v1.json", output)
            self.assertIn("Regenerate with: python scripts/gen_contracts_rs.py", output)


if __name__ == "__main__":
    unittest.main()
