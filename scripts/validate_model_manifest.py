#!/usr/bin/env python3
"""
Validate MODEL_MANIFEST.json schema and cross-reference with IPC examples/runtime defaults.

This script:
1. Validates MODEL_MANIFEST.json parses correctly
2. Validates required schema fields are present
3. Validates asr.initialize examples in IPC_V1_EXAMPLES.jsonl use manifest model_id
4. Validates Rust runtime defaults resolve model_id from manifest-backed defaults

Exit codes:
  0 - All validations passed
  1 - Validation errors found
"""

import json
import re
import sys
from pathlib import Path
from typing import Any

# Required top-level fields in manifest
REQUIRED_MANIFEST_FIELDS = {
    "schema_version",
    "model_id",
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

# Required fields in file objects
REQUIRED_FILE_FIELDS = {
    "path",
    "size_bytes",
    "sha256",
}

SHA256_PATTERN = re.compile(r"^[a-f0-9]{64}$")

MANIFEST_MODEL_INCLUDE_SNIPPET = 'include_str!("../../shared/model/MODEL_MANIFEST.json")'
DEFAULT_MODEL_CALL_SNIPPET = "model_defaults::default_model_id()"


def validate_manifest_schema(manifest: dict[str, Any]) -> list[str]:
    """Validate manifest has required fields."""
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
    if "schema_version" in manifest:
        if not isinstance(manifest["schema_version"], str):
            errors.append("schema_version must be a string")

    if "model_id" in manifest:
        if not isinstance(manifest["model_id"], str):
            errors.append("model_id must be a string")
        elif not manifest["model_id"]:
            errors.append("model_id cannot be empty")

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


def validate_ipc_model_ids(manifest: dict[str, Any], examples_file: Path) -> list[str]:
    """Validate asr.initialize examples use the manifest model_id."""
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
                continue  # Parse errors handled by other validator

            data = obj.get("data", {})
            method = data.get("method")

            # Check asr.initialize requests
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
    """Validate Rust default model wiring matches manifest contract."""
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

    manifest_file = repo_root / "shared" / "model" / "MODEL_MANIFEST.json"
    examples_file = repo_root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"

    all_errors: list[str] = []

    # Check manifest exists
    if not manifest_file.exists():
        print(f"ERROR: Manifest file not found: {manifest_file}", file=sys.stderr)
        return 1

    # Parse manifest
    try:
        with open(manifest_file, "r") as f:
            manifest = json.load(f)
    except json.JSONDecodeError as e:
        print(f"ERROR: Failed to parse manifest JSON: {e}", file=sys.stderr)
        return 1

    # Validate schema
    schema_errors = validate_manifest_schema(manifest)
    all_errors.extend(schema_errors)

    # Validate IPC and runtime defaults cross-reference
    all_errors.extend(validate_ipc_model_ids(manifest, examples_file))
    all_errors.extend(validate_rust_model_defaults(manifest, repo_root))

    # Print results
    if all_errors:
        print("VALIDATION FAILED", file=sys.stderr)
        print("-" * 40, file=sys.stderr)
        for err in all_errors:
            print(f"  {err}", file=sys.stderr)
        print("-" * 40, file=sys.stderr)
        return 1

    print("✓ Model Manifest Validation Passed")
    print(f"  Model ID: {manifest.get('model_id', 'N/A')}")
    print(f"  Source: {manifest.get('source', 'N/A')}")
    print(f"  Revision: {manifest.get('revision', 'N/A')[:12]}...")
    print(f"  License: {manifest.get('license', {}).get('spdx_id', 'N/A')}")
    print(f"  Files: {len(manifest.get('files', []))}")
    print(f"  Total Size: {manifest.get('total_size_bytes', 0) / 1e9:.2f} GB")

    return 0


if __name__ == "__main__":
    sys.exit(main())
