#!/usr/bin/env python3
"""Validate cross-language AppConfig/ReplacementRule parity against shared schemas.

Checks:
1. shared/schema/AppConfig.schema.json property set matches
   - src/types.ts AppConfig fields
   - src-tauri/src/config.rs ROOT_CONFIG_FIELDS
2. shared/schema/ReplacementRule.schema.json property set matches
   - src/types.ts ReplacementRule fields
   - src-tauri/src/config.rs REPLACEMENT_RULE_FIELDS
3. ReplacementKind enum literals in TypeScript match schema enum.

Exit codes:
  0 - Parity checks passed
  1 - Drift detected
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Iterable

APP_CONFIG_CONST = "ROOT_CONFIG_FIELDS"
REPLACEMENT_RULE_CONST = "REPLACEMENT_RULE_FIELDS"


def load_schema_property_names(schema_path: Path) -> set[str]:
    data = json.loads(schema_path.read_text())
    properties = data.get("properties")
    if not isinstance(properties, dict):
        raise ValueError(f"{schema_path} missing object 'properties'")
    return set(properties.keys())


def load_schema_enum(schema_path: Path, property_name: str) -> set[str]:
    data = json.loads(schema_path.read_text())
    props = data.get("properties", {})
    prop = props.get(property_name, {}) if isinstance(props, dict) else {}
    enum_vals = prop.get("enum", []) if isinstance(prop, dict) else []
    if not isinstance(enum_vals, list):
        raise ValueError(f"{schema_path} property '{property_name}' has non-list enum")
    return {v for v in enum_vals if isinstance(v, str)}


def parse_rust_const_string_array(content: str, const_name: str) -> set[str]:
    pattern = re.compile(
        rf"const\s+{re.escape(const_name)}\s*:\s*\[&str;\s*\d+\]\s*=\s*\[(.*?)\];",
        re.DOTALL,
    )
    match = pattern.search(content)
    if not match:
        raise ValueError(f"Rust constant '{const_name}' not found")

    body = match.group(1)
    return set(re.findall(r'"([^"]+)"', body))


def parse_ts_interface_fields(content: str, interface_name: str) -> set[str]:
    pattern = re.compile(
        rf"export\s+interface\s+{re.escape(interface_name)}\s*\{{(.*?)\n\}}",
        re.DOTALL,
    )
    match = pattern.search(content)
    if not match:
        raise ValueError(f"TypeScript interface '{interface_name}' not found")

    body = match.group(1)
    fields: set[str] = set()
    for line in body.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("/**") or stripped.startswith("*"):
            continue
        m = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\??\s*:", stripped)
        if m:
            fields.add(m.group(1))

    if not fields:
        raise ValueError(f"No fields parsed for TypeScript interface '{interface_name}'")
    return fields


def parse_ts_union_literals(content: str, type_name: str) -> set[str]:
    pattern = re.compile(
        rf"export\s+type\s+{re.escape(type_name)}\s*=\s*(.*?);",
        re.DOTALL,
    )
    match = pattern.search(content)
    if not match:
        raise ValueError(f"TypeScript type alias '{type_name}' not found")

    rhs = match.group(1)
    return set(re.findall(r"'([^']+)'", rhs))


def _format_set(items: Iterable[str]) -> str:
    return ", ".join(sorted(items))


def compare_sets(expected: set[str], actual: set[str], label: str) -> list[str]:
    errors: list[str] = []
    missing = expected - actual
    extra = actual - expected

    if missing:
        errors.append(f"{label}: missing fields: {_format_set(missing)}")
    if extra:
        errors.append(f"{label}: extra fields: {_format_set(extra)}")

    return errors


def validate_config_schema_parity(repo_root: Path) -> list[str]:
    errors: list[str] = []

    app_schema_path = repo_root / "shared" / "schema" / "AppConfig.schema.json"
    replacement_schema_path = repo_root / "shared" / "schema" / "ReplacementRule.schema.json"
    ts_types_path = repo_root / "src" / "types.ts"
    rust_config_path = repo_root / "src-tauri" / "src" / "config.rs"

    required_files = [
        app_schema_path,
        replacement_schema_path,
        ts_types_path,
        rust_config_path,
    ]
    for file_path in required_files:
        if not file_path.exists():
            errors.append(f"Required file not found: {file_path.relative_to(repo_root)}")

    if errors:
        return errors

    app_schema_fields = load_schema_property_names(app_schema_path)
    replacement_schema_fields = load_schema_property_names(replacement_schema_path)

    ts_content = ts_types_path.read_text()
    rust_content = rust_config_path.read_text()

    ts_app_fields = parse_ts_interface_fields(ts_content, "AppConfig")
    ts_replacement_fields = parse_ts_interface_fields(ts_content, "ReplacementRule")

    rust_app_fields = parse_rust_const_string_array(rust_content, APP_CONFIG_CONST)
    rust_replacement_fields = parse_rust_const_string_array(rust_content, REPLACEMENT_RULE_CONST)

    errors.extend(compare_sets(app_schema_fields, ts_app_fields, "TypeScript AppConfig parity"))
    errors.extend(compare_sets(app_schema_fields, rust_app_fields, "Rust AppConfig parity"))
    errors.extend(
        compare_sets(
            replacement_schema_fields,
            ts_replacement_fields,
            "TypeScript ReplacementRule parity",
        )
    )
    errors.extend(
        compare_sets(
            replacement_schema_fields,
            rust_replacement_fields,
            "Rust ReplacementRule parity",
        )
    )

    schema_kind_enum = load_schema_enum(replacement_schema_path, "kind")
    ts_kind_enum = parse_ts_union_literals(ts_content, "ReplacementKind")
    if schema_kind_enum != ts_kind_enum:
        missing = schema_kind_enum - ts_kind_enum
        extra = ts_kind_enum - schema_kind_enum
        if missing:
            errors.append(f"TypeScript ReplacementKind missing enum values: {_format_set(missing)}")
        if extra:
            errors.append(f"TypeScript ReplacementKind has extra enum values: {_format_set(extra)}")

    return errors


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]

    try:
        errors = validate_config_schema_parity(repo_root)
    except Exception as exc:  # pragma: no cover - fatal path
        print(f"VALIDATION FAILED: {exc}", file=sys.stderr)
        return 1

    if errors:
        print("VALIDATION FAILED", file=sys.stderr)
        print("-" * 40, file=sys.stderr)
        for err in errors:
            print(f"  {err}", file=sys.stderr)
        print("-" * 40, file=sys.stderr)
        return 1

    print("✓ Config schema parity validation passed")
    print("  Shared schemas: AppConfig, ReplacementRule")
    print("  Cross-language targets: src/types.ts and src-tauri/src/config.rs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
