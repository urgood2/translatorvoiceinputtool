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
        path.write_text(
            json.dumps(
                {
                    "items": [
                        {
                            "type": "event",
                            "name": "state:changed",
                            "payload_schema": {"$ref": "#/$defs/state_changed_payload"},
                        }
                    ],
                    "$defs": {
                        "state_changed_payload": {
                            "type": "object",
                            "properties": {"detail": {"type": ["string", "null"]}},
                        }
                    },
                }
            )
            + "\n",
            encoding="utf-8",
        )

    # Minimal contract items matching the real tauri.events.v1.json (aliases retired)
    _DEFAULT_EVENT_ITEMS: list[dict] = [
        {
            "type": "event",
            "name": "state:changed",
            "deprecated_aliases": [],
            "payload_schema": {"$ref": "#/$defs/state_changed_payload"},
        },
        {"type": "event", "name": "transcript:complete", "deprecated_aliases": []},
        {"type": "event", "name": "transcript:error", "deprecated_aliases": []},
        {"type": "event", "name": "sidecar:status", "deprecated_aliases": []},
        {"type": "event", "name": "model:status"},
        {"type": "event", "name": "recording:status"},
    ]

    @staticmethod
    def _default_tauri_events_contract(items: list[dict]) -> dict:
        return {
            "items": items,
            "$defs": {
                "state_changed_payload": {
                    "type": "object",
                    "properties": {
                        "seq": {"type": "integer"},
                        "state": {"type": "string"},
                        "enabled": {"type": "boolean"},
                        "detail": {"type": ["string", "null"]},
                        "timestamp": {"type": "string"},
                    },
                }
            },
        }

    def _validate_fixture_category(
        self,
        root: Path,
        *,
        examples_rows: list[dict],
        event_items: list[dict] | None = None,
        events_contract: dict | None = None,
    ) -> tuple[list[str], list[str]]:
        examples_path = root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
        self._write_jsonl(examples_path, examples_rows)
        self._write_events_contract(root / "shared" / "contracts" / "tauri.events.v1.json")
        items = event_items if event_items is not None else self._DEFAULT_EVENT_ITEMS
        tauri_events_contract = (
            events_contract if events_contract is not None else self._default_tauri_events_contract(items)
        )
        contracts = {"sidecar.rpc": {"items": []}, "tauri.events": tauri_events_contract}

        with (
            patch.object(MODULE.vc, "EXAMPLES_PATH", examples_path),
            patch.object(MODULE.vc, "validate_sidecar_examples_against_contract", return_value=[]),
            patch.object(MODULE.vc, "validate_tauri_event_payload_examples", return_value=[]),
        ):
            return MODULE.validate_fixture_category(root, contracts)

    def test_validate_fixture_category_enforces_model_status_mapped_payload(self) -> None:
        """With no deprecated aliases, only model:status mapped payload fixture is enforced."""
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
                            "params": {"state": "idle"},
                        },
                    }
                ],
            )

            self.assertTrue(any("missing mapped payload fixture for 'model:status'" in err for err in errors))

    def test_validate_fixture_category_accepts_required_canonical_mapped_fixtures(self) -> None:
        """With no deprecated aliases, only model:status mapped payload is required."""

        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            _summaries, errors = self._validate_fixture_category(
                root,
                examples_rows=[
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

    def test_validate_fixture_category_rejects_undeclared_legacy_alias_when_aliases_retired(self) -> None:
        """Mapped fixture events must be declared by contract canonical names or deprecated_aliases."""
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            _summaries, errors = self._validate_fixture_category(
                root,
                examples_rows=[
                    {
                        "type": "notification",
                        "data": {
                            "jsonrpc": "2.0",
                            "method": "event.transcript_complete_legacy",
                            "params": {
                                "mapped_tauri_event": "transcription:complete",
                                "mapped_tauri_payload_legacy": {"text": "legacy"},
                            },
                        },
                    },
                    {
                        "type": "notification",
                        "data": {
                            "jsonrpc": "2.0",
                            "method": "event.model_status",
                            "params": {
                                "mapped_tauri_event": "model:status",
                                "mapped_tauri_payload_canonical": {"status": "ready"},
                            },
                        },
                    },
                ],
            )

            self.assertTrue(
                any("uses undeclared event name 'transcription:complete'" in err for err in errors),
                msg=str(errors),
            )

    def test_validate_fixture_category_requires_state_detail_in_state_payload_schema(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            tauri_events_contract = {
                "items": [
                    {
                        "type": "event",
                        "name": "state:changed",
                        "payload_schema": {"$ref": "#/$defs/state_changed_payload"},
                    },
                    {"type": "event", "name": "model:status"},
                ],
                "$defs": {
                    # Include an unrelated "detail" elsewhere to ensure checks target state payload path.
                    "unrelated_payload": {"type": "object", "properties": {"detail": {"type": "string"}}},
                    "state_changed_payload": {
                        "type": "object",
                        "properties": {
                            "seq": {"type": "integer"},
                            "state": {"type": "string"},
                            "enabled": {"type": "boolean"},
                            "timestamp": {"type": "string"},
                        },
                    },
                },
            }
            _summaries, errors = self._validate_fixture_category(
                root,
                examples_rows=[
                    {
                        "type": "notification",
                        "data": {
                            "jsonrpc": "2.0",
                            "method": "event.model_status",
                            "params": {
                                "mapped_tauri_event": "model:status",
                                "mapped_tauri_payload_canonical": {"status": "ready"},
                            },
                        },
                    },
                ],
                events_contract=tauri_events_contract,
            )

            self.assertTrue(
                any("state:changed payload schema must include canonical 'detail' field" in err for err in errors),
                msg=str(errors),
            )

    def test_validate_fixture_category_rejects_legacy_error_detail_in_state_payload_schema(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            tauri_events_contract = {
                "items": [
                    {
                        "type": "event",
                        "name": "state:changed",
                        "payload_schema": {"$ref": "#/$defs/state_changed_payload"},
                    },
                    {"type": "event", "name": "model:status"},
                ],
                "$defs": {
                    "state_changed_payload": {
                        "type": "object",
                        "properties": {
                            "seq": {"type": "integer"},
                            "state": {"type": "string"},
                            "enabled": {"type": "boolean"},
                            "detail": {"type": "string"},
                            "error_detail": {"type": "string"},
                            "timestamp": {"type": "string"},
                        },
                    }
                },
            }
            _summaries, errors = self._validate_fixture_category(
                root,
                examples_rows=[
                    {
                        "type": "notification",
                        "data": {
                            "jsonrpc": "2.0",
                            "method": "event.model_status",
                            "params": {
                                "mapped_tauri_event": "model:status",
                                "mapped_tauri_payload_canonical": {"status": "ready"},
                            },
                        },
                    },
                ],
                events_contract=tauri_events_contract,
            )

            self.assertTrue(
                any("state:changed payload schema should not include legacy 'error_detail' field" in err for err in errors),
                msg=str(errors),
            )

    def test_fixture_alias_coverage_derived_from_contract_not_hardcoded(self) -> None:
        """Regression (kex5): required fixture set must be driven by contract deprecated_aliases."""
        # Synthetic contract with a custom event + alias not in the real contract
        custom_items = [
            {"type": "event", "name": "custom:event", "deprecated_aliases": ["custom_legacy"]},
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
        items = [{"type": "event", "name": "model:status"}]  # no deprecated_aliases
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

    def test_drift_prevention_category_enforces_legacy_alias_scenarios(self) -> None:
        summaries, errors = MODULE.validate_drift_prevention_category()
        self.assertEqual(errors, [])
        self.assertIn(
            "legacy alias fixture coverage checks remain enforced via synthetic contract",
            summaries,
        )


if __name__ == "__main__":
    unittest.main()
