"""Regression checks for overlay config-gate loop polling behavior."""

import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
INTEGRATION_RS = REPO_ROOT / "src-tauri" / "src" / "integration.rs"
COMMANDS_RS = REPO_ROOT / "src-tauri" / "src" / "commands.rs"


def _extract_start_overlay_window_loop_block(source: str) -> str:
    start = source.find("fn start_overlay_window_loop(&self)")
    if start == -1:
        raise AssertionError("start_overlay_window_loop not found")
    end = source.find("fn emit_app_error_event(", start)
    if end == -1:
        raise AssertionError("end marker for start_overlay_window_loop not found")
    return source[start:end]


class OverlayConfigGateLoopTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.integration_text = INTEGRATION_RS.read_text(encoding="utf-8")
        cls.commands_text = COMMANDS_RS.read_text(encoding="utf-8")
        cls.overlay_loop_block = _extract_start_overlay_window_loop_block(cls.integration_text)

    def test_overlay_loop_waits_on_notify_instead_of_periodic_interval(self) -> None:
        self.assertIn("overlay_config_notify.notified().await", self.overlay_loop_block)
        self.assertNotIn("tokio::time::interval(", self.overlay_loop_block)

    def test_config_commands_notify_overlay_loop_on_changes(self) -> None:
        self.assertIn("manager.notify_overlay_config_changed();", self.commands_text)
        self.assertIn("tauri::async_runtime::spawn(async move", self.commands_text)


if __name__ == "__main__":
    unittest.main()
