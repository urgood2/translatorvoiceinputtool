"""Regression checks for shared/STORAGE_PERSISTENCE_MODEL.md log-storage claims."""

import re
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
REFERENCE = REPO_ROOT / "shared" / "STORAGE_PERSISTENCE_MODEL.md"
LOG_BUFFER = REPO_ROOT / "src-tauri" / "src" / "log_buffer.rs"
LIB_RS = REPO_ROOT / "src-tauri" / "src" / "lib.rs"
CONFIG_RS = REPO_ROOT / "src-tauri" / "src" / "config.rs"


class StoragePersistenceReferenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.reference_text = REFERENCE.read_text(encoding="utf-8")
        cls.log_buffer_text = LOG_BUFFER.read_text(encoding="utf-8")
        cls.lib_text = LIB_RS.read_text(encoding="utf-8")
        cls.config_text = CONFIG_RS.read_text(encoding="utf-8")

    def test_reference_states_no_persistent_log_sink_is_implemented(self) -> None:
        self.assertRegex(
            self.reference_text,
            re.compile(r"no persistent file-log sink or rotation path is implemented", re.IGNORECASE),
        )

    def test_reference_does_not_claim_rotated_file_logs_exist_today(self) -> None:
        self.assertNotIn(
            "Optional persistent logs: rotated file logs used only for diagnostics/debug workflows.",
            self.reference_text,
        )

    def test_runtime_initializes_ring_buffer_logger(self) -> None:
        self.assertIn("log_buffer::init_buffer_logger(log::Level::Info);", self.lib_text)

    def test_runtime_has_no_log_file_persistence_config_surface(self) -> None:
        self.assertNotIn("log_persistence", self.config_text)
        self.assertNotIn("log_file", self.config_text)
        self.assertNotIn("log_path", self.config_text)

    def test_log_buffer_module_has_no_filesystem_write_path(self) -> None:
        forbidden_markers = [
            "OpenOptions",
            "File::create",
            "File::open",
            "std::fs::File",
            "create_dir_all",
        ]
        for marker in forbidden_markers:
            self.assertNotIn(marker, self.log_buffer_text)


if __name__ == "__main__":
    unittest.main()
