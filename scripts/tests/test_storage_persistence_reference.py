"""Regression checks for shared/STORAGE_PERSISTENCE_MODEL.md storage claims."""

import re
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
REFERENCE = REPO_ROOT / "shared" / "STORAGE_PERSISTENCE_MODEL.md"
LOG_BUFFER = REPO_ROOT / "src-tauri" / "src" / "log_buffer.rs"
LIB_RS = REPO_ROOT / "src-tauri" / "src" / "lib.rs"
CONFIG_RS = REPO_ROOT / "src-tauri" / "src" / "config.rs"
HISTORY_PERSISTENCE_RS = REPO_ROOT / "src-tauri" / "src" / "history_persistence.rs"
MODEL_CACHE_PY = REPO_ROOT / "sidecar" / "src" / "openvoicy_sidecar" / "model_cache.py"


class StoragePersistenceReferenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.reference_text = REFERENCE.read_text(encoding="utf-8")
        cls.log_buffer_text = LOG_BUFFER.read_text(encoding="utf-8")
        cls.lib_text = LIB_RS.read_text(encoding="utf-8")
        cls.config_text = CONFIG_RS.read_text(encoding="utf-8")
        cls.history_persistence_text = HISTORY_PERSISTENCE_RS.read_text(encoding="utf-8")
        cls.model_cache_text = MODEL_CACHE_PY.read_text(encoding="utf-8")

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

    def test_reference_describes_runtime_history_persistence_wiring(self) -> None:
        self.assertIn("build_history_persistence", self.reference_text)
        self.assertIn("TranscriptHistory::with_capacity_and_persistence", self.reference_text)
        self.assertIn("history.jsonl", self.reference_text)

    def test_reference_describes_history_persistence_gating(self) -> None:
        self.assertIn('history.persistence_mode != "disk"', self.reference_text)
        self.assertIn('history.persistence_mode="disk"', self.reference_text)
        self.assertIn("history.encrypt_at_rest=false", self.reference_text)
        self.assertIn("keychain availability", self.reference_text)
        self.assertIn("falls back to memory-only", self.reference_text)

    def test_runtime_wires_history_persistence_backend(self) -> None:
        self.assertIn("build_history_persistence(", self.lib_text)
        self.assertIn("TranscriptHistory::with_capacity_and_persistence(", self.lib_text)

    def test_history_persistence_module_implements_documented_gates(self) -> None:
        self.assertIn('persistence_mode != "disk"', self.history_persistence_text)
        self.assertIn("if !encrypt_at_rest", self.history_persistence_text)
        self.assertIn("EncryptionProvider::from_keychain()", self.history_persistence_text)
        self.assertIn("falling back to memory-only history", self.history_persistence_text.lower())

    def test_runtime_has_no_log_file_persistence_config_surface(self) -> None:
        self.assertNotIn("log_persistence", self.config_text)
        self.assertNotIn("log_file", self.config_text)
        self.assertNotIn("log_path", self.config_text)

    def test_reference_documents_config_lifecycle_atomic_and_recovery_claims(self) -> None:
        self.assertIn("config.json.tmp", self.reference_text)
        self.assertIn("config.json.corrupt", self.reference_text)
        self.assertIn("Rename `.tmp` to `config.json`", self.reference_text)
        self.assertIn("rename bad file to `.corrupt`", self.reference_text)
        self.assertIn("Migration must be additive", self.reference_text)

    def test_runtime_config_implements_tmp_staging_and_atomic_replace(self) -> None:
        self.assertIn('path.with_extension("json.tmp")', self.config_text)
        self.assertIn("replace_config_file(&temp, path)", self.config_text)
        self.assertIn("fn replace_config_file(temp: &PathBuf, path: &PathBuf)", self.config_text)
        self.assertIn("fs::rename(temp, path)", self.config_text)

    def test_runtime_config_implements_corrupt_backup_and_migration_entrypoint(self) -> None:
        self.assertIn('path.with_extension("json.corrupt")', self.config_text)
        self.assertIn("Failed to backup corrupt config", self.config_text)
        self.assertIn("fn migrate_config(mut config: Value) -> AppConfig", self.config_text)
        self.assertIn("Future migrations go here", self.config_text)

    def test_reference_mentions_history_persistence_config_fields(self) -> None:
        self.assertIn("history.persistence_mode", self.reference_text)
        self.assertIn("history.encrypt_at_rest", self.reference_text)

    def test_reference_does_not_claim_history_disk_path_is_unimplemented(self) -> None:
        self.assertNotRegex(
            self.reference_text,
            re.compile(r"disk persistence.*not yet implemented", re.IGNORECASE),
        )

    def test_runtime_builds_history_persistence_backend(self) -> None:
        self.assertIn("build_history_persistence(", self.lib_text)
        self.assertIn("TranscriptHistory::with_capacity_and_persistence(", self.lib_text)
        self.assertIn('config::config_dir().join("history.jsonl")', self.lib_text)

    def test_history_backend_contains_disk_and_encryption_gates(self) -> None:
        self.assertIn('if persistence_mode != "disk"', self.history_persistence_text)
        self.assertIn("if !encrypt_at_rest", self.history_persistence_text)
        self.assertIn("EncryptionProvider::from_keychain()", self.history_persistence_text)
        self.assertIn(
            "falling back to memory-only history for privacy",
            self.history_persistence_text.lower(),
        )

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

    def test_model_cache_module_uses_partial_staging_and_atomic_activation(self) -> None:
        self.assertIn('partial_root = cache_dir / ".partial"', self.model_cache_text)
        self.assertIn("temp_dir = partial_root / manifest.model_id", self.model_cache_text)
        self.assertIn("_activate_staged_model_dir(temp_dir, model_dir)", self.model_cache_text)

    def test_model_cache_module_verifies_sha_and_size_before_activation(self) -> None:
        self.assertIn("hash_ok, actual_sha256 = verify_sha256", self.model_cache_text)
        self.assertIn("if actual_size != file_info.size_bytes", self.model_cache_text)
        self.assertIn('"expected_size_bytes": file_info.size_bytes', self.model_cache_text)

    def test_model_cache_module_purge_semantics_remove_model_directories(self) -> None:
        self.assertIn("def purge_cache(self, model_id: Optional[str] = None)", self.model_cache_text)
        self.assertIn("shutil.rmtree(model_dir)", self.model_cache_text)
        self.assertIn("shutil.rmtree(item)", self.model_cache_text)


if __name__ == "__main__":
    unittest.main()
