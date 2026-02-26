#!/usr/bin/env python3
"""Generate TypeScript contract types from shared contract JSON files.

Reads:
- shared/contracts/tauri.commands.v1.json
- shared/contracts/tauri.events.v1.json
- shared/contracts/sidecar.rpc.v1.json

Writes:
- src/types.contracts.ts

Output is deterministic:
- no timestamps
- no absolute paths
- stable sorted ordering for generated type blocks/maps
"""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = REPO_ROOT / "src" / "types.contracts.ts"

COMMANDS_CONTRACT = REPO_ROOT / "shared" / "contracts" / "tauri.commands.v1.json"
EVENTS_CONTRACT = REPO_ROOT / "shared" / "contracts" / "tauri.events.v1.json"
SIDECAR_CONTRACT = REPO_ROOT / "shared" / "contracts" / "sidecar.rpc.v1.json"


@dataclass(frozen=True)
class ContractContext:
    """Conversion context for one contract file."""

    type_prefix: str
    defs: dict[str, Any]
    def_aliases: dict[str, str]


def pascal_case(value: str) -> str:
    parts = re.split(r"[^A-Za-z0-9]+", value)
    return "".join(part[:1].upper() + part[1:] for part in parts if part)


def format_identifier(name: str) -> str:
    if re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", name):
        return name
    return json.dumps(name)


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


def literal_ts(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    return json.dumps(value)


def type_from_ref(ref: str, ctx: ContractContext) -> str:
    if ref.startswith("#/$defs/"):
        key = ref[len("#/$defs/") :]
        return ctx.def_aliases.get(key, "unknown")
    if ref.startswith("#/definitions/"):
        key = ref[len("#/definitions/") :]
        return ctx.def_aliases.get(key, "unknown")
    return "unknown"


def schema_to_ts(schema: Any, ctx: ContractContext) -> str:
    if not isinstance(schema, dict):
        return "unknown"

    if "$ref" in schema:
        ref = schema["$ref"]
        if isinstance(ref, str):
            return type_from_ref(ref, ctx)
        return "unknown"

    if "const" in schema:
        return literal_ts(schema["const"])

    if "enum" in schema and isinstance(schema["enum"], list):
        literals = [literal_ts(v) for v in schema["enum"]]
        literals = unique_preserve_order(literals)
        return " | ".join(literals) if literals else "unknown"

    for combiner, joiner in (("anyOf", " | "), ("oneOf", " | "), ("allOf", " & ")):
        if combiner in schema and isinstance(schema[combiner], list):
            parts = [schema_to_ts(part, ctx) for part in schema[combiner]]
            parts = unique_preserve_order(parts)
            return joiner.join(parts) if parts else "unknown"

    schema_type = schema.get("type")
    if isinstance(schema_type, list):
        branches: list[str] = []
        for t in schema_type:
            if isinstance(t, str):
                branches.append(schema_to_ts(schema_with_forced_type(schema, t), ctx))
        branches = unique_preserve_order(branches)
        return " | ".join(branches) if branches else "unknown"

    if schema_type == "string":
        return "string"
    if schema_type in ("number", "integer"):
        return "number"
    if schema_type == "boolean":
        return "boolean"
    if schema_type == "null":
        return "null"
    if schema_type == "array":
        item_ts = schema_to_ts(schema.get("items", {}), ctx)
        return f"Array<{item_ts}>"
    if schema_type == "object" or "properties" in schema or "additionalProperties" in schema:
        return object_schema_to_ts(schema, ctx)

    return "unknown"


def object_schema_to_ts(schema: dict[str, Any], ctx: ContractContext) -> str:
    properties = schema.get("properties")
    required = set(schema.get("required", [])) if isinstance(schema.get("required"), list) else set()
    additional = schema.get("additionalProperties")

    if not isinstance(properties, dict):
        if additional is False:
            return "Record<string, never>"
        if additional is True or additional is None:
            return "Record<string, unknown>"
        if isinstance(additional, dict):
            return f"Record<string, {schema_to_ts(additional, ctx)}>"
        return "Record<string, unknown>"

    lines: list[str] = ["{"]
    for prop_name in sorted(properties):
        prop_schema = properties[prop_name]
        optional = "" if prop_name in required else "?"
        prop_ts = schema_to_ts(prop_schema, ctx)
        lines.append(f"  {format_identifier(prop_name)}{optional}: {prop_ts};")

    if additional is True:
        lines.append("  [key: string]: unknown;")
    elif isinstance(additional, dict):
        lines.append(f"  [key: string]: {schema_to_ts(additional, ctx)};")

    lines.append("}")
    return "\n".join(lines)


def build_contract_context(type_prefix: str, contract: dict[str, Any]) -> ContractContext:
    defs = contract.get("$defs")
    if not isinstance(defs, dict):
        defs = contract.get("definitions")
    if not isinstance(defs, dict):
        defs = {}

    def_aliases = {
        def_name: f"{type_prefix}Def{pascal_case(def_name)}"
        for def_name in sorted(defs.keys())
    }
    return ContractContext(type_prefix=type_prefix, defs=defs, def_aliases=def_aliases)


def emit_defs(lines: list[str], ctx: ContractContext) -> None:
    if not ctx.def_aliases:
        return
    lines.append(f"// {ctx.type_prefix} local definitions")
    for def_name in sorted(ctx.def_aliases):
        alias = ctx.def_aliases[def_name]
        schema = ctx.defs[def_name]
        lines.append(f"export type {alias} = {schema_to_ts(schema, ctx)};")
        lines.append("")


def upper_snake(name: str) -> str:
    """Convert 'foo:bar' / 'foo.bar' / 'foo_bar' to 'FOO_BAR'."""
    return re.sub(r"[^A-Za-z0-9]+", "_", name).upper()


def emit_name_union(lines: list[str], type_name: str, names: list[str]) -> None:
    if not names:
        lines.append(f"export type {type_name} = never;")
        return
    literals = " | ".join(json.dumps(name) for name in names)
    lines.append(f"export type {type_name} = {literals};")


def generate_types(tauri_commands: dict[str, Any], tauri_events: dict[str, Any], sidecar_rpc: dict[str, Any]) -> str:
    lines: list[str] = []
    lines.append("/*")
    lines.append(" * AUTO-GENERATED FILE. DO NOT EDIT.")
    lines.append(" *")
    lines.append(" * Generated by: scripts/gen_contracts_ts.py")
    lines.append(" * Sources:")
    lines.append(" * - shared/contracts/tauri.commands.v1.json")
    lines.append(" * - shared/contracts/tauri.events.v1.json")
    lines.append(" * - shared/contracts/sidecar.rpc.v1.json")
    lines.append(" */")
    lines.append("")

    commands_ctx = build_contract_context("TauriCommand", tauri_commands)
    events_ctx = build_contract_context("TauriEvent", tauri_events)
    sidecar_ctx = build_contract_context("SidecarRpc", sidecar_rpc)

    emit_defs(lines, commands_ctx)
    emit_defs(lines, events_ctx)
    emit_defs(lines, sidecar_ctx)

    command_items = sorted(
        [item for item in tauri_commands.get("items", []) if item.get("type") == "command"],
        key=lambda item: item.get("name", ""),
    )
    command_names: list[str] = []
    lines.append("// Tauri command params/results")
    for item in command_items:
        name = item.get("name")
        if not isinstance(name, str):
            continue
        command_names.append(name)
        pascal = pascal_case(name)
        params_ts = schema_to_ts(item.get("params_schema", {}), commands_ctx)
        result_ts = schema_to_ts(item.get("result_schema", {}), commands_ctx)
        lines.append(f"export type TauriCommand{pascal}Params = {params_ts};")
        lines.append(f"export type TauriCommand{pascal}Result = {result_ts};")
        lines.append("")

    emit_name_union(lines, "TauriCommandName", command_names)
    lines.append("export interface TauriCommandParamsMap {")
    for name in command_names:
        lines.append(f"  {json.dumps(name)}: TauriCommand{pascal_case(name)}Params;")
    lines.append("}")
    lines.append("export interface TauriCommandResultMap {")
    for name in command_names:
        lines.append(f"  {json.dumps(name)}: TauriCommand{pascal_case(name)}Result;")
    lines.append("}")
    lines.append("")

    event_items = sorted(
        [item for item in tauri_events.get("items", []) if item.get("type") == "event"],
        key=lambda item: item.get("name", ""),
    )
    event_names: list[str] = []
    lines.append("// Tauri event payloads")
    for item in event_items:
        name = item.get("name")
        if not isinstance(name, str):
            continue
        event_names.append(name)
        pascal = pascal_case(name)
        payload_ts = schema_to_ts(item.get("payload_schema", {}), events_ctx)
        lines.append(f"export type TauriEvent{pascal}Payload = {payload_ts};")
        lines.append("")

    emit_name_union(lines, "TauriEventName", event_names)
    lines.append("export interface TauriEventPayloadMap {")
    for name in event_names:
        lines.append(f"  {json.dumps(name)}: TauriEvent{pascal_case(name)}Payload;")
    lines.append("}")
    lines.append("")

    method_items = sorted(
        [item for item in sidecar_rpc.get("items", []) if item.get("type") == "method"],
        key=lambda item: item.get("name", ""),
    )
    method_names: list[str] = []
    required_method_names: list[str] = []
    optional_method_names: list[str] = []

    lines.append("// Sidecar RPC method params/results")
    for item in method_items:
        name = item.get("name")
        if not isinstance(name, str):
            continue
        method_names.append(name)
        if item.get("required") is True:
            required_method_names.append(name)
        else:
            optional_method_names.append(name)
        pascal = pascal_case(name)
        params_ts = schema_to_ts(item.get("params_schema", {}), sidecar_ctx)
        result_ts = schema_to_ts(item.get("result_schema", {}), sidecar_ctx)
        lines.append(f"export type SidecarRpcMethod{pascal}Params = {params_ts};")
        lines.append(f"export type SidecarRpcMethod{pascal}Result = {result_ts};")
        lines.append("")

    emit_name_union(lines, "SidecarRpcMethodName", method_names)
    emit_name_union(lines, "SidecarRpcRequiredMethodName", required_method_names)
    emit_name_union(lines, "SidecarRpcOptionalMethodName", optional_method_names)
    lines.append("export interface SidecarRpcMethodParamsMap {")
    for name in method_names:
        lines.append(f"  {json.dumps(name)}: SidecarRpcMethod{pascal_case(name)}Params;")
    lines.append("}")
    lines.append("export interface SidecarRpcMethodResultMap {")
    for name in method_names:
        lines.append(f"  {json.dumps(name)}: SidecarRpcMethod{pascal_case(name)}Result;")
    lines.append("}")
    lines.append("")

    notification_items = sorted(
        [item for item in sidecar_rpc.get("items", []) if item.get("type") == "notification"],
        key=lambda item: item.get("name", ""),
    )
    notification_names: list[str] = []
    lines.append("// Sidecar RPC notification params")
    for item in notification_items:
        name = item.get("name")
        if not isinstance(name, str):
            continue
        notification_names.append(name)
        pascal = pascal_case(name)
        payload_ts = schema_to_ts(item.get("params_schema", {}), sidecar_ctx)
        lines.append(f"export type SidecarRpcNotification{pascal}Params = {payload_ts};")
        lines.append("")

    emit_name_union(lines, "SidecarRpcNotificationName", notification_names)
    lines.append("export interface SidecarRpcNotificationParamsMap {")
    for name in notification_names:
        lines.append(f"  {json.dumps(name)}: SidecarRpcNotification{pascal_case(name)}Params;")
    lines.append("}")
    lines.append("")

    # Command name constants
    lines.append("// Command name constants")
    for name in command_names:
        lines.append(f"export const COMMAND_{upper_snake(name)} = {json.dumps(name)} as const;")
    lines.append("")

    # Event name constants + legacy aliases
    lines.append("// Event name constants")
    emitted_event_consts: set[str] = set()
    for item in event_items:
        name = item.get("name")
        if not isinstance(name, str):
            continue
        const_name = f"EVENT_{upper_snake(name)}"
        lines.append(f"export const {const_name} = {json.dumps(name)} as const;")
        emitted_event_consts.add(const_name)
        for alias in item.get("deprecated_aliases", []):
            if not isinstance(alias, str) or not alias:
                continue
            alias_const = f"EVENT_{upper_snake(alias)}"
            if alias_const in emitted_event_consts:
                alias_const = f"EVENT_{upper_snake(alias)}_LEGACY"
            lines.append(f"export const {alias_const} = {json.dumps(alias)} as const;")
            emitted_event_consts.add(alias_const)
    lines.append("")

    # Sidecar RPC method name constants
    lines.append("// Sidecar RPC method name constants")
    for name in method_names:
        lines.append(f"export const RPC_METHOD_{upper_snake(name)} = {json.dumps(name)} as const;")
    lines.append("")

    # Event alias mapping (only non-empty)
    alias_entries: list[tuple[str, list[str]]] = []
    for item in event_items:
        name = item.get("name")
        aliases = item.get("deprecated_aliases", [])
        if isinstance(name, str) and isinstance(aliases, list) and aliases:
            alias_entries.append((name, [a for a in aliases if isinstance(a, str) and a]))
    if alias_entries:
        lines.append("// Deprecated event alias mapping (canonical -> legacy names)")
        lines.append("export const EVENT_ALIASES: Record<string, readonly string[]> = {")
        for name, aliases in sorted(alias_entries):
            alias_list = ", ".join(json.dumps(a) for a in aliases)
            lines.append(f"  {json.dumps(name)}: [{alias_list}],")
        lines.append("};")
        lines.append("")

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
    output = generate_types(tauri_commands, tauri_events, sidecar_rpc)
    output_path.write_text(output, encoding="utf-8")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate src/types.contracts.ts from contract JSON files.")
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
        help="Output TypeScript file path (default: src/types.contracts.ts).",
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
