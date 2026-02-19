import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "validate_contracts.py"
SPEC = importlib.util.spec_from_file_location("validate_contracts", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class ValidateContractsTests(unittest.TestCase):
    def test_extract_listen_event_names_from_text_resolves_constants(self) -> None:
        text = """
const EVENTS = {
  STATE_CHANGED: 'state:changed',
  SIDECAR_STATUS: 'sidecar:status',
} as const;

listen(EVENTS.STATE_CHANGED, () => {});
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

    def test_self_test_mode_returns_success(self) -> None:
        self.assertEqual(MODULE.main(["--self-test"]), 0)


if __name__ == "__main__":
    unittest.main()
