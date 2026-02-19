import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "validate_ipc_examples.py"
SPEC = importlib.util.spec_from_file_location("validate_ipc_examples", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class ValidateIPCExamplesTests(unittest.TestCase):
    @staticmethod
    def _write_jsonl(path: Path, rows: list[dict]) -> None:
        path.write_text("\n".join(json.dumps(row) for row in rows) + "\n")

    @staticmethod
    def _write_contract(path: Path) -> None:
        path.write_text(
            json.dumps(
                {
                    "items": [
                        {
                            "type": "method",
                            "name": "audio.meter_start",
                            "result_schema": {
                                "type": "object",
                                "required": ["running", "interval_ms"],
                            },
                        },
                        {
                            "type": "method",
                            "name": "recording.stop",
                            "result_schema": {
                                "type": "object",
                                "required": ["audio_duration_ms", "sample_rate", "channels", "session_id"],
                            },
                        },
                    ]
                }
            )
        )

    def test_method_level_contract_shapes_detects_missing_result_fields(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            examples_file = Path(tmpdir) / "examples.jsonl"
            contract_file = Path(tmpdir) / "sidecar.rpc.v1.json"
            self._write_contract(contract_file)
            self._write_jsonl(
                examples_file,
                [
                    {
                        "type": "request",
                        "data": {"jsonrpc": "2.0", "id": 1, "method": "audio.meter_start"},
                    },
                    {
                        "type": "response",
                        "data": {"jsonrpc": "2.0", "id": 1, "result": {"status": "started"}},
                    },
                ],
            )

            errors = MODULE.validate_method_level_contract_shapes(examples_file, contract_file)
            self.assertGreaterEqual(len(errors), 2)
            self.assertTrue(any("audio.meter_start.result: missing required field 'running'" in err for err in errors))
            self.assertTrue(
                any("audio.meter_start.result: missing required field 'interval_ms'" in err for err in errors)
            )

    def test_method_level_contract_shapes_passes_when_fields_present(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            examples_file = Path(tmpdir) / "examples.jsonl"
            contract_file = Path(tmpdir) / "sidecar.rpc.v1.json"
            self._write_contract(contract_file)
            self._write_jsonl(
                examples_file,
                [
                    {
                        "type": "request",
                        "data": {"jsonrpc": "2.0", "id": 17, "method": "recording.stop"},
                    },
                    {
                        "type": "response",
                        "data": {
                            "jsonrpc": "2.0",
                            "id": 17,
                            "result": {
                                "audio_duration_ms": 3250,
                                "sample_rate": 16000,
                                "channels": 1,
                                "session_id": "550e8400-e29b-41d4-a716-446655440000",
                            },
                        },
                    },
                ],
            )

            errors = MODULE.validate_method_level_contract_shapes(examples_file, contract_file)
            self.assertEqual(errors, [])

    def test_method_level_contract_shapes_detects_request_param_schema_drift(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            examples_file = Path(tmpdir) / "examples.jsonl"
            contract_file = Path(tmpdir) / "sidecar.rpc.v1.json"
            contract_file.write_text(
                json.dumps(
                    {
                        "items": [
                            {
                                "type": "method",
                                "name": "recording.stop",
                                "params_schema": {
                                    "type": "object",
                                    "required": ["session_id"],
                                    "properties": {"session_id": {"type": "string", "minLength": 1}},
                                    "additionalProperties": False,
                                },
                                "result_schema": {"type": "object"},
                            }
                        ]
                    }
                )
            )
            self._write_jsonl(
                examples_file,
                [
                    {
                        "type": "request",
                        "data": {
                            "jsonrpc": "2.0",
                            "id": 17,
                            "method": "recording.stop",
                            "params": {"session_id": "", "unexpected": True},
                        },
                    }
                ],
            )

            errors = MODULE.validate_method_level_contract_shapes(examples_file, contract_file)
            self.assertGreaterEqual(len(errors), 2)
            self.assertTrue(any("minLength" in err for err in errors))
            self.assertTrue(any("unexpected field 'unexpected'" in err for err in errors))

    def test_method_level_contract_shapes_detects_result_enum_drift(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            examples_file = Path(tmpdir) / "examples.jsonl"
            contract_file = Path(tmpdir) / "sidecar.rpc.v1.json"
            contract_file.write_text(
                json.dumps(
                    {
                        "items": [
                            {
                                "type": "method",
                                "name": "model.get_status",
                                "params_schema": {"type": "object", "additionalProperties": False},
                                "result_schema": {
                                    "type": "object",
                                    "required": ["status"],
                                    "properties": {"status": {"type": "string", "enum": ["missing", "ready"]}},
                                    "additionalProperties": True,
                                },
                            }
                        ]
                    }
                )
            )
            self._write_jsonl(
                examples_file,
                [
                    {
                        "type": "request",
                        "data": {"jsonrpc": "2.0", "id": 9, "method": "model.get_status"},
                    },
                    {
                        "type": "response",
                        "data": {"jsonrpc": "2.0", "id": 9, "result": {"status": "downloading"}},
                    },
                ],
            )

            errors = MODULE.validate_method_level_contract_shapes(examples_file, contract_file)
            self.assertEqual(len(errors), 1)
            self.assertIn("expected one of", errors[0])
            self.assertIn("downloading", errors[0])

    def test_status_get_idle_fixture_variants_requires_no_model_example(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            examples_file = Path(tmpdir) / "examples.jsonl"
            self._write_jsonl(
                examples_file,
                [
                    {
                        "type": "request",
                        "data": {"jsonrpc": "2.0", "id": 20, "method": "status.get"},
                    },
                    {
                        "type": "response",
                        "data": {
                            "jsonrpc": "2.0",
                            "id": 20,
                            "result": {
                                "state": "idle",
                                "model": {"model_id": "parakeet-tdt-0.6b-v3", "status": "ready"},
                            },
                        },
                    },
                ],
            )

            errors = MODULE.validate_status_get_idle_fixture_variants(examples_file)
            self.assertEqual(len(errors), 1)
            self.assertIn("without model object", errors[0])

    def test_status_get_idle_fixture_variants_passes_with_both_examples(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            examples_file = Path(tmpdir) / "examples.jsonl"
            self._write_jsonl(
                examples_file,
                [
                    {
                        "type": "request",
                        "data": {"jsonrpc": "2.0", "id": 20, "method": "status.get"},
                    },
                    {
                        "type": "response",
                        "data": {
                            "jsonrpc": "2.0",
                            "id": 20,
                            "result": {
                                "state": "idle",
                                "model": {"model_id": "parakeet-tdt-0.6b-v3", "status": "ready"},
                            },
                        },
                    },
                    {
                        "type": "request",
                        "data": {"jsonrpc": "2.0", "id": 34, "method": "status.get"},
                    },
                    {
                        "type": "response",
                        "data": {
                            "jsonrpc": "2.0",
                            "id": 34,
                            "result": {"state": "idle"},
                        },
                    },
                ],
            )

            errors = MODULE.validate_status_get_idle_fixture_variants(examples_file)
            self.assertEqual(errors, [])

    def test_contract_method_coverage_reports_missing_required_fixture(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            examples_file = Path(tmpdir) / "examples.jsonl"
            contract_file = Path(tmpdir) / "sidecar.rpc.v1.json"
            contract_file.write_text(
                json.dumps(
                    {
                        "items": [
                            {"type": "method", "name": "status.get", "required": True},
                            {"type": "method", "name": "system.ping", "required": False},
                        ]
                    }
                )
            )
            self._write_jsonl(
                examples_file,
                [
                    {
                        "type": "request",
                        "data": {"jsonrpc": "2.0", "id": 1, "method": "system.ping"},
                    }
                ],
            )

            errors, covered, total = MODULE.validate_contract_method_coverage(examples_file, contract_file)
            self.assertEqual((covered, total), (1, 2))
            self.assertEqual(len(errors), 1)
            self.assertIn("status.get", errors[0])

    def test_validate_example_request_uses_dynamic_contract_method_set(self) -> None:
        obj = {
            "_comment": "custom request",
            "type": "request",
            "data": {"jsonrpc": "2.0", "id": 1, "method": "custom.method"},
        }
        errors = MODULE.validate_example(
            obj,
            1,
            request_methods={"custom.method"},
            notification_methods=set(),
        )
        self.assertEqual(errors, [])

    def test_detect_duplicate_fixture_corpora_warns_for_duplicate_copy(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            canonical = root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
            canonical.parent.mkdir(parents=True, exist_ok=True)
            canonical.write_text('{"type":"request","data":{"method":"status.get"}}\n', encoding="utf-8")

            derived = root / "shared" / "contracts" / "examples" / "IPC_V1_EXAMPLES.jsonl"
            derived.parent.mkdir(parents=True, exist_ok=True)
            derived.write_text(canonical.read_text(encoding="utf-8"), encoding="utf-8")

            warnings, errors = MODULE.detect_duplicate_fixture_corpora(root)
            self.assertEqual(errors, [])
            self.assertEqual(len(warnings), 1)
            self.assertIn("Duplicate fixture corpus detected", warnings[0])

    def test_detect_duplicate_fixture_corpora_errors_on_conflict(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            canonical = root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
            canonical.parent.mkdir(parents=True, exist_ok=True)
            canonical.write_text('{"type":"request","data":{"method":"status.get"}}\n', encoding="utf-8")

            derived = root / "shared" / "contracts" / "examples" / "IPC_V1_EXAMPLES.jsonl"
            derived.parent.mkdir(parents=True, exist_ok=True)
            derived.write_text('{"type":"request","data":{"method":"status.get_typo"}}\n', encoding="utf-8")

            warnings, errors = MODULE.detect_duplicate_fixture_corpora(root)
            self.assertEqual(len(warnings), 1)
            self.assertEqual(len(errors), 1)
            self.assertIn("Conflicting fixture corpus detected", errors[0])


if __name__ == "__main__":
    unittest.main()
