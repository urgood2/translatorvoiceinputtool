import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
PACKAGED_RESOURCES_SCRIPT = (
    REPO_ROOT / "scripts" / "e2e" / "test-packaged-resources.sh"
)


class PackagedResourcesScriptTests(unittest.TestCase):
    def test_packaged_resources_script_runs_self_test_with_staged_shared_root(
        self,
    ) -> None:
        content = PACKAGED_RESOURCES_SCRIPT.read_text()

        self.assertIn("detect_target_triple()", content)
        self.assertIn("openvoicy-sidecar-$TARGET", content)
        self.assertIn("OPENVOICY_SIDECAR_COMMAND", content)
        self.assertIn("--target TARGET_TRIPLE", content)
        self.assertIn("OPENVOICY_SHARED_ROOT", content)
        self.assertIn("python3 -m openvoicy_sidecar.self_test", content)
        self.assertIn("shared/contracts", content)
        self.assertIn("shared/model", content)
        self.assertIn("shared/replacements", content)


if __name__ == "__main__":
    unittest.main()
