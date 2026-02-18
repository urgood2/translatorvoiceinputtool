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


if __name__ == "__main__":
    unittest.main()
