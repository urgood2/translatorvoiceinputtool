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
            "model_family": "parakeet",
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

    def test_validate_manifest_schema_rejects_non_positive_file_size(self) -> None:
        manifest = self._minimal_manifest()
        manifest["files"][0]["size_bytes"] = 0
        errors = MODULE.validate_manifest_schema(manifest)
        self.assertTrue(any("size_bytes must be positive" in err for err in errors))

    def test_validate_manifest_schema_requires_model_family(self) -> None:
        manifest = self._minimal_manifest()
        manifest.pop("model_family")
        errors = MODULE.validate_manifest_schema(manifest)
        self.assertTrue(any("Missing required field: model_family" in err for err in errors))

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

    def test_validate_document_against_schema_catalog_passes(self) -> None:
        schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelCatalog.schema.json"
        schema = json.loads(schema_path.read_text())
        catalog = {
            "schema_version": 1,
            "models": [
                {
                    "model_id": "nvidia/parakeet-tdt-0.6b-v3",
                    "family": "parakeet",
                    "display_name": "NVIDIA Parakeet TDT 0.6B v3",
                    "description": "Test model",
                    "supported_languages": ["en"],
                    "default_language": "en",
                    "size_bytes": 1,
                    "manifest_path": "manifests/parakeet-tdt-0.6b-v3.json",
                }
            ],
        }
        errors = MODULE.validate_document_against_schema(catalog, schema, "MODEL_CATALOG.json")
        self.assertEqual(errors, [])

    def test_validate_catalog_manifest_paths_requires_existing_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            model_root = Path(tmpdir)
            catalog = {
                "schema_version": 1,
                "models": [
                    {
                        "model_id": "nvidia/parakeet-tdt-0.6b-v3",
                        "manifest_path": "manifests/parakeet-tdt-0.6b-v3.json",
                    }
                ],
            }
            errors, _ = MODULE.validate_catalog_manifest_paths(catalog, model_root)
            self.assertTrue(any("does not resolve" in err for err in errors))

    def test_validate_catalog_manifest_paths_detects_model_id_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            model_root = Path(tmpdir)
            manifests_dir = model_root / "manifests"
            manifests_dir.mkdir(parents=True)
            manifest_path = manifests_dir / "parakeet-tdt-0.6b-v3.json"
            manifest_path.write_text(
                json.dumps(
                    {
                        "model_id": "openai/whisper-base",
                        "version": "1.0",
                        "files": [
                            {
                                "path": "a.bin",
                                "urls": ["https://example.com/a.bin"],
                                "size_bytes": 1,
                                "sha256": "a" * 64,
                            }
                        ],
                    }
                )
            )
            catalog = {
                "schema_version": 1,
                "models": [
                    {
                        "model_id": "nvidia/parakeet-tdt-0.6b-v3",
                        "manifest_path": "manifests/parakeet-tdt-0.6b-v3.json",
                    }
                ],
            }
            errors, _ = MODULE.validate_catalog_manifest_paths(catalog, model_root)
            self.assertTrue(any("does not match" in err for err in errors))

    def test_validate_catalog_unique_model_ids_detects_duplicates(self) -> None:
        catalog = {
            "schema_version": 1,
            "models": [
                {"model_id": "nvidia/parakeet-tdt-0.6b-v3"},
                {"model_id": "openai/whisper-base"},
                {"model_id": "nvidia/parakeet-tdt-0.6b-v3"},
            ],
        }

        errors = MODULE.validate_catalog_unique_model_ids(catalog)
        self.assertEqual(len(errors), 1)
        self.assertIn("duplicate model_id 'nvidia/parakeet-tdt-0.6b-v3'", errors[0])

    def test_validate_catalog_unique_model_ids_passes_for_unique_entries(self) -> None:
        catalog = {
            "schema_version": 1,
            "models": [
                {"model_id": "nvidia/parakeet-tdt-0.6b-v3"},
                {"model_id": "openai/whisper-base"},
            ],
        }

        errors = MODULE.validate_catalog_unique_model_ids(catalog)
        self.assertEqual(errors, [])

    def test_validate_manifests_directory_validates_sha_format(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            repo_root = Path(tmpdir)
            manifests_dir = repo_root / "shared" / "model" / "manifests"
            manifests_dir.mkdir(parents=True)
            bad_manifest = manifests_dir / "bad.json"
            bad_manifest.write_text(
                json.dumps(
                    {
                        "model_id": "nvidia/parakeet-tdt-0.6b-v3",
                        "version": "1.0",
                        "files": [
                            {
                                "path": "a.bin",
                                "urls": ["https://example.com/a.bin"],
                                "size_bytes": 1,
                                "sha256": "BAD_SHA",
                            }
                        ],
                    }
                )
            )
            schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelManifest.schema.json"
            schema = json.loads(schema_path.read_text())

            errors, _ = MODULE.validate_manifests_directory(manifests_dir, schema, repo_root)
            self.assertTrue(any("sha256" in err for err in errors))

    # ── Catalog schema validation ──────────────────────────────────

    def test_catalog_schema_rejects_missing_required_field(self) -> None:
        """Missing required field in catalog entry -> validation failure."""
        schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelCatalog.schema.json"
        schema = json.loads(schema_path.read_text())
        catalog = {
            "schema_version": 1,
            "models": [
                {
                    "model_id": "nvidia/parakeet-tdt-0.6b-v3",
                    # missing family, display_name, description, etc.
                }
            ],
        }
        errors = MODULE.validate_document_against_schema(catalog, schema, "catalog")
        self.assertTrue(len(errors) > 0)
        self.assertTrue(any("required" in err.lower() for err in errors))

    def test_catalog_schema_rejects_empty_supported_languages(self) -> None:
        """supported_languages must have at least one entry."""
        schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelCatalog.schema.json"
        schema = json.loads(schema_path.read_text())
        catalog = {
            "schema_version": 1,
            "models": [
                {
                    "model_id": "nvidia/parakeet-tdt-0.6b-v3",
                    "family": "parakeet",
                    "display_name": "Test",
                    "description": "Test",
                    "supported_languages": [],
                    "default_language": "en",
                    "size_bytes": 1,
                    "manifest_path": "manifests/test.json",
                }
            ],
        }
        errors = MODULE.validate_document_against_schema(catalog, schema, "catalog")
        self.assertTrue(any("minItems" in err or "too short" in err for err in errors))

    def test_catalog_schema_rejects_unknown_family_enum(self) -> None:
        """family must stay aligned with supported backend allowlist."""
        schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelCatalog.schema.json"
        schema = json.loads(schema_path.read_text())
        catalog = {
            "schema_version": 1,
            "models": [
                {
                    "model_id": "nvidia/parakeet-tdt-0.6b-v3",
                    "family": "canary",
                    "display_name": "Test",
                    "description": "Test",
                    "supported_languages": ["en"],
                    "default_language": "en",
                    "size_bytes": 1,
                    "manifest_path": "manifests/test.json",
                }
            ],
        }
        errors = MODULE.validate_document_against_schema(catalog, schema, "catalog")
        self.assertTrue(
            any("is not one of" in err or "enum" in err.lower() for err in errors),
            f"expected enum validation error, got: {errors}",
        )

    def test_catalog_schema_rejects_unsupported_family(self) -> None:
        """family must stay aligned with supported backend allowlist."""
        schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelCatalog.schema.json"
        schema = json.loads(schema_path.read_text())
        catalog = {
            "schema_version": 1,
            "models": [
                {
                    "model_id": "nvidia/parakeet-tdt-0.6b-v3",
                    "family": "canary",
                    "display_name": "Test",
                    "description": "Test",
                    "supported_languages": ["en"],
                    "default_language": "en",
                    "size_bytes": 1,
                    "manifest_path": "manifests/test.json",
                }
            ],
        }
        errors = MODULE.validate_document_against_schema(catalog, schema, "catalog")
        self.assertTrue(any("is not one of" in err for err in errors))

    def test_catalog_schema_accepts_whisper_family(self) -> None:
        """whisper remains a valid family enum value."""
        schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelCatalog.schema.json"
        schema = json.loads(schema_path.read_text())
        catalog = {
            "schema_version": 1,
            "models": [
                {
                    "model_id": "openai/whisper-base",
                    "family": "whisper",
                    "display_name": "Whisper Base",
                    "description": "Test whisper model",
                    "supported_languages": ["en"],
                    "default_language": "en",
                    "size_bytes": 1,
                    "manifest_path": "manifests/whisper-base.json",
                }
            ],
        }
        errors = MODULE.validate_document_against_schema(catalog, schema, "catalog")
        self.assertEqual(errors, [])

    # ── Per-model manifest schema validation ───────────────────────

    def test_manifest_schema_validates_valid_manifest(self) -> None:
        """Valid per-model manifest validates successfully."""
        schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelManifest.schema.json"
        schema = json.loads(schema_path.read_text())
        manifest = {
            "model_id": "nvidia/parakeet-tdt-0.6b-v3",
            "model_family": "parakeet",
            "version": "3.0",
            "revision": "6d590f77001d318fb17a0b5bf7ee329a91b52598",
            "files": [
                {
                    "path": "model.nemo",
                    "urls": ["https://example.com/model.nemo"],
                    "size_bytes": 2509332480,
                    "sha256": "cf4679f1a52ce7400b7b394b2e008b95b7a9f6e209a02ecdde2b28ab9e1bb079",
                }
            ],
        }
        errors = MODULE.validate_document_against_schema(manifest, schema, "manifest")
        self.assertEqual(errors, [])

    def test_manifest_schema_rejects_http_urls(self) -> None:
        """Manifest urls must use https://."""
        schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelManifest.schema.json"
        schema = json.loads(schema_path.read_text())
        manifest = {
            "model_id": "nvidia/parakeet-tdt-0.6b-v3",
            "model_family": "parakeet",
            "version": "1.0",
            "files": [
                {
                    "path": "model.bin",
                    "urls": ["http://insecure.example.com/model.bin"],
                    "size_bytes": 100,
                    "sha256": "a" * 64,
                }
            ],
        }
        errors = MODULE.validate_document_against_schema(manifest, schema, "manifest")
        self.assertTrue(len(errors) > 0)

    def test_manifest_schema_rejects_missing_files(self) -> None:
        """Manifest must have files array."""
        schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelManifest.schema.json"
        schema = json.loads(schema_path.read_text())
        manifest = {
            "model_id": "nvidia/parakeet-tdt-0.6b-v3",
            "model_family": "parakeet",
            "version": "1.0",
        }
        errors = MODULE.validate_document_against_schema(manifest, schema, "manifest")
        self.assertTrue(any("files" in err for err in errors))

    def test_manifest_schema_requires_model_family(self) -> None:
        """model_family is required for backend dispatch alignment."""
        schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelManifest.schema.json"
        schema = json.loads(schema_path.read_text())
        manifest = {
            "model_id": "nvidia/parakeet-tdt-0.6b-v3",
            "version": "1.0",
            "files": [
                {
                    "path": "model.bin",
                    "urls": ["https://example.com/model.bin"],
                    "size_bytes": 100,
                    "sha256": "a" * 64,
                }
            ],
        }
        errors = MODULE.validate_document_against_schema(manifest, schema, "manifest")
        self.assertTrue(any("model_family" in err and "required" in err for err in errors))

    def test_manifest_schema_rejects_non_positive_size_bytes(self) -> None:
        """size_bytes must be strictly positive."""
        schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelManifest.schema.json"
        schema = json.loads(schema_path.read_text())
        manifest = {
            "model_id": "nvidia/parakeet-tdt-0.6b-v3",
            "model_family": "parakeet",
            "version": "1.0",
            "files": [
                {
                    "path": "model.bin",
                    "urls": ["https://example.com/model.bin"],
                    "size_bytes": 0,
                    "sha256": "a" * 64,
                }
            ],
        }
        errors = MODULE.validate_document_against_schema(manifest, schema, "manifest")
        self.assertTrue(any("size_bytes" in err for err in errors))

    # ── Manifests directory validation ─────────────────────────────

    def test_manifests_directory_validates_valid_manifest(self) -> None:
        """Valid manifest in manifests/ passes validation."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo_root = Path(tmpdir)
            manifests_dir = repo_root / "shared" / "model" / "manifests"
            manifests_dir.mkdir(parents=True)
            (manifests_dir / "good.json").write_text(
                json.dumps(
                    {
                        "model_id": "nvidia/parakeet-tdt-0.6b-v3",
                        "model_family": "parakeet",
                        "version": "1.0",
                        "files": [
                            {
                                "path": "model.bin",
                                "urls": ["https://example.com/model.bin"],
                                "size_bytes": 100,
                                "sha256": "a" * 64,
                            }
                        ],
                    }
                )
            )
            schema_path = Path(__file__).resolve().parents[2] / "shared" / "schema" / "ModelManifest.schema.json"
            schema = json.loads(schema_path.read_text())

            errors, docs = MODULE.validate_manifests_directory(manifests_dir, schema, repo_root)
            self.assertEqual(errors, [])
            self.assertEqual(len(docs), 1)

    def test_validate_real_catalog_against_schema(self) -> None:
        """The actual MODEL_CATALOG.json validates against schema."""
        repo_root = Path(__file__).resolve().parents[2]
        schema_path = repo_root / "shared" / "schema" / "ModelCatalog.schema.json"
        catalog_path = repo_root / "shared" / "model" / "MODEL_CATALOG.json"

        schema = json.loads(schema_path.read_text())
        catalog = json.loads(catalog_path.read_text())

        errors = MODULE.validate_document_against_schema(catalog, schema, "MODEL_CATALOG.json")
        self.assertEqual(errors, [])

    def test_validate_real_manifests_against_schema(self) -> None:
        """All manifests in manifests/ validate against schema."""
        repo_root = Path(__file__).resolve().parents[2]
        schema_path = repo_root / "shared" / "schema" / "ModelManifest.schema.json"
        manifests_dir = repo_root / "shared" / "model" / "manifests"

        schema = json.loads(schema_path.read_text())

        errors, docs = MODULE.validate_manifests_directory(manifests_dir, schema, repo_root)
        self.assertEqual(errors, [])
        self.assertGreater(len(docs), 0)


if __name__ == "__main__":
    unittest.main()
