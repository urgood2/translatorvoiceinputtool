import subprocess
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]


def run_bash(script: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["bash", "-lc", script],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )


class E2ECommonTimeoutTests(unittest.TestCase):
    def test_python_runner_times_out(self) -> None:
        result = run_bash(
            """
            source scripts/e2e/lib/common.sh
            set +e
            export E2E_TIMEOUT_RUNNER=python3
            e2e_timeout_run 0.2 bash -lc 'sleep 1'
            exit $?
            """
        )
        self.assertEqual(result.returncode, 124, msg=result.stderr)

    def test_python_runner_passthrough_stdout(self) -> None:
        result = run_bash(
            """
            source scripts/e2e/lib/common.sh
            set +e
            export E2E_TIMEOUT_RUNNER=python3
            e2e_timeout_run 2 bash -lc 'printf ok'
            """
        )
        self.assertEqual(result.returncode, 0, msg=result.stderr)
        self.assertEqual(result.stdout, "ok")

    def test_timeout_runner_auto_detects_supported_tool(self) -> None:
        result = run_bash(
            """
            source scripts/e2e/lib/common.sh
            set +e
            e2e_timeout_runner
            """
        )
        self.assertEqual(result.returncode, 0, msg=result.stderr)
        self.assertIn(result.stdout.strip(), {"timeout", "gtimeout", "python3"})


if __name__ == "__main__":
    unittest.main()
