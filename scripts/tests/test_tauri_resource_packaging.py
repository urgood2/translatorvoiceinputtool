"""Regression tests for Tauri bundle resource packaging."""

from __future__ import annotations

import json
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
TAURI_CONF = REPO_ROOT / "src-tauri" / "tauri.conf.json"


class TauriResourcePackagingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.config = json.loads(TAURI_CONF.read_text())
        cls.resources = cls.config["bundle"]["resources"]
        cls.resource_set = set(cls.resources)

    def test_required_shared_resource_globs_present(self) -> None:
        required = {
            "../shared/contracts/*",
            "../shared/model/*",
            "../shared/replacements/*",
        }
        self.assertTrue(required.issubset(self.resource_set))

    def test_model_manifests_directory_is_packaged(self) -> None:
        self.assertIn("../shared/model/manifests/*", self.resource_set)

    def test_resource_globs_resolve_to_existing_files(self) -> None:
        for pattern in (
            "../shared/contracts/*",
            "../shared/model/*",
            "../shared/model/manifests/*",
            "../shared/replacements/*",
        ):
            resolved = sorted((REPO_ROOT / "src-tauri").glob(pattern))
            with self.subTest(pattern=pattern):
                self.assertGreater(
                    len(resolved),
                    0,
                    f"Resource glob {pattern} should match at least one file",
                )


if __name__ == "__main__":
    unittest.main()
