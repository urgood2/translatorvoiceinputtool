import importlib.util
import io
import json
import sys
import tempfile
import unittest
import warnings
from contextlib import redirect_stdout
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "validate_contracts.py"
SPEC = importlib.util.spec_from_file_location("validate_contracts", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class ValidateContractsTests(unittest.TestCase):
    @staticmethod
    def _write_jsonl(path: Path, rows: list[dict]) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("\n".join(json.dumps(row) for row in rows) + "\n", encoding="utf-8")

    def test_extract_listen_event_names_from_text_resolves_constants(self) -> None:
        text = """
const EVENTS = {
  STATE_CHANGED: 'state:changed',
  SIDECAR_STATUS: 'sidecar:status',
} as const;

listen(EVENTS.STATE_CHANGED, () => {});
registerListener<Record<string, unknown>>(EVENTS.SIDECAR_STATUS, () => {});
listen('sidecar:status', () => {});
        """.strip()
        rows = MODULE.extract_listen_event_names_from_text(text)
        names = [event_name for _line, _expr, event_name in rows if event_name is not None]
        self.assertIn("state:changed", names)
        self.assertIn("sidecar:status", names)

    def test_coerce_js_object_literal_to_json_text(self) -> None:
        payload = "{ seq: 1, state: 'idle', enabled: true, detail: undefined, }"
        parsed = json.loads(MODULE.coerce_js_object_literal_to_json_text(payload))
        self.assertEqual(parsed["seq"], 1)
        self.assertEqual(parsed["state"], "idle")
        self.assertTrue(parsed["enabled"])
        self.assertIsNone(parsed["detail"])

    def test_coerce_js_object_literal_to_json_text_accepts_typescript_runtime_constructs(self) -> None:
        payload = """
{
  seq: 42,
  entry: {
    id: 'entry-42',
    timestamp: new Date().toISOString(),
    injection_result: { status: 'injected' as const },
  },
}
        """.strip()
        parsed = json.loads(MODULE.coerce_js_object_literal_to_json_text(payload))
        self.assertEqual(parsed["seq"], 42)
        self.assertEqual(parsed["entry"]["id"], "entry-42")
        self.assertEqual(parsed["entry"]["timestamp"], "1970-01-01T00:00:00.000Z")
        self.assertEqual(parsed["entry"]["injection_result"]["status"], "injected")

    def test_validate_instance_against_schema_reports_required_field(self) -> None:
        root = {"$schema": "http://json-schema.org/draft-07/schema#", "type": "object"}
        schema = {
            "type": "object",
            "required": ["state"],
            "properties": {"state": {"type": "string"}},
            "additionalProperties": False,
        }
        errors = MODULE.validate_instance_against_schema({}, schema, root, "event.payload")
        self.assertEqual(len(errors), 1)
        self.assertIn("state", errors[0])

    def test_validate_instance_against_schema_resolves_root_defs_without_refresolver(self) -> None:
        root = {
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$defs": {
                "payload": {
                    "type": "object",
                    "required": ["status"],
                    "properties": {"status": {"type": "string"}},
                    "additionalProperties": False,
                }
            },
        }
        schema = {"$ref": "#/$defs/payload"}
        errors = MODULE.validate_instance_against_schema({}, schema, root, "event.payload")
        self.assertEqual(len(errors), 1)
        self.assertIn("status", errors[0])

    def test_validate_contracts_script_no_longer_uses_refresolver(self) -> None:
        source = SCRIPT_PATH.read_text(encoding="utf-8")
        self.assertNotRegex(source, r"from\s+jsonschema\s+import[^\n]*RefResolver")
        self.assertNotIn("RefResolver.from_schema", source)

    def test_validate_instance_against_schema_emits_no_deprecation_warning(self) -> None:
        root = {
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$defs": {
                "payload": {
                    "type": "object",
                    "required": ["status"],
                    "properties": {"status": {"type": "string"}},
                    "additionalProperties": False,
                }
            },
        }
        schema = {"$ref": "#/$defs/payload"}

        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always", DeprecationWarning)
            errors = MODULE.validate_instance_against_schema({}, schema, root, "event.payload")

        self.assertEqual(len(errors), 1)
        deprecations = [
            str(w.message)
            for w in caught
            if issubclass(w.category, DeprecationWarning)
        ]
        self.assertEqual(deprecations, [])

    def test_validate_legacy_alias_fixture_coverage_accepts_complete_pairs(self) -> None:
        events_contract = {
            "items": [
                {
                    "type": "event",
                    "name": "state:changed",
                    "deprecated_aliases": ["state_changed"],
                },
                {
                    "type": "event",
                    "name": "transcript:complete",
                    "deprecated_aliases": ["transcription:complete"],
                },
            ]
        }
        seen = {"state:changed", "state_changed", "transcript:complete", "transcription:complete"}
        errors = MODULE.validate_legacy_alias_fixture_coverage(events_contract, seen)
        self.assertEqual(errors, [])

    def test_validate_legacy_alias_fixture_coverage_reports_missing_alias_or_canonical(self) -> None:
        events_contract = {
            "items": [
                {
                    "type": "event",
                    "name": "state:changed",
                    "deprecated_aliases": ["state_changed"],
                }
            ]
        }
        errors = MODULE.validate_legacy_alias_fixture_coverage(events_contract, {"state:changed"})
        self.assertEqual(len(errors), 1)
        self.assertIn("state_changed", errors[0])

    def test_validate_legacy_alias_fixture_coverage_allows_retired_alias_policy(self) -> None:
        events_contract = {
            "items": [
                {"type": "event", "name": "state:changed", "deprecated_aliases": []},
                {"type": "event", "name": "transcript:complete"},
            ]
        }
        seen = {"state:changed", "transcript:complete"}
        errors = MODULE.validate_legacy_alias_fixture_coverage(events_contract, seen)
        self.assertEqual(errors, [])

    def test_tauri_event_name_maps_returns_empty_alias_map_when_retired(self) -> None:
        events_contract = {
            "items": [
                {"type": "event", "name": "state:changed", "deprecated_aliases": []},
                {"type": "event", "name": "transcript:complete"},
            ]
        }
        canonical_names, alias_to_canonical = MODULE.tauri_event_name_maps(events_contract)
        self.assertIn("state:changed", canonical_names)
        self.assertIn("transcript:complete", canonical_names)
        self.assertEqual(alias_to_canonical, {})

    def test_extract_event_payload_examples_honors_ignore_marker(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            fixture = Path(tmpdir) / "fixture.test.ts"
            fixture.write_text(
                "\n".join(
                    [
                        "emitMockEvent('state:changed', { seq: 1, state: 'idle', enabled: true });",
                        "// contract-validate-ignore: legacy shape",
                        "emitMockEvent('transcript:complete', { text: 'legacy' });",
                        "fireMockEventWithLog('app:error', { seq: 2, error: { code: 'E', message: 'x', recoverable: false } });",
                    ]
                ),
                encoding="utf-8",
            )
            rows = MODULE.extract_event_payload_examples_from_test_file(fixture)
            names = [name for _line, name, _payload in rows]
            self.assertEqual(names, ["state:changed", "app:error"])

    def test_validate_generated_files_detects_drift(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            scripts_dir = root / "scripts"
            scripts_dir.mkdir(parents=True)
            out_dir = root / "src"
            out_dir.mkdir(parents=True)

            generator = scripts_dir / "gen_contracts_ts.py"
            generator.write_text(
                "\n".join(
                    [
                        "import argparse",
                        "from pathlib import Path",
                        "p=argparse.ArgumentParser()",
                        "p.add_argument('--repo-root')",
                        "p.add_argument('--out')",
                        "a=p.parse_args()",
                        "Path(a.out).write_text('fresh\\n')",
                    ]
                )
            )

            target = out_dir / "types.contracts.ts"
            target.write_text("stale\n")
            errors = MODULE.run_generator_and_diff(root, "scripts/gen_contracts_ts.py", "src/types.contracts.ts")
            self.assertEqual(len(errors), 1)
            self.assertIn("out of date", errors[0])

    def test_validate_derived_fixture_corpus_invokes_check_script(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            scripts_dir = root / "scripts"
            scripts_dir.mkdir(parents=True, exist_ok=True)
            derived_dir = root / "shared" / "contracts" / "examples"
            derived_dir.mkdir(parents=True, exist_ok=True)
            (derived_dir / "IPC_V1_EXAMPLES.jsonl").write_text("{}\n", encoding="utf-8")

            check_script = scripts_dir / "gen_contract_examples.py"
            check_script.write_text(
                "\n".join(
                    [
                        "import argparse",
                        "import sys",
                        "p=argparse.ArgumentParser()",
                        "p.add_argument('--repo-root')",
                        "p.add_argument('--check', action='store_true')",
                        "p.parse_args()",
                        "sys.exit(1)",
                    ]
                ),
                encoding="utf-8",
            )

            errors = MODULE.validate_derived_fixture_corpus(root)
            self.assertEqual(len(errors), 1)
            self.assertIn("derived fixture corpus check failed", errors[0])

    def test_validate_frontend_listener_events_reports_undeclared_event(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            hook_file = root / "src" / "hooks" / "useTauriEvents.ts"
            hook_file.parent.mkdir(parents=True, exist_ok=True)
            hook_file.write_text(
                "\n".join(
                    [
                        "import { listen } from '@tauri-apps/api/event';",
                        "void listen('state:changed', () => {});",
                        "void listen('state:changd', () => {});",
                    ]
                ),
                encoding="utf-8",
            )

            events_contract = {
                "items": [
                    {
                        "type": "event",
                        "name": "state:changed",
                        "payload_schema": {"type": "object"},
                    }
                ]
            }
            errors = MODULE.validate_frontend_listener_events(root, events_contract)
            self.assertEqual(len(errors), 1)
            self.assertIn("state:changd", errors[0])
            self.assertIn("undeclared event", errors[0])

    def test_validate_frontend_listener_events_warns_alias_without_canonical_listener(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            hook_file = root / "src" / "hooks" / "useTauriEvents.ts"
            hook_file.parent.mkdir(parents=True, exist_ok=True)
            hook_file.write_text(
                "\n".join(
                    [
                        "import { listen } from '@tauri-apps/api/event';",
                        "void listen('state_changed', () => {});",
                    ]
                ),
                encoding="utf-8",
            )

            events_contract = {
                "items": [
                    {
                        "type": "event",
                        "name": "state:changed",
                        "deprecated_aliases": ["state_changed"],
                        "payload_schema": {"type": "object"},
                    }
                ]
            }

            stream = io.StringIO()
            with redirect_stdout(stream):
                errors = MODULE.validate_frontend_listener_events(root, events_contract)
            self.assertEqual(errors, [])

            output = stream.getvalue()
            self.assertIn("WARN:", output)
            self.assertIn("uses legacy alias", output)
            self.assertIn("without canonical listener 'state:changed'", output)
            self.assertIn("1 listeners checked, 1 valid, 1 using legacy aliases", output)

    def test_validate_frontend_listener_events_accepts_overlay_toggle_listener(self) -> None:
        """Regression: declared overlay listener should validate cleanly."""
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            overlay_file = root / "src" / "overlay" / "OverlayApp.tsx"
            overlay_file.parent.mkdir(parents=True, exist_ok=True)
            overlay_file.write_text(
                "\n".join(
                    [
                        "import { listen } from '@tauri-apps/api/event';",
                        "void listen('overlay:toggle', () => {});",
                    ]
                ),
                encoding="utf-8",
            )

            events_contract = {
                "items": [
                    {
                        "type": "event",
                        "name": "overlay:toggle",
                        "payload_schema": {"type": "object"},
                    }
                ]
            }

            errors = MODULE.validate_frontend_listener_events(root, events_contract)
            self.assertEqual(errors, [])

    def test_validate_rust_event_payloads_accepts_matching_payload_shape(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            rust_file = root / "src-tauri" / "src" / "integration.rs"
            rust_file.parent.mkdir(parents=True, exist_ok=True)
            rust_file.write_text(
                "\n".join(
                    [
                        "use serde_json::{json, Value};",
                        'const EVENT_STATE_CHANGED: &str = "state:changed";',
                        "fn state_changed_event_payload() -> Value {",
                        '  json!({ "state": "idle", "enabled": true, "timestamp": "2026-02-19T00:00:00Z", "detail": null })',
                        "}",
                        "fn emit_with_shared_seq<T>(_handle: &T, _events: &[&str], _payload: Value, _seq: &u64) {}",
                        "fn wire(handle: &u8, event_seq: &u64) {",
                        "  emit_with_shared_seq(handle, &[EVENT_STATE_CHANGED], state_changed_event_payload(), event_seq);",
                        "}",
                    ]
                ),
                encoding="utf-8",
            )

            events_contract = {
                "items": [
                    {
                        "type": "event",
                        "name": "state:changed",
                        "payload_schema": {
                            "type": "object",
                            "required": ["seq", "state", "enabled", "timestamp"],
                            "properties": {
                                "seq": {"type": "integer"},
                                "state": {"type": "string"},
                                "enabled": {"type": "boolean"},
                                "timestamp": {"type": "string"},
                                "detail": {"type": ["string", "null"]},
                            },
                            "additionalProperties": False,
                        },
                    }
                ]
            }

            errors = MODULE.validate_rust_event_payloads(root, events_contract)
            self.assertEqual(errors, [])

    def test_validate_rust_event_payloads_reports_extra_field(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            rust_file = root / "src-tauri" / "src" / "integration.rs"
            rust_file.parent.mkdir(parents=True, exist_ok=True)
            rust_file.write_text(
                "\n".join(
                    [
                        "use serde_json::{json, Value};",
                        'const EVENT_STATE_CHANGED: &str = "state:changed";',
                        "fn state_changed_event_payload() -> Value {",
                        '  json!({ "state": "idle", "enabled": true, "timestamp": "2026-02-19T00:00:00Z", "unexpected": 1 })',
                        "}",
                        "fn emit_with_shared_seq<T>(_handle: &T, _events: &[&str], _payload: Value, _seq: &u64) {}",
                        "fn wire(handle: &u8, event_seq: &u64) {",
                        "  emit_with_shared_seq(handle, &[EVENT_STATE_CHANGED], state_changed_event_payload(), event_seq);",
                        "}",
                    ]
                ),
                encoding="utf-8",
            )

            events_contract = {
                "items": [
                    {
                        "type": "event",
                        "name": "state:changed",
                        "payload_schema": {
                            "type": "object",
                            "required": ["seq", "state", "enabled", "timestamp"],
                            "properties": {
                                "seq": {"type": "integer"},
                                "state": {"type": "string"},
                                "enabled": {"type": "boolean"},
                                "timestamp": {"type": "string"},
                            },
                            "additionalProperties": False,
                        },
                    }
                ]
            }

            errors = MODULE.validate_rust_event_payloads(root, events_contract)
            self.assertTrue(any("unexpected field(s) not in schema" in err for err in errors))

    def test_validate_rust_event_payloads_scans_multiple_source_files(self) -> None:
        """Regression (13vc, 32y1): validator must scan all .rs files, not just integration.rs."""
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            src_dir = root / "src-tauri" / "src"
            src_dir.mkdir(parents=True, exist_ok=True)

            # integration.rs: defines the event constant
            (src_dir / "integration.rs").write_text(
                "\n".join(
                    [
                        'const EVENT_SIDECAR_STATUS: &str = "sidecar:status";',
                    ]
                ),
                encoding="utf-8",
            )

            # supervisor.rs: emits the event via app_handle.emit
            (src_dir / "supervisor.rs").write_text(
                "\n".join(
                    [
                        'const EVENT_SIDECAR_STATUS: &str = "sidecar:status";',
                        "fn status_payload() -> serde_json::Value {",
                        '  json!({ "state": "running", "restart_count": 0 })',
                        "}",
                        "fn emit_status() {",
                        "  let payload = status_payload();",
                        "  app_handle.emit(EVENT_SIDECAR_STATUS, payload);",
                        "}",
                    ]
                ),
                encoding="utf-8",
            )

            events_contract = {
                "items": [
                    {
                        "type": "event",
                        "name": "sidecar:status",
                        "payload_schema": {
                            "type": "object",
                            "required": ["state", "restart_count"],
                            "properties": {
                                "state": {"type": "string"},
                                "restart_count": {"type": "integer"},
                            },
                            "additionalProperties": True,
                        },
                    }
                ]
            }

            errors = MODULE.validate_rust_event_payloads(root, events_contract)
            self.assertEqual(errors, [])

    def test_validate_rust_event_payloads_infers_shape_from_function_return_struct(self) -> None:
        """Regression (1e3m): infer payload shape for json!(identifier) from helper return struct."""
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            rust_file = root / "src-tauri" / "src" / "integration.rs"
            rust_file.parent.mkdir(parents=True, exist_ok=True)
            rust_file.write_text(
                "\n".join(
                    [
                        "use serde::Serialize;",
                        "use serde_json::{json, Value};",
                        'const EVENT_MODEL_PROGRESS: &str = "model:progress";',
                        "#[derive(Serialize)]",
                        "pub struct ModelProgress {",
                        "  pub model_id: Option<String>,",
                        "  pub current: u64,",
                        "  pub total: Option<u64>,",
                        "  pub unit: String,",
                        "}",
                        "fn model_progress_from_parts(current: u64, total: Option<u64>, unit: Option<String>) -> ModelProgress {",
                        "  ModelProgress {",
                        "    model_id: Some(\"parakeet\".to_string()),",
                        "    current,",
                        "    total,",
                        "    unit: unit.unwrap_or_else(|| \"bytes\".to_string()),",
                        "  }",
                        "}",
                        "fn emit_with_shared_seq<T>(_handle: &T, _events: &[&str], _payload: Value, _seq: &u64) {}",
                        "fn wire(handle: &u8, event_seq: &u64) {",
                        "  let model_progress_data = model_progress_from_parts(1, Some(2), None);",
                        "  emit_with_shared_seq(handle, &[EVENT_MODEL_PROGRESS], json!(model_progress_data), event_seq);",
                        "}",
                    ]
                ),
                encoding="utf-8",
            )

            events_contract = {
                "items": [
                    {
                        "type": "event",
                        "name": "model:progress",
                        "payload_schema": {
                            "type": "object",
                            "required": ["seq", "current", "unit"],
                            "properties": {
                                "seq": {"type": "integer"},
                                "model_id": {"type": ["string", "null"]},
                                "current": {"type": "integer"},
                                "total": {"type": "integer"},
                                "unit": {"type": "string"},
                            },
                            "additionalProperties": True,
                        },
                    }
                ]
            }

            errors = MODULE.validate_rust_event_payloads(root, events_contract)
            self.assertEqual(errors, [])

    def test_validate_tauri_event_payload_examples_reports_schema_violation(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            fixture = root / "src" / "hooks" / "useTauriEvents.test.ts"
            fixture.parent.mkdir(parents=True, exist_ok=True)
            fixture.write_text(
                "\n".join(
                    [
                        "emitMockEvent('state:changed', {",
                        "  seq: 1,",
                        "  state: 'idle',",
                        "  enabled: true,",
                        "  unexpected: true,",
                        "});",
                    ]
                ),
                encoding="utf-8",
            )

            events_contract = {
                "$schema": "http://json-schema.org/draft-07/schema#",
                "items": [
                    {
                        "type": "event",
                        "name": "state:changed",
                        "payload_schema": {
                            "type": "object",
                            "required": ["seq", "state", "enabled"],
                            "properties": {
                                "seq": {"type": "integer"},
                                "state": {"type": "string"},
                                "enabled": {"type": "boolean"},
                            },
                            "additionalProperties": False,
                        },
                    }
                ],
            }

            errors = MODULE.validate_tauri_event_payload_examples(root, events_contract)
            self.assertGreaterEqual(len(errors), 1)
            self.assertTrue(any("unexpected" in err for err in errors))

    def test_validate_sidecar_examples_reports_missing_required_method_fixture(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            examples_path = root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
            self._write_jsonl(
                examples_path,
                [
                    {
                        "type": "request",
                        "data": {"jsonrpc": "2.0", "id": 1, "method": "system.ping", "params": {}},
                    }
                ],
            )

            sidecar_contract = {
                "$schema": "http://json-schema.org/draft-07/schema#",
                "items": [
                    {
                        "type": "method",
                        "name": "status.get",
                        "required": True,
                        "params_schema": {"type": "object", "additionalProperties": False},
                        "result_schema": {"type": "object"},
                    },
                    {
                        "type": "method",
                        "name": "system.ping",
                        "required": False,
                        "params_schema": {"type": "object", "additionalProperties": False},
                        "result_schema": {"type": "object"},
                    },
                ],
            }

            errors = MODULE.validate_sidecar_examples_against_contract(root, sidecar_contract)
            self.assertTrue(any("missing request fixture for required sidecar method 'status.get'" in err for err in errors))

    def test_validate_sidecar_examples_fails_unknown_fixture_method_name(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            examples_path = root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
            self._write_jsonl(
                examples_path,
                [
                    {
                        "type": "request",
                        "data": {"jsonrpc": "2.0", "id": 1, "method": "status.get_typo", "params": {}},
                    },
                    {
                        "type": "notification",
                        "data": {"jsonrpc": "2.0", "method": "status.changed_typo", "params": {}},
                    },
                ],
            )

            sidecar_contract = {
                "$schema": "http://json-schema.org/draft-07/schema#",
                "items": [
                    {
                        "type": "method",
                        "name": "status.get",
                        "required": False,
                        "params_schema": {"type": "object", "additionalProperties": False},
                        "result_schema": {"type": "object"},
                    },
                    {
                        "type": "notification",
                        "name": "status.changed",
                        "params_schema": {"type": "object", "additionalProperties": False},
                    },
                ],
            }

            errors = MODULE.validate_sidecar_examples_against_contract(root, sidecar_contract)
            self.assertTrue(any("unknown request method 'status.get_typo'" in err for err in errors))
            self.assertTrue(any("unknown notification method 'status.changed_typo'" in err for err in errors))

    def test_validate_sidecar_handler_dispatch_accepts_required_methods_present(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            server_file = root / "sidecar" / "src" / "openvoicy_sidecar" / "server.py"
            server_file.parent.mkdir(parents=True, exist_ok=True)
            server_file.write_text(
                "\n".join(
                    [
                        "def handle_ping(request):",
                        "    return {}",
                        "def handle_status(request):",
                        "    return {}",
                        "HANDLERS: dict[str, object] = {",
                        "  'system.ping': handle_ping,",
                        "  'status.get': handle_status,",
                        "}",
                    ]
                ),
                encoding="utf-8",
            )

            sidecar_contract = {
                "items": [
                    {
                        "type": "method",
                        "name": "system.ping",
                        "required": True,
                        "params_schema": {"type": "object"},
                        "result_schema": {"type": "object"},
                    },
                    {
                        "type": "method",
                        "name": "status.get",
                        "required": True,
                        "params_schema": {"type": "object"},
                        "result_schema": {"type": "object"},
                    },
                ]
            }

            errors = MODULE.validate_sidecar_handler_dispatch(root, sidecar_contract)
            self.assertEqual(errors, [])

    def test_validate_sidecar_handler_dispatch_reports_missing_required_method(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            server_file = root / "sidecar" / "src" / "openvoicy_sidecar" / "server.py"
            server_file.parent.mkdir(parents=True, exist_ok=True)
            server_file.write_text(
                "\n".join(
                    [
                        "def handle_ping(request):",
                        "    return {}",
                        "HANDLERS: dict[str, object] = {",
                        "  'system.ping': handle_ping,",
                        "}",
                    ]
                ),
                encoding="utf-8",
            )

            sidecar_contract = {
                "items": [
                    {
                        "type": "method",
                        "name": "system.ping",
                        "required": True,
                        "params_schema": {"type": "object"},
                        "result_schema": {"type": "object"},
                    },
                    {
                        "type": "method",
                        "name": "status.get",
                        "required": True,
                        "params_schema": {"type": "object"},
                        "result_schema": {"type": "object"},
                    },
                ]
            }

            errors = MODULE.validate_sidecar_handler_dispatch(root, sidecar_contract)
            self.assertEqual(len(errors), 1)
            self.assertIn("status.get", errors[0])

    def test_validate_sidecar_handler_dispatch_reports_undeclared_handler_method(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            server_file = root / "sidecar" / "src" / "openvoicy_sidecar" / "server.py"
            server_file.parent.mkdir(parents=True, exist_ok=True)
            server_file.write_text(
                "\n".join(
                    [
                        "def handle_ping(request):",
                        "    return {}",
                        "def handle_shadow(request):",
                        "    return {}",
                        "HANDLERS: dict[str, object] = {",
                        "  'system.ping': handle_ping,",
                        "  'shadow.method': handle_shadow,",
                        "}",
                    ]
                ),
                encoding="utf-8",
            )

            sidecar_contract = {
                "items": [
                    {
                        "type": "method",
                        "name": "system.ping",
                        "required": True,
                        "params_schema": {"type": "object"},
                        "result_schema": {"type": "object"},
                    }
                ]
            }

            errors = MODULE.validate_sidecar_handler_dispatch(root, sidecar_contract)
            self.assertEqual(len(errors), 1)
            self.assertIn("shadow.method", errors[0])

    def test_extract_sidecar_handler_methods_works_without_type_annotation(self) -> None:
        """Regression (3qsj): extraction must not depend on dict[...] type annotation."""
        text_no_annotation = "\n".join(
            [
                "HANDLERS = {",
                "  'system.ping': handle_ping,",
                "  'status.get': handle_status,",
                "}",
            ]
        )
        methods = MODULE.extract_sidecar_handler_methods(text_no_annotation)
        self.assertEqual(methods, {"system.ping", "status.get"})

        text_with_Dict = "\n".join(
            [
                "HANDLERS: Dict[str, Callable] = {",
                "  'system.ping': handle_ping,",
                "}",
            ]
        )
        methods2 = MODULE.extract_sidecar_handler_methods(text_with_Dict)
        self.assertEqual(methods2, {"system.ping"})

    def test_validate_generator_determinism_accepts_stable_output(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            scripts_dir = root / "scripts"
            scripts_dir.mkdir(parents=True)
            (root / "src").mkdir(parents=True)
            (root / "src-tauri" / "src").mkdir(parents=True)

            stable_script = "\n".join(
                [
                    "import argparse",
                    "from pathlib import Path",
                    "p=argparse.ArgumentParser()",
                    "p.add_argument('--repo-root')",
                    "p.add_argument('--out')",
                    "a=p.parse_args()",
                    "Path(a.out).write_text('stable\\n', encoding='utf-8')",
                ]
            )
            (scripts_dir / "gen_contracts_ts.py").write_text(stable_script, encoding="utf-8")
            (scripts_dir / "gen_contracts_rs.py").write_text(stable_script, encoding="utf-8")

            errors = MODULE.validate_generator_determinism(root)
            self.assertEqual(errors, [])

    def test_validate_generator_determinism_detects_non_deterministic_output(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            scripts_dir = root / "scripts"
            scripts_dir.mkdir(parents=True)
            (root / "src").mkdir(parents=True)
            (root / "src-tauri" / "src").mkdir(parents=True)

            noisy_script = "\n".join(
                [
                    "import argparse",
                    "import uuid",
                    "from pathlib import Path",
                    "p=argparse.ArgumentParser()",
                    "p.add_argument('--repo-root')",
                    "p.add_argument('--out')",
                    "a=p.parse_args()",
                    "Path(a.out).write_text(str(uuid.uuid4()) + '\\n', encoding='utf-8')",
                ]
            )
            (scripts_dir / "gen_contracts_ts.py").write_text(noisy_script, encoding="utf-8")
            (scripts_dir / "gen_contracts_rs.py").write_text(noisy_script, encoding="utf-8")

            errors = MODULE.validate_generator_determinism(root)
            self.assertGreaterEqual(len(errors), 1)
            self.assertTrue(any("non-deterministic output across runs" in err for err in errors))

    def test_validate_frontend_listener_events_fails_on_unresolved_dynamic_listen(self) -> None:
        """Regression (irkm): unresolved dynamic listen() must fail, not just warn."""
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            hook_file = root / "src" / "hooks" / "useTauriEvents.ts"
            hook_file.parent.mkdir(parents=True, exist_ok=True)
            hook_file.write_text(
                "\n".join(
                    [
                        "import { listen } from '@tauri-apps/api/event';",
                        "void listen(someVariable, () => {});",
                    ]
                ),
                encoding="utf-8",
            )

            events_contract = {
                "items": [
                    {
                        "type": "event",
                        "name": "state:changed",
                        "payload_schema": {"type": "object"},
                    }
                ]
            }
            errors = MODULE.validate_frontend_listener_events(root, events_contract)
            self.assertEqual(len(errors), 1)
            self.assertIn("could not be resolved statically", errors[0])

    def test_validate_frontend_listener_events_skips_known_passthrough_wrappers(self) -> None:
        """Regression (irkm): known pass-through wrappers must be skipped, not flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            hook_file = root / "src" / "hooks" / "useTauriEvents.ts"
            hook_file.parent.mkdir(parents=True, exist_ok=True)
            hook_file.write_text(
                "\n".join(
                    [
                        "import { listen } from '@tauri-apps/api/event';",
                        "const registerListener = async (eventName: string, onEvent: any) => {",
                        "  const unlisten = await listen(eventName, onEvent);",
                        "};",
                        "await registerListener('state:changed', () => {});",
                    ]
                ),
                encoding="utf-8",
            )

            events_contract = {
                "items": [
                    {
                        "type": "event",
                        "name": "state:changed",
                        "payload_schema": {"type": "object"},
                    }
                ]
            }

            stream = io.StringIO()
            with redirect_stdout(stream):
                errors = MODULE.validate_frontend_listener_events(root, events_contract)
            self.assertEqual(errors, [])
            output = stream.getvalue()
            self.assertIn("pass-through wrapper", output)

    def test_self_test_mode_returns_success(self) -> None:
        self.assertEqual(MODULE.main(["--self-test"]), 0)


if __name__ == "__main__":
    unittest.main()
