import os
import stat
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SELF_TEST_WRAPPER = REPO_ROOT / "sidecar" / "self-test"


class SidecarSelfTestWrapperTests(unittest.TestCase):
    def test_wrapper_script_exists_and_is_executable(self) -> None:
        """Regression (221f): self-test must be runnable from source tree."""
        self.assertTrue(
            SELF_TEST_WRAPPER.is_file(),
            "sidecar/self-test wrapper script must exist",
        )
        mode = SELF_TEST_WRAPPER.stat().st_mode
        self.assertTrue(
            mode & stat.S_IXUSR,
            "sidecar/self-test must be executable",
        )

    def test_wrapper_sets_pythonpath_to_src(self) -> None:
        content = SELF_TEST_WRAPPER.read_text()
        self.assertIn("PYTHONPATH", content)
        self.assertIn("src", content)
        self.assertIn("openvoicy_sidecar.self_test", content)


if __name__ == "__main__":
    unittest.main()
