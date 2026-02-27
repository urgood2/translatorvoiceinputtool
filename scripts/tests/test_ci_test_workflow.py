"""Regression tests for .github/workflows/test.yml.

Guards against: wrong action names, single-OS test matrices,
platform-specific cache paths.
"""

import unittest
from pathlib import Path

import yaml

REPO_ROOT = Path(__file__).resolve().parents[2]
TEST_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "test.yml"


def _load_workflow() -> dict:
    return yaml.safe_load(TEST_WORKFLOW.read_text())


class TestWorkflowStructure(unittest.TestCase):
    """Guard rails for test.yml structural correctness."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.text = TEST_WORKFLOW.read_text()
        try:
            cls.wf = _load_workflow()
        except Exception:
            cls.wf = None

    def test_workflow_parses_as_yaml(self) -> None:
        self.assertIsNotNone(self.wf, "test.yml must be valid YAML")

    def test_rust_toolchain_action_name(self) -> None:
        """dtolnay/rust-action does not exist; must be rust-toolchain."""
        self.assertNotIn("dtolnay/rust-action", self.text)
        self.assertIn("dtolnay/rust-toolchain", self.text)

    def test_rust_tests_run_on_all_os(self) -> None:
        matrix = self.wf["jobs"]["rust-tests"]["strategy"]["matrix"]["os"]
        self.assertIn("ubuntu-latest", matrix)
        self.assertIn("windows-latest", matrix)
        self.assertIn("macos-latest", matrix)

    def test_python_tests_run_on_all_os(self) -> None:
        matrix = self.wf["jobs"]["python-tests"]["strategy"]["matrix"]["os"]
        self.assertIn("ubuntu-latest", matrix)
        self.assertIn("windows-latest", matrix)
        self.assertIn("macos-latest", matrix)

    def test_typescript_tests_run_on_all_os(self) -> None:
        matrix = self.wf["jobs"]["typescript-tests"]["strategy"]["matrix"]["os"]
        self.assertIn("ubuntu-latest", matrix)
        self.assertIn("windows-latest", matrix)
        self.assertIn("macos-latest", matrix)

    def test_no_hardcoded_linux_pip_cache_path(self) -> None:
        """Manual ~/.cache/pip path breaks macOS/Windows; use setup-python cache."""
        self.assertNotIn("~/.cache/pip", self.text)

    def test_cargo_locked_flag(self) -> None:
        self.assertIn("--locked", self.text)

    def test_fail_fast_disabled(self) -> None:
        """All matrix combinations should run even if one fails."""
        for job_name in ("rust-tests", "python-tests", "typescript-tests"):
            job = self.wf["jobs"][job_name]
            self.assertFalse(
                job["strategy"].get("fail-fast", True),
                f"{job_name} must set fail-fast: false",
            )

    def test_typescript_workflow_uses_bun(self) -> None:
        self.assertIn("oven-sh/setup-bun", self.text)
        self.assertIn("bun install --frozen-lockfile", self.text)
        self.assertIn("bunx tsc --noEmit", self.text)
        self.assertIn("bun run test", self.text)
        self.assertIn("bun run build", self.text)
        self.assertIn("dist/index.html", self.text)
        self.assertIn("dist/overlay.html", self.text)
        self.assertNotIn("npm ci", self.text)
        self.assertNotIn("npm test", self.text)

    def test_typescript_workflow_includes_typecheck_step(self) -> None:
        self.assertIn("TypeScript typecheck", self.text)
        self.assertIn("bunx tsc --noEmit", self.text)

        steps = self.wf["jobs"]["typescript-tests"]["steps"]
        typecheck_step = next(
            step for step in steps if step.get("name") == "TypeScript typecheck"
        )
        self.assertEqual(typecheck_step.get("run"), "bunx tsc --noEmit")

    def test_typescript_workflow_verifies_frontend_build_outputs(self) -> None:
        self.assertIn("Verify frontend build produces overlay assets", self.text)
        self.assertIn("bun run build", self.text)
        self.assertIn("dist/index.html", self.text)
        self.assertIn("dist/overlay.html", self.text)

        steps = self.wf["jobs"]["typescript-tests"]["steps"]
        build_step = next(
            step
            for step in steps
            if step.get("name") == "Verify frontend build produces overlay assets"
        )
        run_script = str(build_step.get("run", ""))
        self.assertIn("bun run build", run_script)
        self.assertIn("test -f dist/index.html", run_script)
        self.assertIn("test -f dist/overlay.html", run_script)

    def test_typescript_step_order_typecheck_then_tests_then_build_verify(self) -> None:
        steps = self.wf["jobs"]["typescript-tests"]["steps"]
        names = [str(step.get("name", "")) for step in steps]

        typecheck_idx = names.index("TypeScript typecheck")
        tests_idx = names.index("Run TypeScript tests")
        build_verify_idx = names.index("Verify frontend build produces overlay assets")

        self.assertLess(typecheck_idx, tests_idx)
        self.assertLess(tests_idx, build_verify_idx)

    def test_schema_validation_runs_ipc_and_model_manifest_validators(self) -> None:
        self.assertIn("python scripts/validate_ipc_examples.py", self.text)
        self.assertIn("python scripts/validate_model_manifest.py", self.text)

    def test_schema_validation_runs_security_privacy_reference_regression(self) -> None:
        self.assertIn("scripts/tests/test_security_privacy_reference.py", self.text)

    def test_packaged_resource_simulation_step_treats_exit_77_as_skip(self) -> None:
        self.assertIn(
            "Sidecar Self-Test (packaged resource simulation, bundled binary)",
            self.text,
        )
        self.assertIn("bash scripts/e2e/test-packaged-resources.sh", self.text)
        self.assertIn('if [ "$rc" -eq 77 ]; then', self.text)
        self.assertIn("Packaged resource simulation skipped on this runner", self.text)
        self.assertIn('exit "$rc"', self.text)

    def test_packaged_resource_simulation_runs_fixture_suite_on_all_os(self) -> None:
        steps = self.wf["jobs"]["python-tests"]["steps"]
        packaged_step = next(
            step
            for step in steps
            if step.get("name") == "Sidecar Self-Test (packaged resource simulation)"
        )
        self.assertNotIn(
            "if",
            packaged_step,
            "packaged resource simulation should run on all matrix OS entries",
        )
        self.assertIn(
            "python -m unittest scripts.tests.test_e2e_packaged_resources_runtime",
            str(packaged_step.get("run", "")),
        )

    def test_packaged_resource_simulation_bundled_binary_step_is_linux_only(self) -> None:
        steps = self.wf["jobs"]["python-tests"]["steps"]
        bundled_binary_step = next(
            step
            for step in steps
            if step.get("name")
            == "Sidecar Self-Test (packaged resource simulation, bundled binary)"
        )
        self.assertEqual(bundled_binary_step.get("if"), "runner.os == 'Linux'")

    def test_typescript_workflow_has_typecheck_and_build_guard_steps(self) -> None:
        steps = self.wf["jobs"]["typescript-tests"]["steps"]
        names = [step.get("name") for step in steps]

        self.assertIn("TypeScript typecheck", names)
        self.assertIn("Verify frontend build produces overlay assets", names)

        typecheck_step = next(step for step in steps if step.get("name") == "TypeScript typecheck")
        build_verify_step = next(
            step
            for step in steps
            if step.get("name") == "Verify frontend build produces overlay assets"
        )

        self.assertEqual(typecheck_step.get("run"), "bunx tsc --noEmit")
        build_script = build_verify_step.get("run", "")
        self.assertIn("bun run build", build_script)
        self.assertIn("test -f dist/index.html", build_script)
        self.assertIn("test -f dist/overlay.html", build_script)


if __name__ == "__main__":
    unittest.main()
