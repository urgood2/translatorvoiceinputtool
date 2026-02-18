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

    def test_contract_required_result_fields_detects_missing_fields(self) -> None:
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

            errors = MODULE.validate_contract_required_result_fields(examples_file, contract_file)
            self.assertEqual(len(errors), 1)
            self.assertIn("audio.meter_start", errors[0])
            self.assertIn("interval_ms", errors[0])
            self.assertIn("running", errors[0])

    def test_contract_required_result_fields_passes_when_fields_present(self) -> None:
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

            errors = MODULE.validate_contract_required_result_fields(examples_file, contract_file)
            self.assertEqual(errors, [])


if __name__ == "__main__":
    unittest.main()
