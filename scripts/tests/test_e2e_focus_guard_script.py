import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
FOCUS_GUARD_SCRIPT = REPO_ROOT / "scripts" / "e2e" / "test-focus-guard.sh"


class FocusGuardScriptTests(unittest.TestCase):
    def test_meter_start_error_check_validates_specific_kind(self) -> None:
        """Regression (1b2m): must assert a method-specific error kind, not any error."""
        content = FOCUS_GUARD_SCRIPT.read_text()
        self.assertIn("E_DEVICE_NOT_FOUND", content)
        # Must NOT pass on generic or unknown error kinds
        self.assertNotIn(
            '[ "$error_kind" != "unknown" ] && [ "$error_kind" != "null" ]',
            content,
            "Error check must validate specific error kinds, not accept any non-null kind",
        )


if __name__ == "__main__":
    unittest.main()
