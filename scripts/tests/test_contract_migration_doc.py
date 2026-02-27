import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
MIGRATION_DOC = REPO_ROOT / "shared" / "contracts" / "MIGRATION.md"
EVENTS_CONTRACT = REPO_ROOT / "shared" / "contracts" / "tauri.events.v1.json"


class ContractMigrationDocTests(unittest.TestCase):
    def test_migration_doc_records_alias_retirement_and_canonical_only_policy(self) -> None:
        content = MIGRATION_DOC.read_text(encoding="utf-8")

        self.assertIn("compat_window_active: false", content)
        self.assertIn("Legacy event names listed above are unsupported", content)
        self.assertIn("state:changed", content)
        self.assertIn("transcript:complete", content)
        self.assertIn("transcript:error", content)
        self.assertIn("sidecar:status", content)
        self.assertIn("2026-02-26", content)

    def test_tauri_events_contract_is_canonical_only_for_aliases(self) -> None:
        content = EVENTS_CONTRACT.read_text(encoding="utf-8")
        self.assertNotIn('"deprecated_aliases": ["state_changed"]', content)
        self.assertNotIn('"deprecated_aliases": ["transcription:complete"]', content)
        self.assertNotIn('"deprecated_aliases": ["transcription:error"]', content)
        self.assertNotIn('"deprecated_aliases": ["status:changed"]', content)


if __name__ == "__main__":
    unittest.main()
