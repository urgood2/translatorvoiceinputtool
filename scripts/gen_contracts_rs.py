#!/usr/bin/env python3
"""Generate Rust contract constants/types from shared contract JSON files.

Reads:
- shared/contracts/tauri.commands.v1.json
- shared/contracts/tauri.events.v1.json
- shared/contracts/sidecar.rpc.v1.json

Writes:
- src-tauri/src/contracts.rs

Output is deterministic:
- no timestamps
- no absolute paths
- stable sorted ordering for generated blocks
"""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = REPO_ROOT / "src-tauri" / "src" / "contracts.rs"

RUST_KEYWORDS = {
    "as",
    "break",
    "const",
    "continue",
    "crate",
    "else",
    "enum",
    "extern",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "in",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "pub",
    "ref",
    "return",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "trait",
    "true",
    "type",
    "unsafe",
    "use",
    "where",
    "while",
    "async",
    "await",
    "dyn",
    "abstract",
    "become",
    "box",
    "do",
    "final",
    "macro",
    "override",
    "priv",
    "try",
    "typeof",
    "unsized",
    "virtual",
    "yield",
}


@dataclass(frozen=True)
class ContractContext:
    prefix: str
    defs: dict[str, Any]
    def_aliases: dict[str, str]


def pascal_case(value: str) -> str:
    parts = re.split(r"[^A-Za-z0-9]+", value)
    out = "".join(part[:1].upper() + part[1:] for part in parts if part)
    return out or "Unnamed"


def snake_case(value: str) -> str:
    normalized = re.sub(r"[^A-Za-z0-9]+", "_", value)
    normalized = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", normalized)
    normalized = normalized.strip("_").lower()
    return normalized or "field"


def screaming_snake(value: str) -> str:
    normalized = re.sub(r"[^A-Za-z0-9]+", "_", value)
    normalized = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", normalized)
    normalized = re.sub(r"_+", "_", normalized).strip("_").upper()
    return normalized or "UNNAMED"


def rust_field_name(name: str) -> str:
    candidate = snake_case(name)
    if candidate in RUST_KEYWORDS:
        return f"{candidate}_field"
    if candidate and candidate[0].isdigit():
        return f"field_{candidate}"
    return candidate


def unique_preserve_order(items: list[str]) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for item in items:
        if item in seen:
            continue
        seen.add(item)
        out.append(item)
    return out


def schema_with_forced_type(schema: dict[str, Any], type_name: str) -> dict[str, Any]:
    forced = dict(schema)
    forced["type"] = type_name
    return forced


def is_nullable_union_schema(schema: dict[str, Any]) -> tuple[bool, Any]:
    schema_type = schema.get("type")
    if isinstance(schema_type, list):
        non_null = [t for t in schema_type if t != "null"]
        if len(non_null) == 1 and len(schema_type) == 2:
            return True, schema_with_forced_type(schema, non_null[0])
    for combiner in ("anyOf", "oneOf"):
        values = schema.get(combiner)
        if isinstance(values, list):
            null_entries = [v for v in values if isinstance(v, dict) and v.get("type") == "null"]
            other_entries = [v for v in values if not (isinstance(v, dict) and v.get("type") == "null")]
            if len(null_entries) == 1 and len(other_entries) == 1:
                return True, other_entries[0]
    return False, None


def type_from_ref(ref: str, ctx: ContractContext) -> str:
    if ref.startswith("#/$defs/"):
        key = ref[len("#/$defs/") :]
        return ctx.def_aliases.get(key, "serde_json::Value")
    if ref.startswith("#/definitions/"):
        key = ref[len("#/definitions/") :]
        return ctx.def_aliases.get(key, "serde_json::Value")
    return "serde_json::Value"


def schema_to_rust_type(schema: Any, ctx: ContractContext) -> str:
    if not isinstance(schema, dict):
        return "serde_json::Value"

    nullable, inner = is_nullable_union_schema(schema)
    if nullable:
        return f"Option<{schema_to_rust_type(inner, ctx)}>"

    if "$ref" in schema and isinstance(schema["$ref"], str):
        return type_from_ref(schema["$ref"], ctx)

    if "enum" in schema:
        return "String"

    for combiner in ("anyOf", "oneOf", "allOf"):
        if isinstance(schema.get(combiner), list):
            return "serde_json::Value"

    schema_type = schema.get("type")
    if isinstance(schema_type, list):
        branches: list[str] = []
        for t in schema_type:
            if isinstance(t, str):
                branches.append(schema_to_rust_type(schema_with_forced_type(schema, t), ctx))
        branches = unique_preserve_order(branches)
        if len(branches) == 1:
            return branches[0]
        return "serde_json::Value"

    if schema_type == "string":
        return "String"
    if schema_type == "boolean":
        return "bool"
    if schema_type == "integer":
        return "i64"
    if schema_type == "number":
        return "f64"
    if schema_type == "null":
        return "()"
    if schema_type == "array":
        return f"Vec<{schema_to_rust_type(schema.get('items', {}), ctx)}>"
    if schema_type == "object" or "properties" in schema or "additionalProperties" in schema:
        properties = schema.get("properties")
        additional = schema.get("additionalProperties")
        if isinstance(properties, dict):
            return "serde_json::Value"
        if additional is False:
            return "serde_json::Map<String, serde_json::Value>"
        if isinstance(additional, dict):
            return f"BTreeMap<String, {schema_to_rust_type(additional, ctx)}>"
        return "BTreeMap<String, serde_json::Value>"

    return "serde_json::Value"


def build_contract_context(prefix: str, contract: dict[str, Any]) -> ContractContext:
    defs = contract.get("$defs")
    if not isinstance(defs, dict):
        defs = contract.get("definitions")
    if not isinstance(defs, dict):
        defs = {}
    aliases = {name: f"{prefix}Def{pascal_case(name)}" for name in sorted(defs.keys())}
    return ContractContext(prefix=prefix, defs=defs, def_aliases=aliases)


def emit_struct_from_object_schema(
    lines: list[str],
    struct_name: str,
    schema: dict[str, Any],
    ctx: ContractContext,
    *,
    pub: bool = True,
) -> None:
    properties = schema.get("properties")
    required = set(schema.get("required", [])) if isinstance(schema.get("required"), list) else set()
    additional = schema.get("additionalProperties")

    vis = "pub " if pub else ""
    lines.append("#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]")
    lines.append(f"{vis}struct {struct_name} {{")

    if isinstance(properties, dict):
        used_field_names: set[str] = set()
        for prop_name in sorted(properties):
            prop_schema = properties[prop_name]
            base_field_name = rust_field_name(prop_name)
            field_name = base_field_name
            suffix = 2
            while field_name in used_field_names:
                field_name = f"{base_field_name}_{suffix}"
                suffix += 1
            used_field_names.add(field_name)
            field_type = schema_to_rust_type(prop_schema, ctx)
            is_required = prop_name in required
            if not is_required and not field_type.startswith("Option<"):
                field_type = f"Option<{field_type}>"

            attrs: list[str] = []
            if field_name != prop_name:
                attrs.append(f"rename = {json.dumps(prop_name)}")
            if not is_required:
                attrs.append("default")
                attrs.append('skip_serializing_if = "Option::is_none"')
            if attrs:
                lines.append(f"    #[serde({', '.join(attrs)})]")
            lines.append(f"    pub {field_name}: {field_type},")

    if additional is True or isinstance(additional, dict):
        extra_type = (
            schema_to_rust_type(additional, ctx) if isinstance(additional, dict) else "serde_json::Value"
        )
        lines.append("    #[serde(flatten)]")
        lines.append(f"    pub extra: BTreeMap<String, {extra_type}>,")

    lines.append("}")
    lines.append("")


def emit_alias_from_schema(lines: list[str], alias_name: str, schema: dict[str, Any], ctx: ContractContext) -> None:
    rust_type = schema_to_rust_type(schema, ctx)
    lines.append(f"pub type {alias_name} = {rust_type};")
    lines.append("")


def emit_schema_type(lines: list[str], type_name: str, schema: Any, ctx: ContractContext) -> None:
    if not isinstance(schema, dict):
        emit_alias_from_schema(lines, type_name, {}, ctx)
        return

    schema_type = schema.get("type")
    has_object_shape = schema_type == "object" or isinstance(schema.get("properties"), dict)
    if has_object_shape:
        emit_struct_from_object_schema(lines, type_name, schema, ctx)
    else:
        emit_alias_from_schema(lines, type_name, schema, ctx)


def emit_definitions(lines: list[str], ctx: ContractContext) -> None:
    if not ctx.def_aliases:
        return
    lines.append(f"// {ctx.prefix} local definitions")
    for def_name in sorted(ctx.def_aliases):
        alias = ctx.def_aliases[def_name]
        schema = ctx.defs[def_name]
        emit_schema_type(lines, alias, schema, ctx)


def emit_const_list(lines: list[str], const_name: str, values: list[str]) -> None:
    lines.append(f"pub const {const_name}: &[&str] = &[")
    for value in values:
        lines.append(f"    {json.dumps(value)},")
    lines.append("];")
    lines.append("")


def generate_rust(tauri_commands: dict[str, Any], tauri_events: dict[str, Any], sidecar_rpc: dict[str, Any]) -> str:
    lines: list[str] = []
    lines.append("// AUTO-GENERATED from shared/contracts/*.v1.json - do not edit manually")
    lines.append("// Generated by scripts/gen_contracts_rs.py")
    lines.append("// Regenerate with: python scripts/gen_contracts_rs.py")
    lines.append("")
    lines.append("use serde::{Deserialize, Serialize};")
    lines.append("use std::collections::BTreeMap;")
    lines.append("")

    commands_ctx = build_contract_context("TauriCommand", tauri_commands)
    events_ctx = build_contract_context("TauriEvent", tauri_events)
    sidecar_ctx = build_contract_context("SidecarRpc", sidecar_rpc)

    emit_definitions(lines, commands_ctx)
    emit_definitions(lines, events_ctx)
    emit_definitions(lines, sidecar_ctx)

    command_items = sorted(
        [item for item in tauri_commands.get("items", []) if item.get("type") == "command"],
        key=lambda item: item.get("name", ""),
    )
    command_names: list[str] = []
    lines.append("// Tauri command constants and payload types")
    for item in command_items:
        name = item.get("name")
        if not isinstance(name, str):
            continue
        command_names.append(name)
        const_name = f"CMD_{screaming_snake(name)}"
        lines.append(f"pub const {const_name}: &str = {json.dumps(name)};")
    lines.append("")
    emit_const_list(lines, "TAURI_COMMAND_NAMES", command_names)
    for name in command_names:
        pascal = pascal_case(name)
        item = next(i for i in command_items if i.get("name") == name)
        emit_schema_type(lines, f"Command{pascal}Params", item.get("params_schema", {}), commands_ctx)
        emit_schema_type(lines, f"Command{pascal}Result", item.get("result_schema", {}), commands_ctx)

    event_items = sorted(
        [item for item in tauri_events.get("items", []) if item.get("type") == "event"],
        key=lambda item: item.get("name", ""),
    )
    event_names: list[str] = []
    lines.append("// Tauri event constants and payload types")
    for item in event_items:
        name = item.get("name")
        if not isinstance(name, str):
            continue
        event_names.append(name)
        canonical_const = f"EVENT_{screaming_snake(name)}"
        lines.append(f"pub const {canonical_const}: &str = {json.dumps(name)};")
        aliases = item.get("deprecated_aliases", [])
        if isinstance(aliases, list):
            for idx, alias in enumerate(aliases, start=1):
                if not isinstance(alias, str):
                    continue
                suffix = "_LEGACY" if idx == 1 else f"_LEGACY_{idx}"
                alias_const = f"{canonical_const}{suffix}"
                lines.append(f"pub const {alias_const}: &str = {json.dumps(alias)};")
    lines.append("")
    emit_const_list(lines, "TAURI_EVENT_NAMES", event_names)
    for name in event_names:
        pascal = pascal_case(name)
        item = next(i for i in event_items if i.get("name") == name)
        emit_schema_type(lines, f"Event{pascal}Payload", item.get("payload_schema", {}), events_ctx)

    method_items = sorted(
        [item for item in sidecar_rpc.get("items", []) if item.get("type") == "method"],
        key=lambda item: item.get("name", ""),
    )
    method_names: list[str] = []
    required_method_names: list[str] = []
    optional_method_names: list[str] = []
    lines.append("// Sidecar RPC method constants and payload types")
    for item in method_items:
        name = item.get("name")
        if not isinstance(name, str):
            continue
        method_names.append(name)
        if item.get("required") is True:
            required_method_names.append(name)
        else:
            optional_method_names.append(name)
        const_name = f"RPC_{screaming_snake(name)}"
        lines.append(f"pub const {const_name}: &str = {json.dumps(name)};")
    lines.append("")
    emit_const_list(lines, "SIDECAR_RPC_METHOD_NAMES", method_names)
    emit_const_list(lines, "SIDECAR_RPC_REQUIRED_METHOD_NAMES", required_method_names)
    emit_const_list(lines, "SIDECAR_RPC_OPTIONAL_METHOD_NAMES", optional_method_names)
    for name in method_names:
        pascal = pascal_case(name)
        item = next(i for i in method_items if i.get("name") == name)
        emit_schema_type(lines, f"Rpc{pascal}Params", item.get("params_schema", {}), sidecar_ctx)
        emit_schema_type(lines, f"Rpc{pascal}Result", item.get("result_schema", {}), sidecar_ctx)

    notification_items = sorted(
        [item for item in sidecar_rpc.get("items", []) if item.get("type") == "notification"],
        key=lambda item: item.get("name", ""),
    )
    notification_names: list[str] = []
    lines.append("// Sidecar RPC notification constants and payload types")
    for item in notification_items:
        name = item.get("name")
        if not isinstance(name, str):
            continue
        notification_names.append(name)
        const_name = f"RPC_NOTIFY_{screaming_snake(name)}"
        lines.append(f"pub const {const_name}: &str = {json.dumps(name)};")
    lines.append("")
    emit_const_list(lines, "SIDECAR_RPC_NOTIFICATION_NAMES", notification_names)
    for name in notification_names:
        pascal = pascal_case(name)
        item = next(i for i in notification_items if i.get("name") == name)
        emit_schema_type(lines, f"RpcNotification{pascal}Params", item.get("params_schema", {}), sidecar_ctx)

    return "\n".join(lines).rstrip() + "\n"


def read_json(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError(f"Expected JSON object at {path}")
    return data


def generate_to_path(output_path: Path, repo_root: Path) -> None:
    contracts_root = repo_root / "shared" / "contracts"
    tauri_commands = read_json(contracts_root / "tauri.commands.v1.json")
    tauri_events = read_json(contracts_root / "tauri.events.v1.json")
    sidecar_rpc = read_json(contracts_root / "sidecar.rpc.v1.json")
    output = generate_rust(tauri_commands, tauri_events, sidecar_rpc)
    output_path.write_text(output, encoding="utf-8")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate src-tauri/src/contracts.rs from JSON contracts.")
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=REPO_ROOT,
        help="Repository root containing shared/contracts (default: script-inferred root).",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=DEFAULT_OUTPUT,
        help="Output Rust file path (default: src-tauri/src/contracts.rs).",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    repo_root = args.repo_root.resolve()
    out_path = args.out
    if not out_path.is_absolute():
        out_path = repo_root / out_path
    out_path.parent.mkdir(parents=True, exist_ok=True)
    generate_to_path(out_path, repo_root)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
