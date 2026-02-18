import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "validate_config_schema_parity.py"
SPEC = importlib.util.spec_from_file_location("validate_config_schema_parity", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class ValidateConfigSchemaParityTests(unittest.TestCase):
    @staticmethod
    def _write_minimal_repo(root: Path) -> None:
        (root / "shared" / "schema").mkdir(parents=True)
        (root / "src").mkdir(parents=True)
        (root / "src-tauri" / "src").mkdir(parents=True)

        app_schema = {
            "type": "object",
            "properties": {
                "schema_version": {"type": "integer"},
                "audio": {"type": "object"},
                "hotkeys": {"type": "object"},
                "injection": {"type": "object"},
                "model": {"type": ["object", "null"]},
                "replacements": {"type": "array"},
                "ui": {"type": "object"},
                "history": {"type": "object"},
                "presets": {"type": "object"},
            },
        }
        replacement_schema = {
            "type": "object",
            "properties": {
                "id": {"type": "string"},
                "enabled": {"type": "boolean"},
                "kind": {"type": "string", "enum": ["literal", "regex"]},
                "pattern": {"type": "string"},
                "replacement": {"type": "string"},
                "word_boundary": {"type": "boolean"},
                "case_sensitive": {"type": "boolean"},
                "description": {"type": "string"},
                "origin": {"type": "string"},
            },
        }

        (root / "shared" / "schema" / "AppConfig.schema.json").write_text(
            json.dumps(app_schema)
        )
        (
            root / "shared" / "schema" / "ReplacementRule.schema.json"
        ).write_text(json.dumps(replacement_schema))

        (root / "src" / "types.ts").write_text(
            "\n".join(
                [
                    "export type ReplacementKind = 'literal' | 'regex';",
                    "export interface ReplacementRule {",
                    "  id: string;",
                    "  enabled: boolean;",
                    "  kind: ReplacementKind;",
                    "  pattern: string;",
                    "  replacement: string;",
                    "  word_boundary: boolean;",
                    "  case_sensitive: boolean;",
                    "  description?: string;",
                    "  origin?: string;",
                    "}",
                    "export interface AppConfig {",
                    "  schema_version: number;",
                    "  audio: unknown;",
                    "  hotkeys: unknown;",
                    "  injection: unknown;",
                    "  model: unknown;",
                    "  replacements: ReplacementRule[];",
                    "  ui: unknown;",
                    "  history: unknown;",
                    "  presets: unknown;",
                    "}",
                ]
            )
        )

        (root / "src-tauri" / "src" / "config.rs").write_text(
            "\n".join(
                [
                    "const ROOT_CONFIG_FIELDS: [&str; 9] = [",
                    '    "schema_version",',
                    '    "audio",',
                    '    "hotkeys",',
                    '    "injection",',
                    '    "model",',
                    '    "replacements",',
                    '    "ui",',
                    '    "history",',
                    '    "presets",',
                    "];",
                    "const REPLACEMENT_RULE_FIELDS: [&str; 9] = [",
                    '    "id",',
                    '    "kind",',
                    '    "pattern",',
                    '    "replacement",',
                    '    "enabled",',
                    '    "word_boundary",',
                    '    "case_sensitive",',
                    '    "description",',
                    '    "origin",',
                    "];",
                ]
            )
        )

    def test_validate_config_schema_parity_passes_for_matching_shapes(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            self._write_minimal_repo(root)
            self.assertEqual(MODULE.validate_config_schema_parity(root), [])

    def test_validate_config_schema_parity_detects_ts_field_drift(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            self._write_minimal_repo(root)

            types_path = root / "src" / "types.ts"
            content = types_path.read_text().replace("  presets: unknown;\n", "")
            types_path.write_text(content)

            errors = MODULE.validate_config_schema_parity(root)
            self.assertTrue(any("TypeScript AppConfig parity" in err for err in errors))
            self.assertTrue(any("presets" in err for err in errors))

    def test_validate_config_schema_parity_detects_replacement_kind_drift(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            self._write_minimal_repo(root)

            types_path = root / "src" / "types.ts"
            content = types_path.read_text().replace(
                "export type ReplacementKind = 'literal' | 'regex';",
                "export type ReplacementKind = 'literal';",
            )
            types_path.write_text(content)

            errors = MODULE.validate_config_schema_parity(root)
            self.assertTrue(any("ReplacementKind" in err for err in errors))
            self.assertTrue(any("regex" in err for err in errors))


if __name__ == "__main__":
    unittest.main()
