"""Regression checks for shared/DEPENDENCIES_TECH_STACK.md drift."""

import re
import unittest
from pathlib import Path
import tomllib


REPO_ROOT = Path(__file__).resolve().parents[2]
REFERENCE = REPO_ROOT / "shared" / "DEPENDENCIES_TECH_STACK.md"
CARGO_MANIFEST = REPO_ROOT / "src-tauri" / "Cargo.toml"
SIDECAR_PYPROJECT = REPO_ROOT / "sidecar" / "pyproject.toml"


class DependenciesTechStackReferenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.text = REFERENCE.read_text(encoding="utf-8")
        cls.cargo_text = CARGO_MANIFEST.read_text(encoding="utf-8")
        cls.pyproject_text = SIDECAR_PYPROJECT.read_text(encoding="utf-8")
        cls.cargo_manifest = tomllib.loads(cls.cargo_text)
        cls.pyproject_manifest = tomllib.loads(cls.pyproject_text)
        cls.rust_dependencies = set(cls.cargo_manifest.get("dependencies", {}).keys())
        cls.sidecar_runtime_dependencies = {
            cls._dependency_name(dep)
            for dep in cls.pyproject_manifest.get("project", {}).get("dependencies", [])
            if isinstance(dep, str)
        }

    @staticmethod
    def _dependency_name(spec: str) -> str:
        # Examples: "numpy>=1.24.0", "pkg[extra]==1.2.3"
        return re.split(r"[<>=!~\\[]", spec, maxsplit=1)[0].strip()

    def test_mentions_required_rust_dependency_rodio_as_current(self) -> None:
        self.assertIn("`rodio` (required in current manifest)", self.text)

    def test_sidecar_runtime_dependencies_match_current_pyproject_shape(self) -> None:
        listed_match = re.search(
            r"Sidecar runtime dependencies in `sidecar/pyproject.toml` are currently:\n"
            r"((?:\s*-\s+`[^`]+`\n)+)",
            self.text,
        )
        self.assertIsNotNone(
            listed_match,
            "Reference must enumerate current sidecar runtime dependencies from pyproject",
        )
        listed_block = listed_match.group(1) if listed_match else ""
        documented_dependencies = set(re.findall(r"-\s+`([^`]+)`", listed_block))

        self.assertEqual(
            documented_dependencies,
            self.sidecar_runtime_dependencies,
            "Documented current sidecar runtime dependencies must match sidecar/pyproject.toml",
        )

        self.assertIn("not currently in manifest", self.text)
        self.assertIn("`faster-whisper`", self.text)
        self.assertIn("`ctranslate2`", self.text)

    def test_drift_checklist_references_primary_manifests(self) -> None:
        self.assertIn("`src-tauri/Cargo.toml`", self.text)
        self.assertIn("`sidecar/pyproject.toml`", self.text)
        self.assertIn("`package.json`", self.text)
        self.assertIn("`bun.lock`", self.text)
        self.assertIn("`package-lock.json`", self.text)

    def test_global_hotkey_crate_name_matches_rust_manifest(self) -> None:
        hotkey_dependencies = {
            name for name in self.rust_dependencies if "hotkey" in name.lower()
        }
        self.assertEqual(hotkey_dependencies, {"global-hotkey"})
        self.assertIn("`global-hotkey`", self.text)
        self.assertNotIn("`global_hotkey`", self.text)


if __name__ == "__main__":
    unittest.main()
