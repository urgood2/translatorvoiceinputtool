import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
BUNDLE_SCRIPT = REPO_ROOT / "scripts" / "bundle-sidecar.sh"


class BundleSidecarResourceTests(unittest.TestCase):
    def test_bundle_script_copies_model_catalog_and_manifests(self) -> None:
        content = BUNDLE_SCRIPT.read_text()

        self.assertIn("SIDECAR_SHARED_MODEL", content)
        self.assertIn("TAURI_SHARED_MODEL", content)
        self.assertIn("MODEL_CATALOG.json", content)
        self.assertIn("MODEL_MANIFEST.json", content)
        self.assertIn("SIDECAR_SHARED_MANIFESTS", content)
        self.assertIn("TAURI_SHARED_MANIFESTS", content)
        self.assertIn(
            "cp \"$SIDECAR_SHARED_MODEL/MODEL_CATALOG.json\" \"$TAURI_SHARED_MODEL/MODEL_CATALOG.json\"",
            content,
        )
        self.assertIn(
            "cp \"$SIDECAR_SHARED_MODEL/MODEL_MANIFEST.json\" \"$TAURI_SHARED_MODEL/MODEL_MANIFEST.json\"",
            content,
        )
        self.assertIn(
            "cp \"$SIDECAR_SHARED_MANIFESTS\"/*.json \"$TAURI_SHARED_MANIFESTS/\"",
            content,
        )
        self.assertIn("--smoke-test", content)
        self.assertIn('scripts/e2e/test-packaged-app.sh', content)


if __name__ == "__main__":
    unittest.main()
