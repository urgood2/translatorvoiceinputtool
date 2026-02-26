"""Regression tests for .github/workflows/build.yml.

Guards against: wrong action names, broken target args, missing OS matrix entries.
"""

import unittest
from pathlib import Path

import yaml

REPO_ROOT = Path(__file__).resolve().parents[2]
BUILD_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "build.yml"


def _load_workflow() -> dict:
    return yaml.safe_load(BUILD_WORKFLOW.read_text())


class TestBuildWorkflowStructure(unittest.TestCase):
    """Guard rails for build.yml structural correctness."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.text = BUILD_WORKFLOW.read_text()
        try:
            cls.wf = _load_workflow()
        except Exception:
            cls.wf = None

    def test_workflow_parses_as_yaml(self) -> None:
        self.assertIsNotNone(self.wf, "build.yml must be valid YAML")

    def test_rust_toolchain_action_name(self) -> None:
        self.assertNotIn("dtolnay/rust-action", self.text)
        self.assertIn("dtolnay/rust-toolchain", self.text)

    def test_sidecar_builds_for_all_platforms(self) -> None:
        matrix = self.wf["jobs"]["build-sidecar"]["strategy"]["matrix"]["include"]
        targets = {entry["target"] for entry in matrix}
        self.assertIn("linux-x64", targets)
        self.assertIn("windows-x64", targets)
        self.assertIn("macos-x64", targets)
        self.assertIn("macos-arm64", targets)

    def test_app_builds_for_all_platforms(self) -> None:
        matrix = self.wf["jobs"]["build-app"]["strategy"]["matrix"]["include"]
        targets = {entry["target"] for entry in matrix}
        self.assertIn("linux", targets)
        self.assertIn("windows", targets)
        self.assertIn("macos-x64", targets)
        self.assertIn("macos-arm64", targets)

    def test_no_empty_target_flag(self) -> None:
        """Passing --target with empty value breaks cargo; must be conditional."""
        self.assertNotIn("--target ${{", self.text,
                         "Use conditional expression that includes --target inside the expression")

    def test_fail_fast_disabled(self) -> None:
        for job_name in ("build-sidecar", "build-app"):
            job = self.wf["jobs"][job_name]
            self.assertFalse(
                job["strategy"].get("fail-fast", True),
                f"{job_name} must set fail-fast: false",
            )


if __name__ == "__main__":
    unittest.main()
