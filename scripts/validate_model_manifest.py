#!/usr/bin/env python3
"""
Validate MODEL_MANIFEST.json, MODEL_CATALOG.json, and individual manifests.

This script validates:
1. Legacy MODEL_MANIFEST.json parses and has required schema fields
2. New MODEL_CATALOG.json parses and validates against schema
3. Each manifest in manifests/ validates against schema
4. Catalog manifest_path references resolve to existing files
5. SHA256 format validation across all manifests
6. Cross-references with IPC examples and runtime defaults

Exit codes:
  0 - All validations passed
  1 - Validation errors found
"""

import json
import re
import sys
from pathlib import Path
from typing import Any

try:
    import jsonschema
    from jsonschema import Draft7Validator
except ImportError:
    print(
        "ERROR: jsonschema dependency is required. Install with 'pip install jsonschema'",
        file=sys.stderr,
    )
    sys.exit(1)

# Required top-level fields in legacy manifest
REQUIRED_MANIFEST_FIELDS = {
    "schema_version",
    "model_id",
    "model_family",
    "source",
    "revision",
    "license",
    "files",
    "total_size_bytes",
}

# Required fields in license object
REQUIRED_LICENSE_FIELDS = {
    "spdx_id",
    "redistribution_allowed",
}

# Required fields in legacy file objects
REQUIRED_FILE_FIELDS = {
    "path",
    "size_bytes",
    "sha256",
}

SHA256_PATTERN = re.compile(r"^[a-f0-9]{64}$")
SUPPORTED_MODEL_FAMILIES = {"parakeet", "whisper"}

MANIFEST_MODEL_INCLUDE_SNIPPET = 'include_str!("../../shared/model/MODEL_MANIFEST.json")'
DEFAULT_MODEL_CALL_SNIPPET = "model_defaults::default_model_id()"


def load_json_file(path: Path, label: str) -> tuple[dict[str, Any] | None, list[str]]:
    """Load a JSON object from file."""
    if not path.exists():
        return None, [f"{label} not found: {path}"]

    try:
        payload = json.loads(path.read_text())
    except json.JSONDecodeError as exc:
        return None, [f"{label} failed to parse: {exc}"]

    if not isinstance(payload, dict):
        return None, [f"{label} must be a JSON object"]

    return payload, []


def validate_document_against_schema(
    document: dict[str, Any], schema: dict[str, Any], label: str
) -> list[str]:
    """Validate document against a JSON schema (draft-07)."""
    errors: list[str] = []

    try:
        Draft7Validator.check_schema(schema)
    except jsonschema.SchemaError as exc:
        return [f"{label}: schema invalid: {exc.message}"]

    validator = Draft7Validator(schema)
    for err in sorted(validator.iter_errors(document), key=lambda e: list(e.absolute_path)):
        path = ".".join(str(p) for p in err.absolute_path) or "(root)"
        errors.append(f"{label}: {path}: {err.message}")

    return errors


def validate_sha256_fields(manifest: dict[str, Any], label: str) -> list[str]:
    """Validate sha256 field format for all files in a manifest-like payload."""
    errors: list[str] = []
    files = manifest.get("files")
    if not isinstance(files, list):
        return errors

    for i, file_obj in enumerate(files):
        if not isinstance(file_obj, dict):
            continue
        sha256 = file_obj.get("sha256")
        if not isinstance(sha256, str):
            errors.append(f"{label}: files[{i}].sha256 must be a string")
        elif not SHA256_PATTERN.fullmatch(sha256):
            errors.append(
                f"{label}: files[{i}].sha256 must be a 64-character lowercase hex digest"
            )

    return errors


def validate_manifest_schema(manifest: dict[str, Any]) -> list[str]:
    """Validate legacy MODEL_MANIFEST.json required fields and constraints."""
    errors = []

    # Check top-level required fields
    for field in REQUIRED_MANIFEST_FIELDS:
        if field not in manifest:
            errors.append(f"Missing required field: {field}")

    # Check license fields
    if "license" in manifest:
        license_obj = manifest["license"]
        if not isinstance(license_obj, dict):
            errors.append("license must be an object")
        else:
            for field in REQUIRED_LICENSE_FIELDS:
                if field not in license_obj:
                    errors.append(f"Missing required license field: {field}")

    # Check files array
    if "files" in manifest:
        if not isinstance(manifest["files"], list):
            errors.append("files must be an array")
        else:
            for i, file_obj in enumerate(manifest["files"]):
                if not isinstance(file_obj, dict):
                    errors.append(f"files[{i}] must be an object")
                    continue
                for field in REQUIRED_FILE_FIELDS:
                    if field not in file_obj:
                        errors.append(f"files[{i}] missing required field: {field}")
                size_bytes = file_obj.get("size_bytes")
                if not isinstance(size_bytes, int):
                    errors.append(f"files[{i}].size_bytes must be an integer")
                elif size_bytes <= 0:
                    errors.append(f"files[{i}].size_bytes must be positive")
                sha256 = file_obj.get("sha256")
                if not isinstance(sha256, str):
                    errors.append(f"files[{i}].sha256 must be a string")
                elif not SHA256_PATTERN.fullmatch(sha256):
                    errors.append(
                        f"files[{i}].sha256 must be a 64-character lowercase hex digest"
                    )

    if "verification" in manifest:
        verification = manifest["verification"]
        if not isinstance(verification, dict):
            errors.append("verification must be an object")
        elif verification.get("sha256_verified") is not True:
            errors.append("verification.sha256_verified must be true")

    # Validate types
    if "schema_version" in manifest and not isinstance(manifest["schema_version"], str):
        errors.append("schema_version must be a string")

    if "model_id" in manifest:
        if not isinstance(manifest["model_id"], str):
            errors.append("model_id must be a string")
        elif not manifest["model_id"]:
            errors.append("model_id cannot be empty")

    if "model_family" in manifest:
        model_family = manifest["model_family"]
        if not isinstance(model_family, str):
            errors.append("model_family must be a string")
        elif not model_family:
            errors.append("model_family cannot be empty")
        elif model_family not in SUPPORTED_MODEL_FAMILIES:
            allowed = ", ".join(sorted(SUPPORTED_MODEL_FAMILIES))
            errors.append(f"model_family must be one of: {allowed}")

    if "revision" in manifest:
        if not isinstance(manifest["revision"], str):
            errors.append("revision must be a string")
        elif not manifest["revision"]:
            errors.append("revision cannot be empty")

    if "total_size_bytes" in manifest:
        if not isinstance(manifest["total_size_bytes"], int):
            errors.append("total_size_bytes must be an integer")
        elif manifest["total_size_bytes"] < 0:
            errors.append("total_size_bytes cannot be negative")

    return errors


def validate_catalog_manifest_paths(
    catalog: dict[str, Any], model_root: Path
) -> tuple[list[str], dict[Path, dict[str, Any]]]:
    """Validate that catalog manifest_path values resolve to existing manifest files."""
    errors: list[str] = []
    resolved_docs: dict[Path, dict[str, Any]] = {}

    models = catalog.get("models")
    if not isinstance(models, list):
        return ["MODEL_CATALOG.json: models must be an array"], resolved_docs

    for i, model in enumerate(models):
        if not isinstance(model, dict):
            errors.append(f"MODEL_CATALOG.json: models[{i}] must be an object")
            continue

        manifest_path = model.get("manifest_path")
        if not isinstance(manifest_path, str) or not manifest_path:
            errors.append(f"MODEL_CATALOG.json: models[{i}].manifest_path must be a non-empty string")
            continue

        resolved = (model_root / manifest_path).resolve()
        if not resolved.exists():
            errors.append(
                f"MODEL_CATALOG.json: models[{i}].manifest_path does not resolve: {manifest_path}"
            )
            continue
        if not resolved.is_file():
            errors.append(
                f"MODEL_CATALOG.json: models[{i}].manifest_path is not a file: {manifest_path}"
            )
            continue

        parsed, parse_errors = load_json_file(resolved, f"manifest {manifest_path}")
        if parse_errors:
            errors.extend(parse_errors)
            continue
        assert parsed is not None
        resolved_docs[resolved] = parsed

        model_id = model.get("model_id")
        manifest_model_id = parsed.get("model_id")
        if isinstance(model_id, str) and isinstance(manifest_model_id, str) and model_id != manifest_model_id:
            errors.append(
                f"MODEL_CATALOG.json: models[{i}] model_id '{model_id}' does not match {manifest_path} model_id '{manifest_model_id}'"
            )

    return errors, resolved_docs


def validate_catalog_unique_model_ids(catalog: dict[str, Any]) -> list[str]:
    """Validate MODEL_CATALOG.json has unique model_id values."""
    errors: list[str] = []
    models = catalog.get("models")
    if not isinstance(models, list):
        return errors

    seen: dict[str, int] = {}
    for i, model in enumerate(models):
        if not isinstance(model, dict):
            continue
        model_id = model.get("model_id")
        if not isinstance(model_id, str) or not model_id:
            continue
        if model_id in seen:
            errors.append(
                f"MODEL_CATALOG.json: duplicate model_id '{model_id}' at models[{seen[model_id]}] and models[{i}]"
            )
            continue
        seen[model_id] = i

    return errors


def validate_manifests_directory(
    manifests_dir: Path,
    manifest_schema: dict[str, Any],
    repo_root: Path,
) -> tuple[list[str], dict[Path, dict[str, Any]]]:
    """Validate every manifest file under shared/model/manifests/."""
    errors: list[str] = []
    docs: dict[Path, dict[str, Any]] = {}

    if not manifests_dir.exists():
        return [f"manifests directory not found: {manifests_dir}"], docs

    manifest_files = sorted(manifests_dir.glob("*.json"))
    if not manifest_files:
        return [f"no manifest files found in: {manifests_dir}"], docs

    for path in manifest_files:
        rel_label = str(path.relative_to(repo_root))
        parsed, parse_errors = load_json_file(path, rel_label)
        if parse_errors:
            errors.extend(parse_errors)
            continue
        assert parsed is not None
        docs[path.resolve()] = parsed

        errors.extend(validate_document_against_schema(parsed, manifest_schema, rel_label))
        errors.extend(validate_sha256_fields(parsed, rel_label))

    return errors, docs


def validate_ipc_model_ids(manifest: dict[str, Any], examples_file: Path) -> list[str]:
    """Validate asr.initialize examples use the legacy manifest model_id."""
    errors = []

    if not examples_file.exists():
        errors.append(f"IPC examples file not found: {examples_file}")
        return errors

    manifest_model_id = manifest.get("model_id")
    if not manifest_model_id:
        errors.append("Cannot validate IPC examples: manifest has no model_id")
        return errors

    with open(examples_file, "r") as f:
        for line_num, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue

            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue

            data = obj.get("data", {})
            method = data.get("method")

            if obj.get("type") == "request" and method == "asr.initialize":
                params = data.get("params", {})
                model_id = params.get("model_id")

                if model_id and model_id != manifest_model_id:
                    errors.append(
                        f"Line {line_num}: asr.initialize uses model_id '{model_id}' "
                        f"but manifest defines '{manifest_model_id}'"
                    )

    return errors


def validate_rust_model_defaults(manifest: dict[str, Any], repo_root: Path) -> list[str]:
    """Validate Rust default model wiring matches legacy manifest contract."""
    errors = []
    manifest_model_id = manifest.get("model_id")
    if not manifest_model_id:
        return ["Cannot validate Rust defaults: manifest has no model_id"]

    integration_file = repo_root / "src-tauri" / "src" / "integration.rs"
    commands_file = repo_root / "src-tauri" / "src" / "commands.rs"
    model_defaults_file = repo_root / "src-tauri" / "src" / "model_defaults.rs"

    required_snippets = {
        integration_file: DEFAULT_MODEL_CALL_SNIPPET,
        commands_file: DEFAULT_MODEL_CALL_SNIPPET,
        model_defaults_file: MANIFEST_MODEL_INCLUDE_SNIPPET,
    }

    for file_path, snippet in required_snippets.items():
        if not file_path.exists():
            errors.append(f"Rust defaults file not found: {file_path}")
            continue

        content = file_path.read_text()
        if snippet not in content:
            errors.append(
                f"{file_path.relative_to(repo_root)} must include '{snippet}' for manifest-backed defaults"
            )

        for literal in set(re.findall(r'"(parakeet-tdt-0\.6b-v[^"]+)"', content)):
            if literal != manifest_model_id:
                errors.append(
                    f"{file_path.relative_to(repo_root)} embeds model_id '{literal}' "
                    f"but manifest defines '{manifest_model_id}'"
                )

    fallback_match = None
    if model_defaults_file.exists():
        fallback_match = re.search(
            r'const DEFAULT_MODEL_ID:\s*&str\s*=\s*"([^"]+)";',
            model_defaults_file.read_text(),
        )
    if fallback_match and fallback_match.group(1) != manifest_model_id:
        errors.append(
            f"src-tauri/src/model_defaults.rs fallback DEFAULT_MODEL_ID is '{fallback_match.group(1)}' "
            f"but manifest defines '{manifest_model_id}'"
        )

    return errors


def main() -> int:
    """Main validation function."""
    script_dir = Path(__file__).parent
    repo_root = script_dir.parent

    model_root = repo_root / "shared" / "model"
    manifest_file = model_root / "MODEL_MANIFEST.json"
    catalog_file = model_root / "MODEL_CATALOG.json"
    manifests_dir = model_root / "manifests"
    examples_file = repo_root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
    catalog_schema_file = repo_root / "shared" / "schema" / "ModelCatalog.schema.json"
    manifest_schema_file = repo_root / "shared" / "schema" / "ModelManifest.schema.json"

    all_errors: list[str] = []

    # Legacy manifest parsing + validation
    manifest, manifest_parse_errors = load_json_file(manifest_file, "MODEL_MANIFEST.json")
    all_errors.extend(manifest_parse_errors)
    if manifest is not None:
        all_errors.extend(f"MODEL_MANIFEST.json: {e}" for e in validate_manifest_schema(manifest))
        all_errors.extend(validate_sha256_fields(manifest, "MODEL_MANIFEST.json"))
        all_errors.extend(validate_ipc_model_ids(manifest, examples_file))
        all_errors.extend(validate_rust_model_defaults(manifest, repo_root))

    # Load schemas
    catalog_schema, catalog_schema_errors = load_json_file(catalog_schema_file, "ModelCatalog.schema.json")
    manifest_schema, manifest_schema_errors = load_json_file(manifest_schema_file, "ModelManifest.schema.json")
    all_errors.extend(catalog_schema_errors)
    all_errors.extend(manifest_schema_errors)

    # Catalog parsing + schema validation
    catalog, catalog_parse_errors = load_json_file(catalog_file, "MODEL_CATALOG.json")
    all_errors.extend(catalog_parse_errors)

    if catalog is not None and catalog_schema is not None:
        all_errors.extend(validate_document_against_schema(catalog, catalog_schema, "MODEL_CATALOG.json"))

    # Catalog manifest path resolution
    if catalog is not None:
        all_errors.extend(validate_catalog_unique_model_ids(catalog))
        path_errors, _ = validate_catalog_manifest_paths(catalog, model_root)
        all_errors.extend(path_errors)

    # Validate every manifest in manifests/ against schema + sha256
    if manifest_schema is not None:
        dir_errors, _ = validate_manifests_directory(manifests_dir, manifest_schema, repo_root)
        all_errors.extend(dir_errors)

    if all_errors:
        print("VALIDATION FAILED", file=sys.stderr)
        print("-" * 40, file=sys.stderr)
        for err in all_errors:
            print(f"  {err}", file=sys.stderr)
        print("-" * 40, file=sys.stderr)
        return 1

    print("✓ Model Manifest/Catalog Validation Passed")
    if manifest is not None:
        print(f"  Legacy Model ID: {manifest.get('model_id', 'N/A')}")
        print(f"  Legacy Source: {manifest.get('source', 'N/A')}")
    if catalog is not None:
        models = catalog.get("models", [])
        print(f"  Catalog Entries: {len(models) if isinstance(models, list) else 0}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
