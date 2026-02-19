import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


SCRIPTS_DIR = Path(__file__).resolve().parents[1]
VALIDATE_CONTRACTS_PATH = SCRIPTS_DIR / "validate_contracts.py"
VALIDATE_SPEC = importlib.util.spec_from_file_location("validate_contracts", VALIDATE_CONTRACTS_PATH)
VALIDATE_MODULE = importlib.util.module_from_spec(VALIDATE_SPEC)
assert VALIDATE_SPEC and VALIDATE_SPEC.loader
sys.modules[VALIDATE_SPEC.name] = VALIDATE_MODULE
VALIDATE_SPEC.loader.exec_module(VALIDATE_MODULE)

SCRIPT_PATH = SCRIPTS_DIR / "test_contract_validation.py"
SPEC = importlib.util.spec_from_file_location("test_contract_validation", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class TestContractValidationTests(unittest.TestCase):
    @staticmethod
    def _write_jsonl(path: Path, rows: list[dict]) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("\n".join(json.dumps(row) for row in rows) + "\n", encoding="utf-8")

    @staticmethod
    def _write_events_contract(path: Path) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('{"detail":"present"}\n', encoding="utf-8")

    def _validate_fixture_category(
        self,
        root: Path,
        *,
        examples_rows: list[dict],
    ) -> tuple[list[str], list[str]]:
        examples_path = root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
        self._write_jsonl(examples_path, examples_rows)
        self._write_events_contract(root / "shared" / "contracts" / "tauri.events.v1.json")
        contracts = {"sidecar.rpc": {"items": []}, "tauri.events": {"items": []}}

        with (
            patch.object(MODULE.vc, "EXAMPLES_PATH", examples_path),
            patch.object(MODULE.vc, "validate_sidecar_examples_against_contract", return_value=[]),
            patch.object(MODULE.vc, "validate_tauri_event_payload_examples", return_value=[]),
        ):
            return MODULE.validate_fixture_category(root, contracts)

    def test_validate_fixture_category_enforces_drift_fixtures_from_canonical_corpus(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            fixture = root / "src" / "hooks" / "useTauriEvents.test.ts"
            fixture.parent.mkdir(parents=True, exist_ok=True)
            fixture.write_text(
                "\n".join(
                    [
                        "emitMockEvent('state:changed', { status: 'ready' });",
                        "emitMockEvent('model:status', { status: 'ready' });",
                        "emitMockEvent('transcript:complete', { status: 'ready' });",
                        "emitMockEvent('transcription:complete', { status: 'ready' });",
                    ]
                ),
                encoding="utf-8",
            )
            _summaries, errors = self._validate_fixture_category(
                root,
                examples_rows=[
                    {
                        "type": "notification",
                        "data": {
                            "jsonrpc": "2.0",
                            "method": "event.status_changed",
                            "params": {"state": "idle"},
                        },
                    }
                ],
            )

            self.assertTrue(
                any(
                    "missing drift-guard mapped_tauri_event fixture for 'transcript:complete'" in err
                    for err in errors
                )
            )
            self.assertTrue(any("missing mapped payload fixture for 'model:status'" in err for err in errors))
            self.assertFalse(any("useTauriEvents.test.ts: missing drift-guard payload fixture" in err for err in errors))

    def test_validate_fixture_category_accepts_required_canonical_mapped_fixtures(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            _summaries, errors = self._validate_fixture_category(
                root,
                examples_rows=[
                    {
                        "type": "notification",
                        "data": {
                            "jsonrpc": "2.0",
                            "method": "event.status_changed",
                            "params": {
                                "mapped_tauri_event": "state:changed",
                                "mapped_tauri_payload": {"status": "ready"},
                            },
                        },
                    },
                    {
                        "type": "notification",
                        "data": {
                            "jsonrpc": "2.0",
                            "method": "event.status_changed",
                            "params": {
                                "mapped_tauri_event": "model:status",
                                "mapped_tauri_payload_canonical": {"status": "loading"},
                            },
                        },
                    },
                    {
                        "type": "notification",
                        "data": {
                            "jsonrpc": "2.0",
                            "method": "event.transcription_complete",
                            "params": {
                                "mapped_tauri_event": "transcript:complete",
                                "mapped_tauri_payload": {"status": "ready"},
                            },
                        },
                    },
                    {
                        "type": "notification",
                        "data": {
                            "jsonrpc": "2.0",
                            "method": "event.transcription_complete",
                            "params": {
                                "mapped_tauri_event": "transcription:complete",
                                "mapped_tauri_payload": {"status": "ready"},
                            },
                        },
                    },
                ],
            )

            self.assertEqual(errors, [])


if __name__ == "__main__":
    unittest.main()
