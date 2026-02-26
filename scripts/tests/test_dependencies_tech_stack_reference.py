"""Regression checks for shared/DEPENDENCIES_TECH_STACK.md drift."""

import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
REFERENCE = REPO_ROOT / "shared" / "DEPENDENCIES_TECH_STACK.md"


class DependenciesTechStackReferenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.text = REFERENCE.read_text(encoding="utf-8")

    def test_mentions_required_rust_dependency_rodio_as_current(self) -> None:
        self.assertIn("`rodio` (required in current manifest)", self.text)

    def test_sidecar_runtime_dependencies_match_current_pyproject_shape(self) -> None:
        self.assertIn("`sounddevice`", self.text)
        self.assertIn("`numpy`", self.text)
        self.assertIn("`scipy`", self.text)
        self.assertIn("not currently in manifest", self.text)
        self.assertIn("`faster-whisper`", self.text)
        self.assertIn("`ctranslate2`", self.text)

    def test_drift_checklist_references_primary_manifests(self) -> None:
        self.assertIn("`src-tauri/Cargo.toml`", self.text)
        self.assertIn("`sidecar/pyproject.toml`", self.text)
        self.assertIn("`package.json`", self.text)
        self.assertIn("`bun.lock`", self.text)
        self.assertIn("`package-lock.json`", self.text)


if __name__ == "__main__":
    unittest.main()
