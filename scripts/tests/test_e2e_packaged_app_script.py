import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-packaged-app.sh"


class PackagedAppScriptTests(unittest.TestCase):
    def test_packaged_app_script_checks_resources_and_system_info_paths(self) -> None:
        content = SCRIPT.read_text()

        self.assertIn("src-tauri/target/release/bundle", content)
        self.assertIn("find_bundle_artifact()", content)
        self.assertIn("bundle_artifact=", content)
        self.assertIn("Missing packaged app bundle output directory", content)
        self.assertIn("detect_timeout_runner()", content)
        self.assertIn("run_with_timeout()", content)
        self.assertIn('run_with_timeout 10 "$SIDECAR_BIN"', content)
        self.assertNotIn('| timeout 10 "$SIDECAR_BIN"', content)
        self.assertIn("OPENVOICY_SHARED_ROOT", content)
        self.assertIn("OPENVOICY_SIDECAR_COMMAND", content)
        self.assertIn("python3 -m openvoicy_sidecar.self_test", content)
        self.assertIn('"system.info"', content)
        self.assertIn('"resource_paths"', content)
        self.assertIn("MODEL_MANIFEST.json", content)
        self.assertIn("PRESETS.json", content)
        self.assertIn("system.info resource path validation: OK", content)


if __name__ == "__main__":
    unittest.main()
