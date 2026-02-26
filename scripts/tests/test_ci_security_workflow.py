"""Regression tests for .github/workflows/security.yml."""

import unittest
from pathlib import Path

import yaml

REPO_ROOT = Path(__file__).resolve().parents[2]
SECURITY_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "security.yml"


def _load_workflow() -> dict:
    return yaml.safe_load(SECURITY_WORKFLOW.read_text())


class SecurityWorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.text = SECURITY_WORKFLOW.read_text()
        cls.workflow = _load_workflow()

    def test_workflow_is_valid_yaml(self) -> None:
        self.assertIsInstance(self.workflow, dict)

    def test_dependency_and_sast_job_present(self) -> None:
        self.assertIn("dependency-and-sast", self.workflow["jobs"])
        self.assertEqual(
            self.workflow["jobs"]["dependency-and-sast"]["runs-on"],
            "ubuntu-latest",
        )

    def test_rust_dependency_audit_is_configured(self) -> None:
        self.assertIn("cargo audit", self.text)
        self.assertIn("cargo-audit", self.text)

    def test_javascript_dependency_audit_is_configured(self) -> None:
        self.assertIn("npm audit --audit-level=high", self.text)
        self.assertIn("npm ci --ignore-scripts", self.text)

    def test_python_dependency_and_sast_scans_are_configured(self) -> None:
        self.assertIn("pip-audit", self.text)
        self.assertIn("bandit -r src/openvoicy_sidecar -x tests", self.text)


if __name__ == "__main__":
    unittest.main()
