import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
PACKAGED_RESOURCES_SCRIPT = (
    REPO_ROOT / "scripts" / "e2e" / "test-packaged-resources.sh"
)


class PackagedResourcesScriptTests(unittest.TestCase):
    def test_packaged_resources_script_queries_system_info_with_staged_shared_root(
        self,
    ) -> None:
        content = PACKAGED_RESOURCES_SCRIPT.read_text()

        self.assertIn("detect_target_triple()", content)
        self.assertIn("openvoicy-sidecar-$TARGET", content)
        self.assertIn("run_with_timeout()", content)
        self.assertIn("--target TARGET_TRIPLE", content)
        self.assertIn("OPENVOICY_SHARED_ROOT", content)
        self.assertIn('"system.info"', content)
        self.assertIn('run_with_timeout 10 "$SIDECAR_BIN"', content)
        self.assertNotIn("python3 -m openvoicy_sidecar.self_test", content)
        self.assertNotIn(":$PYTHONPATH", content)
        self.assertNotIn("export PYTHONPATH=", content)
        self.assertIn("shared/contracts", content)
        self.assertIn("shared/model", content)
        self.assertIn("shared/replacements", content)
        self.assertIn("system.info resource path validation: OK", content)


if __name__ == "__main__":
    unittest.main()
