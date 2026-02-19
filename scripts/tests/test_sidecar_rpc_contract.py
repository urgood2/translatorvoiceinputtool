import json
import unittest
from pathlib import Path


CONTRACT_PATH = Path(__file__).resolve().parents[2] / "shared" / "contracts" / "sidecar.rpc.v1.json"


class SidecarRpcContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.contract = json.loads(CONTRACT_PATH.read_text(encoding="utf-8"))
        cls.items = cls.contract["items"]

    def test_baseline_shape(self) -> None:
        self.assertEqual(self.contract["version"], 1)
        self.assertIsInstance(self.items, list)
        self.assertGreater(len(self.items), 0)

    def test_all_methods_define_required_and_schemas(self) -> None:
        methods = [item for item in self.items if item.get("type") == "method"]
        self.assertGreater(len(methods), 0)

        for method in methods:
            self.assertIsInstance(method.get("required"), bool, method.get("name"))
            self.assertIn("params_schema", method, method.get("name"))
            self.assertIn("result_schema", method, method.get("name"))
            self.assertEqual(method["params_schema"].get("type"), "object", method.get("name"))
            self.assertEqual(method["result_schema"].get("type"), "object", method.get("name"))

    def test_model_install_method_exists_as_optional_compat_alias(self) -> None:
        model_install = next(
            item for item in self.items if item.get("type") == "method" and item.get("name") == "model.install"
        )
        self.assertFalse(model_install["required"])
        self.assertEqual(model_install["params_schema"]["type"], "object")
        self.assertIn("status", model_install["result_schema"]["required"])

    def test_model_progress_notification_exists_as_optional(self) -> None:
        model_progress = next(
            item for item in self.items if item.get("type") == "notification" and item.get("name") == "event.model_progress"
        )
        self.assertFalse(model_progress["required"])
        self.assertEqual(model_progress["params_schema"]["type"], "object")
        self.assertIn("current", model_progress["params_schema"]["required"])


if __name__ == "__main__":
    unittest.main()
