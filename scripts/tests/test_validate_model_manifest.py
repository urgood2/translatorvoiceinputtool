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
    @staticmethod
    def _minimal_manifest() -> dict:
        return {
            "schema_version": "1",
            "model_id": "parakeet-tdt-0.6b-v3",
            "source": "nvidia/parakeet-tdt-0.6b-v3",
            "revision": "6d590f77001d318fb17a0b5bf7ee329a91b52598",
            "license": {"spdx_id": "CC-BY-4.0", "redistribution_allowed": True},
            "files": [
                {
                    "path": "parakeet-tdt-0.6b-v3.nemo",
                    "size_bytes": 2509332480,
                    "sha256": "cf4679f1a52ce7400b7b394b2e008b95b7a9f6e209a02ecdde2b28ab9e1bb079",
                }
            ],
            "total_size_bytes": 2509332480,
            "verification": {"sha256_verified": True},
        }

    def test_validate_manifest_schema_rejects_non_digest_sha(self) -> None:
        manifest = self._minimal_manifest()
        manifest["files"][0]["sha256"] = "VERIFY_ON_FIRST_DOWNLOAD"
        errors = MODULE.validate_manifest_schema(manifest)
        self.assertTrue(any("64-character lowercase hex digest" in err for err in errors))

    def test_validate_manifest_schema_requires_sha256_verified_true(self) -> None:
        manifest = self._minimal_manifest()
        manifest["verification"]["sha256_verified"] = False
        errors = MODULE.validate_manifest_schema(manifest)
        self.assertTrue(any("verification.sha256_verified must be true" in err for err in errors))

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
