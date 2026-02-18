import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
BUILD_SCRIPT = REPO_ROOT / "scripts" / "build-sidecar.sh"


class BuildSidecarTimeoutTests(unittest.TestCase):
    def test_build_script_uses_portable_timeout_runner(self) -> None:
        content = BUILD_SCRIPT.read_text()

        self.assertIn("sidecar_timeout_runner()", content)
        self.assertIn("sidecar_timeout_run()", content)
        self.assertIn("command -v gtimeout", content)
        self.assertIn("python3 - \"$seconds\" \"$@\"", content)

        # Regression guard: do not reintroduce hard dependency on GNU timeout.
        self.assertNotIn("| timeout 10 \"$BINARY_PATH\"", content)


if __name__ == "__main__":
    unittest.main()
