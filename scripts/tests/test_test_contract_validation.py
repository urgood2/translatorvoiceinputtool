import importlib.util
import contextlib
import io
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

    # Minimal contract items matching the real tauri.events.v1.json aliases
    _DEFAULT_EVENT_ITEMS: list[dict] = [
        {"name": "state:changed", "deprecated_aliases": ["state_changed"]},
        {"name": "transcript:complete", "deprecated_aliases": ["transcription:complete"]},
        {"name": "transcript:error", "deprecated_aliases": ["transcription:error"]},
        {"name": "sidecar:status", "deprecated_aliases": ["status:changed"]},
        {"name": "model:status"},
        {"name": "recording:status"},
    ]

    def _validate_fixture_category(
        self,
        root: Path,
        *,
        examples_rows: list[dict],
        event_items: list[dict] | None = None,
    ) -> tuple[list[str], list[str]]:
        examples_path = root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
        self._write_jsonl(examples_path, examples_rows)
        self._write_events_contract(root / "shared" / "contracts" / "tauri.events.v1.json")
        items = event_items if event_items is not None else self._DEFAULT_EVENT_ITEMS
        contracts = {"sidecar.rpc": {"items": []}, "tauri.events": {"items": items}}

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
        """All canonical events with aliases, plus the aliases themselves, must have fixtures."""

        def _notif(event_name: str, **extra: object) -> dict:
            params: dict = {"mapped_tauri_event": event_name, "mapped_tauri_payload": {"status": "ok"}}
            params.update(extra)
            return {
                "type": "notification",
                "data": {"jsonrpc": "2.0", "method": "event.x", "params": params},
            }

        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            _summaries, errors = self._validate_fixture_category(
                root,
                examples_rows=[
                    _notif("state:changed"),
                    _notif("state_changed"),
                    _notif("transcript:complete"),
                    _notif("transcription:complete"),
                    _notif("transcript:error"),
                    _notif("transcription:error"),
                    _notif("sidecar:status"),
                    _notif("status:changed"),
                    {
                        "type": "notification",
                        "data": {
                            "jsonrpc": "2.0",
                            "method": "event.model_status",
                            "params": {
                                "mapped_tauri_event": "model:status",
                                "mapped_tauri_payload_canonical": {"status": "loading"},
                            },
                        },
                    },
                ],
            )

            self.assertEqual(errors, [])


    def test_fixture_alias_coverage_derived_from_contract_not_hardcoded(self) -> None:
        """Regression (kex5): required fixture set must be driven by contract deprecated_aliases."""
        # Synthetic contract with a custom event + alias not in the real contract
        custom_items = [
            {"name": "custom:event", "deprecated_aliases": ["custom_legacy"]},
        ]
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            # Provide NO mapped fixtures at all
            _summaries, errors = self._validate_fixture_category(
                root,
                examples_rows=[],
                event_items=custom_items,
            )
            # Must require fixtures for both canonical and alias
            missing = [e for e in errors if "missing drift-guard mapped_tauri_event fixture" in e]
            self.assertEqual(len(missing), 2)
            self.assertTrue(any("'custom:event'" in e for e in missing))
            self.assertTrue(any("'custom_legacy'" in e for e in missing))

    def test_events_without_aliases_do_not_require_alias_fixtures(self) -> None:
        """Events without deprecated_aliases should not generate alias fixture requirements."""
        items = [{"name": "model:status"}]  # no deprecated_aliases
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            _summaries, errors = self._validate_fixture_category(
                root,
                examples_rows=[],
                event_items=items,
            )
            alias_errors = [e for e in errors if "missing drift-guard" in e]
            self.assertEqual(alias_errors, [])

    def test_drift_prevention_category_does_not_emit_misleading_fail_logs_on_success(self) -> None:
        """Regression (1z8f): expected-negative synthetic checks should not print FAIL lines."""
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            _summaries, errors = MODULE.validate_drift_prevention_category()

        self.assertEqual(errors, [])
        self.assertNotIn("FAIL: sidecar HANDLERS includes method", output.getvalue())


if __name__ == "__main__":
    unittest.main()
