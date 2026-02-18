import re
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
INTEGRATION_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "integration.yml"


class IntegrationWorkflowTests(unittest.TestCase):
    def test_pytest_failures_are_not_masked(self) -> None:
        workflow = INTEGRATION_WORKFLOW.read_text()

        # Guard against swallowing integration test failures in CI.
        self.assertNotRegex(workflow, re.compile(r"pytest[^\n]*\|\|\s*true"))

    def test_sidecar_ping_verifies_stdin_stdout_rpc(self) -> None:
        workflow = INTEGRATION_WORKFLOW.read_text()

        # Guard against validating an unrelated socket path.
        self.assertNotIn("nc -U /tmp/openvoicy.sock", workflow)

        # Require direct sidecar stdin/stdout invocation and protocol assertion.
        self.assertIn("timeout 10 sidecar/dist/openvoicy-sidecar", workflow)
        self.assertIn("system.ping stdin/stdout RPC verified", workflow)
        self.assertIn('payload.get("result", {}).get("protocol") != "v1"', workflow)


if __name__ == "__main__":
    unittest.main()
