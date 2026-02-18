import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "validate_model_manifest.py"
SPEC = importlib.util.spec_from_file_location("validate_model_manifest", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class ValidateModelManifestTests(unittest.TestCase):
    def test_validate_ipc_model_ids_fails_on_mismatch(self) -> None:
        manifest = {"model_id": "parakeet-tdt-0.6b-v3"}
        with tempfile.TemporaryDirectory() as tmpdir:
            examples_file = Path(tmpdir) / "IPC_V1_EXAMPLES.jsonl"
            examples_file.write_text(
                json.dumps(
                    {
                        "type": "request",
                        "data": {
                            "jsonrpc": "2.0",
                            "id": 1,
                            "method": "asr.initialize",
                            "params": {"model_id": "parakeet-tdt-0.6b-v2"},
                        },
                    }
                )
            )
            errors = MODULE.validate_ipc_model_ids(manifest, examples_file)
            self.assertEqual(len(errors), 1)
            self.assertIn("parakeet-tdt-0.6b-v2", errors[0])

    def test_validate_ipc_model_ids_passes_on_match(self) -> None:
        manifest = {"model_id": "parakeet-tdt-0.6b-v3"}
        with tempfile.TemporaryDirectory() as tmpdir:
            examples_file = Path(tmpdir) / "IPC_V1_EXAMPLES.jsonl"
            examples_file.write_text(
                json.dumps(
                    {
                        "type": "request",
                        "data": {
                            "jsonrpc": "2.0",
                            "id": 2,
                            "method": "asr.initialize",
                            "params": {"model_id": "parakeet-tdt-0.6b-v3"},
                        },
                    }
                )
            )
            self.assertEqual(MODULE.validate_ipc_model_ids(manifest, examples_file), [])

    def test_validate_rust_model_defaults_requires_manifest_default_call(self) -> None:
        manifest = {"model_id": "parakeet-tdt-0.6b-v3"}
        with tempfile.TemporaryDirectory() as tmpdir:
            repo_root = Path(tmpdir)
            src_dir = repo_root / "src-tauri" / "src"
            src_dir.mkdir(parents=True)
            (src_dir / "integration.rs").write_text("fn configured_model_id() -> String { \"x\".to_string() }\n")
            (src_dir / "commands.rs").write_text(
                "pub fn get_model_status() -> String { model_defaults::default_model_id().to_string() }\n"
            )
            (src_dir / "model_defaults.rs").write_text(
                'const DEFAULT_MODEL_ID: &str = "parakeet-tdt-0.6b-v3";\n'
                'const MANIFEST_STR: &str = include_str!("../../shared/model/MODEL_MANIFEST.json");\n'
            )

            errors = MODULE.validate_rust_model_defaults(manifest, repo_root)
            self.assertTrue(any("integration.rs" in err for err in errors))

    def test_validate_rust_model_defaults_rejects_drifted_literal(self) -> None:
        manifest = {"model_id": "parakeet-tdt-0.6b-v3"}
        with tempfile.TemporaryDirectory() as tmpdir:
            repo_root = Path(tmpdir)
            src_dir = repo_root / "src-tauri" / "src"
            src_dir.mkdir(parents=True)
            (src_dir / "integration.rs").write_text(
                'fn configured_model_id() -> String { "parakeet-tdt-0.6b-v2".to_string() }\n'
                "fn defaulted() { let _ = model_defaults::default_model_id(); }\n"
            )
            (src_dir / "commands.rs").write_text(
                "pub fn get_model_status() { let _ = model_defaults::default_model_id(); }\n"
            )
            (src_dir / "model_defaults.rs").write_text(
                'const DEFAULT_MODEL_ID: &str = "parakeet-tdt-0.6b-v3";\n'
                'const MANIFEST_STR: &str = include_str!("../../shared/model/MODEL_MANIFEST.json");\n'
            )

            errors = MODULE.validate_rust_model_defaults(manifest, repo_root)
            self.assertTrue(any("v2" in err for err in errors))

    def test_validate_rust_model_defaults_passes_with_manifest_wiring(self) -> None:
        manifest = {"model_id": "parakeet-tdt-0.6b-v3"}
        with tempfile.TemporaryDirectory() as tmpdir:
            repo_root = Path(tmpdir)
            src_dir = repo_root / "src-tauri" / "src"
            src_dir.mkdir(parents=True)
            (src_dir / "integration.rs").write_text(
                "fn configured_model_id() -> String {\n"
                "    model_defaults::default_model_id().to_string()\n"
                "}\n"
            )
            (src_dir / "commands.rs").write_text(
                "pub fn get_model_status() -> String {\n"
                "    model_defaults::default_model_id().to_string()\n"
                "}\n"
            )
            (src_dir / "model_defaults.rs").write_text(
                'const DEFAULT_MODEL_ID: &str = "parakeet-tdt-0.6b-v3";\n'
                'const MANIFEST_STR: &str = include_str!("../../shared/model/MODEL_MANIFEST.json");\n'
            )

            self.assertEqual(MODULE.validate_rust_model_defaults(manifest, repo_root), [])


if __name__ == "__main__":
    unittest.main()
