import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
TEST_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "test.yml"
INTEGRATION_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "integration.yml"
WORKSPACE_TARGET = "${{ github.workspace }}/src-tauri/target"


class CargoTargetDirWorkflowTests(unittest.TestCase):
    def test_rust_workflows_do_not_use_tmp_cargo_target(self) -> None:
        test_workflow = TEST_WORKFLOW.read_text()
        integration_workflow = INTEGRATION_WORKFLOW.read_text()

        # Regression guard: /tmp can be capacity constrained on CI runners.
        self.assertNotIn("/tmp/cargo-target", test_workflow)
        self.assertNotIn("/tmp/cargo-target", integration_workflow)

    def test_rust_workflows_use_workspace_target_dir(self) -> None:
        test_workflow = TEST_WORKFLOW.read_text()
        integration_workflow = INTEGRATION_WORKFLOW.read_text()

        self.assertIn(f"CARGO_TARGET_DIR: {WORKSPACE_TARGET}", test_workflow)
        self.assertIn(f"CARGO_TARGET_DIR: {WORKSPACE_TARGET}", integration_workflow)

    def test_test_workflow_runs_contract_validation_step(self) -> None:
        test_workflow = TEST_WORKFLOW.read_text()

        self.assertIn("Validate contracts and generated artifacts", test_workflow)
        self.assertIn("python scripts/validate_contracts.py", test_workflow)
        self.assertIn("Run comprehensive contract validation suite", test_workflow)
        self.assertIn("python scripts/test_contract_validation.py", test_workflow)

    def test_python_workflow_runs_sidecar_self_test_after_pytest(self) -> None:
        test_workflow = TEST_WORKFLOW.read_text()

        self.assertIn("Sidecar Self-Test", test_workflow)
        self.assertIn("python -m openvoicy_sidecar.self_test", test_workflow)

        pytest_index = test_workflow.index("Run pytest")
        self_test_index = test_workflow.index("Sidecar Self-Test")
        self.assertGreater(self_test_index, pytest_index)


if __name__ == "__main__":
    unittest.main()
