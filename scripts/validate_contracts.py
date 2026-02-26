#!/usr/bin/env python3
"""
Contract validation entrypoint for CI.

Checks:
1) Contract schema fragments are valid Draft-07 JSON Schema.
2) Generated files are up-to-date (re-generate and diff).
3) Frontend Tauri listener event names are declared in tauri.events.v1.json
   (canonical name or deprecated alias).
4) IPC examples validate against sidecar RPC method/notification schemas.
5) Required sidecar RPC methods have fixture request entries.
6) Event payload examples in frontend tests validate against tauri.events schemas.
7) No hard-coded allowlists: names are derived from contract JSON.

Exit codes:
  0 = all checks passed
  1 = validation failures
  2 = script/runtime error
"""

from __future__ import annotations

import argparse
import difflib
import json
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from jsonschema import Draft7Validator, RefResolver
from jsonschema.exceptions import SchemaError


REPO_ROOT = Path(__file__).resolve().parents[1]

CONTRACT_PATHS = {
    "tauri.commands": REPO_ROOT / "shared" / "contracts" / "tauri.commands.v1.json",
    "tauri.events": REPO_ROOT / "shared" / "contracts" / "tauri.events.v1.json",
    "sidecar.rpc": REPO_ROOT / "shared" / "contracts" / "sidecar.rpc.v1.json",
}

GENERATED_TARGETS = (
    ("scripts/gen_contracts_ts.py", "src/types.contracts.ts"),
    ("scripts/gen_contracts_rs.py", "src-tauri/src/contracts.rs"),
)
DERIVED_FIXTURE_CHECK_SCRIPT = "scripts/gen_contract_examples.py"
DERIVED_FIXTURE_DIR = Path("shared/contracts/examples")

FRONTEND_GLOB_PATTERNS = ("src/**/*.ts", "src/**/*.tsx")
EXCLUDED_FRONTEND_PATH_PARTS = {"tests"}
EXAMPLES_PATH = REPO_ROOT / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
EVENT_PAYLOAD_EXAMPLES_PATH = REPO_ROOT / "src" / "hooks" / "useTauriEvents.test.ts"
EVENT_PAYLOAD_IGNORE_MARKER = "contract-validate-ignore"
SIDECAR_SERVER_PATH = REPO_ROOT / "sidecar" / "src" / "openvoicy_sidecar" / "server.py"

METHOD_NAME_RE = re.compile(r"([a-z]+\.[a-z_]+)")
ABSOLUTE_PATH_RE = re.compile(
    r"(?<![A-Za-z0-9_])"
    r"(?:"
    r"/(?!/)(?:[^/\s\"']+/)+[^/\s\"']+"
    r"|[A-Za-z]:\\\\(?:[^\\\\\s\"']+\\\\)+[^\\\\\s\"']+"
    r")"
)


@dataclass(frozen=True)
class ListenerRegistration:
    file: Path
    line: int
    expression: str
    event_name: str | None


@dataclass(frozen=True)
class EventPayloadExample:
    file: Path
    line: int
    event_name: str
    payload: dict[str, Any]


@dataclass(frozen=True)
class RustEmissionSite:
    file: Path
    line: int
    event_name: str
    payload_expr: str


def log(message: str) -> None:
    print(f"[validate_contracts] {message}")


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def line_number_for_offset(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def iter_contract_schema_fragments(contract_name: str, contract_data: dict[str, Any]) -> list[tuple[str, dict[str, Any]]]:
    fragments: list[tuple[str, dict[str, Any]]] = []
    items = contract_data.get("items", [])
    if isinstance(items, list):
        for idx, item in enumerate(items):
            if not isinstance(item, dict):
                continue
            item_name = item.get("name", f"item[{idx}]")
            if contract_name == "tauri.commands":
                for key in ("params_schema", "result_schema"):
                    schema = item.get(key)
                    if isinstance(schema, dict):
                        fragments.append((f"items[{idx}] {item_name} {key}", schema))
            elif contract_name == "tauri.events":
                schema = item.get("payload_schema")
                if isinstance(schema, dict):
                    fragments.append((f"items[{idx}] {item_name} payload_schema", schema))
            elif contract_name == "sidecar.rpc":
                if item.get("type") == "method":
                    for key in ("params_schema", "result_schema"):
                        schema = item.get(key)
                        if isinstance(schema, dict):
                            fragments.append((f"items[{idx}] {item_name} {key}", schema))
                elif item.get("type") == "notification":
                    schema = item.get("params_schema")
                    if isinstance(schema, dict):
                        fragments.append((f"items[{idx}] {item_name} params_schema", schema))

    defs = contract_data.get("$defs")
    if not isinstance(defs, dict):
        defs = contract_data.get("definitions")
    if isinstance(defs, dict):
        for def_name, def_schema in sorted(defs.items()):
            if isinstance(def_schema, dict):
                fragments.append((f"defs.{def_name}", def_schema))
    return fragments


def validate_contract_schema_fragments(contracts: dict[str, dict[str, Any]]) -> list[str]:
    errors: list[str] = []
    for contract_name, contract_data in contracts.items():
        for fragment_label, schema in iter_contract_schema_fragments(contract_name, contract_data):
            try:
                Draft7Validator.check_schema(schema)
            except SchemaError as exc:
                errors.append(f"{contract_name} {fragment_label}: invalid Draft-07 schema: {exc.message}")
    return errors


def run_generator_and_diff(repo_root: Path, script_rel: str, target_rel: str) -> list[str]:
    errors: list[str] = []
    script_path = repo_root / script_rel
    target_path = repo_root / target_rel

    if not script_path.exists():
        return [f"missing generator script: {script_rel}"]
    if not target_path.exists():
        return [f"missing generated file: {target_rel}"]

    with tempfile.TemporaryDirectory() as tmpdir:
        tmp_out = Path(tmpdir) / Path(target_rel).name
        cmd = [sys.executable, str(script_path), "--repo-root", str(repo_root), "--out", str(tmp_out)]
        proc = subprocess.run(cmd, capture_output=True, text=True)
        if proc.returncode != 0:
            details = proc.stderr.strip() or proc.stdout.strip() or "no output"
            return [f"generator failed ({script_rel}): {details}"]

        generated = tmp_out.read_text(encoding="utf-8")
        committed = target_path.read_text(encoding="utf-8")
        if generated != committed:
            diff = "".join(
                difflib.unified_diff(
                    committed.splitlines(keepends=True),
                    generated.splitlines(keepends=True),
                    fromfile=f"{target_rel} (committed)",
                    tofile=f"{target_rel} (regenerated)",
                    n=2,
                )
            )
            preview = "\n".join(diff.splitlines()[:40])
            errors.append(f"{target_rel} is out of date with {script_rel}:\n{preview}")
    return errors


def validate_generated_files(repo_root: Path) -> list[str]:
    errors: list[str] = []
    for script_rel, target_rel in GENERATED_TARGETS:
        errors.extend(run_generator_and_diff(repo_root, script_rel, target_rel))
    return errors


def validate_derived_fixture_corpus(repo_root: Path) -> list[str]:
    derived_dir = repo_root / DERIVED_FIXTURE_DIR
    if not derived_dir.exists():
        return []
    if not any(path.is_file() and path.suffix == ".jsonl" for path in derived_dir.glob("*.jsonl")):
        return []

    script_path = repo_root / DERIVED_FIXTURE_CHECK_SCRIPT
    if not script_path.exists():
        return [f"missing derived fixture check script: {DERIVED_FIXTURE_CHECK_SCRIPT}"]

    proc = subprocess.run(
        [sys.executable, str(script_path), "--repo-root", str(repo_root), "--check"],
        capture_output=True,
        text=True,
    )
    if proc.returncode == 0:
        return []

    details = proc.stderr.strip() or proc.stdout.strip() or "no output"
    return [f"derived fixture corpus check failed: {details}"]


def run_generator_for_text(repo_root: Path, script_rel: str, out_path: Path) -> tuple[int, str]:
    script_path = repo_root / script_rel
    if not script_path.exists():
        return (1, f"missing generator script: {script_rel}")
    cmd = [sys.executable, str(script_path), "--repo-root", str(repo_root), "--out", str(out_path)]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        details = proc.stderr.strip() or proc.stdout.strip() or "no output"
        return (proc.returncode, f"generator failed ({script_rel}): {details}")
    return (0, out_path.read_text(encoding="utf-8"))


def validate_generator_determinism(repo_root: Path) -> list[str]:
    errors: list[str] = []
    seen_scripts: set[str] = set()
    for script_rel, _target_rel in GENERATED_TARGETS:
        if script_rel in seen_scripts:
            continue
        seen_scripts.add(script_rel)

        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            out_a = tmp / "a.out"
            out_b = tmp / "b.out"

            first_code, first_output = run_generator_for_text(repo_root, script_rel, out_a)
            if first_code != 0:
                errors.append(first_output)
                continue
            second_code, second_output = run_generator_for_text(repo_root, script_rel, out_b)
            if second_code != 0:
                errors.append(second_output)
                continue

            if first_output != second_output:
                diff = "".join(
                    difflib.unified_diff(
                        first_output.splitlines(keepends=True),
                        second_output.splitlines(keepends=True),
                        fromfile=f"{script_rel} run #1",
                        tofile=f"{script_rel} run #2",
                        n=2,
                    )
                )
                preview = "\n".join(diff.splitlines()[:40])
                errors.append(f"{script_rel}: non-deterministic output across runs:\n{preview}")

            if "Generated at" in first_output:
                errors.append(f"{script_rel}: output includes timestamp marker 'Generated at'")
            if str(repo_root.resolve()) in first_output:
                errors.append(f"{script_rel}: output includes absolute repository path")
            if "\r\n" in first_output:
                errors.append(f"{script_rel}: output contains CRLF; expected LF-only output")

            absolute_paths = sorted(set(ABSOLUTE_PATH_RE.findall(first_output)))
            if absolute_paths:
                preview_paths = ", ".join(absolute_paths[:3])
                errors.append(f"{script_rel}: output includes absolute path-like values: {preview_paths}")

    return errors


def parse_string_constants(text: str) -> dict[str, str]:
    values: dict[str, str] = {}
    for match in re.finditer(r"const\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([\"'])([^\"']+)\2", text):
        values[match.group(1)] = match.group(3)
    return values


def extract_balanced_braces(text: str, start_index: int) -> tuple[str, int]:
    depth = 0
    in_single = False
    in_double = False
    escape = False
    idx = start_index

    while idx < len(text):
        ch = text[idx]
        if escape:
            escape = False
            idx += 1
            continue
        if ch == "\\":
            escape = True
            idx += 1
            continue
        if not in_double and ch == "'":
            in_single = not in_single
            idx += 1
            continue
        if not in_single and ch == '"':
            in_double = not in_double
            idx += 1
            continue
        if in_single or in_double:
            idx += 1
            continue
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return text[start_index : idx + 1], idx + 1
        idx += 1
    raise ValueError("unbalanced braces")


def extract_balanced_parentheses(text: str, start_index: int) -> tuple[str, int]:
    depth = 0
    in_single = False
    in_double = False
    escape = False
    idx = start_index

    while idx < len(text):
        ch = text[idx]
        if escape:
            escape = False
            idx += 1
            continue
        if ch == "\\":
            escape = True
            idx += 1
            continue
        if not in_double and ch == "'":
            in_single = not in_single
            idx += 1
            continue
        if not in_single and ch == '"':
            in_double = not in_double
            idx += 1
            continue
        if in_single or in_double:
            idx += 1
            continue
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
            if depth == 0:
                return text[start_index : idx + 1], idx + 1
        idx += 1
    raise ValueError("unbalanced parentheses")


def split_top_level_commas(text: str) -> list[str]:
    parts: list[str] = []
    start = 0
    depth_paren = 0
    depth_brace = 0
    depth_bracket = 0
    in_single = False
    in_double = False
    escape = False

    for idx, ch in enumerate(text):
        if escape:
            escape = False
            continue
        if ch == "\\":
            escape = True
            continue
        if not in_double and ch == "'":
            in_single = not in_single
            continue
        if not in_single and ch == '"':
            in_double = not in_double
            continue
        if in_single or in_double:
            continue
        if ch == "(":
            depth_paren += 1
            continue
        if ch == ")":
            depth_paren = max(0, depth_paren - 1)
            continue
        if ch == "{":
            depth_brace += 1
            continue
        if ch == "}":
            depth_brace = max(0, depth_brace - 1)
            continue
        if ch == "[":
            depth_bracket += 1
            continue
        if ch == "]":
            depth_bracket = max(0, depth_bracket - 1)
            continue
        if ch == "," and depth_paren == 0 and depth_brace == 0 and depth_bracket == 0:
            parts.append(text[start:idx].strip())
            start = idx + 1

    tail = text[start:].strip()
    if tail:
        parts.append(tail)
    return parts


def parse_const_object_string_maps(text: str) -> dict[str, dict[str, str]]:
    maps: dict[str, dict[str, str]] = {}
    for match in re.finditer(r"const\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*{", text):
        obj_name = match.group(1)
        brace_start = match.end() - 1
        try:
            body, _ = extract_balanced_braces(text, brace_start)
        except ValueError:
            continue

        values: dict[str, str] = {}
        for entry in re.finditer(r"([A-Za-z_][A-Za-z0-9_]*)\s*:\s*([\"'])([^\"']+)\2", body):
            key = entry.group(1)
            value = entry.group(3)
            values[key] = value
        if values:
            maps[obj_name] = values
    return maps


def extract_listen_event_names_from_text(text: str) -> list[tuple[int, str, str | None]]:
    object_maps = parse_const_object_string_maps(text)
    string_constants = parse_string_constants(text)
    results: list[tuple[int, str, str | None]] = []

    listen_re = re.compile(
        r"\b(?:listen|registerListener)(?:<[^>]+>+)?\s*\(\s*([A-Za-z_][A-Za-z0-9_\.]*|[\"'][^\"']+[\"'])"
    )
    for match in listen_re.finditer(text):
        expr = match.group(1).strip()
        line = line_number_for_offset(text, match.start())
        event_name: str | None = None

        if (expr.startswith("'") and expr.endswith("'")) or (expr.startswith('"') and expr.endswith('"')):
            event_name = expr[1:-1]
        elif "." in expr:
            left, right = expr.split(".", 1)
            event_name = object_maps.get(left, {}).get(right)
        else:
            event_name = string_constants.get(expr)

        results.append((line, expr, event_name))
    return results


def allowed_tauri_event_names(events_contract: dict[str, Any]) -> tuple[set[str], dict[str, dict[str, Any]]]:
    allowed: set[str] = set()
    schema_map: dict[str, dict[str, Any]] = {}
    for item in events_contract.get("items", []):
        if not isinstance(item, dict) or item.get("type") != "event":
            continue
        name = item.get("name")
        payload_schema = item.get("payload_schema")
        if not isinstance(name, str) or not isinstance(payload_schema, dict):
            continue
        allowed.add(name)
        schema_map[name] = payload_schema
        aliases = item.get("deprecated_aliases", [])
        if isinstance(aliases, list):
            for alias in aliases:
                if isinstance(alias, str):
                    allowed.add(alias)
                    schema_map[alias] = payload_schema
    return allowed, schema_map


def tauri_event_name_maps(events_contract: dict[str, Any]) -> tuple[set[str], dict[str, str]]:
    canonical_names: set[str] = set()
    alias_to_canonical: dict[str, str] = {}
    for item in events_contract.get("items", []):
        if not isinstance(item, dict) or item.get("type") != "event":
            continue
        canonical = item.get("name")
        if not isinstance(canonical, str):
            continue
        canonical_names.add(canonical)
        aliases = item.get("deprecated_aliases", [])
        if not isinstance(aliases, list):
            continue
        for alias in aliases:
            if isinstance(alias, str):
                alias_to_canonical[alias] = canonical
    return canonical_names, alias_to_canonical


KNOWN_PASSTHROUGH_WRAPPERS: set[tuple[str, str]] = {
    # registerListener<T>(eventName, ...) wrapper in useTauriEvents hook
    ("src/hooks/useTauriEvents.ts", "eventName"),
    # useTauriEvent<T>(eventName, ...) generic hook
    ("src/hooks/useTauriEvents.ts", "eventName"),
    # subscribe<T>(eventName, ...) helper in OverlayApp
    ("src/overlay/OverlayApp.tsx", "eventName"),
}


def validate_frontend_listener_events(repo_root: Path, events_contract: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    allowed, _ = allowed_tauri_event_names(events_contract)
    canonical_names, alias_to_canonical = tauri_event_name_maps(events_contract)

    registrations: list[ListenerRegistration] = []
    for pattern in FRONTEND_GLOB_PATTERNS:
        for path in sorted(repo_root.glob(pattern)):
            if not path.is_file():
                continue
            if any(part in EXCLUDED_FRONTEND_PATH_PARTS for part in path.parts):
                continue
            if path.name.endswith(".test.ts") or path.name.endswith(".test.tsx"):
                continue

            text = path.read_text(encoding="utf-8")
            for line, expr, event_name in extract_listen_event_names_from_text(text):
                registrations.append(
                    ListenerRegistration(
                        file=path,
                        line=line,
                        expression=expr,
                        event_name=event_name,
                    )
                )

    if not registrations:
        log("FAIL: no frontend listen(...) registrations found for event-name validation")
        errors.append("no frontend listen(...) registrations found for event-name validation")
        return errors

    canonical_seen = {r.event_name for r in registrations if isinstance(r.event_name, str) and r.event_name in canonical_names}
    valid = 0
    legacy_alias_listeners = 0
    unresolved = 0
    failed = 0

    for reg in registrations:
        rel = reg.file.relative_to(repo_root)
        location = f"{rel}:{reg.line}"

        if reg.event_name is None:
            rel_key = (str(rel), reg.expression)
            if rel_key in KNOWN_PASSTHROUGH_WRAPPERS:
                log(f"OK: {location}: listener '{reg.expression}' is a known pass-through wrapper (skipped)")
                continue
            unresolved += 1
            log(f"FAIL: {location}: listener '{reg.expression}' could not be resolved statically")
            errors.append(
                f"{location}: listener '{reg.expression}' could not be resolved statically"
            )
            continue

        if reg.event_name not in allowed:
            failed += 1
            log(
                f"FAIL: {location}: listener '{reg.expression}' resolved to undeclared event '{reg.event_name}'"
            )
            errors.append(
                f"{location}: listener '{reg.expression}' resolved to undeclared event '{reg.event_name}'"
            )
            continue

        canonical_for_alias = alias_to_canonical.get(reg.event_name)
        if canonical_for_alias is not None:
            legacy_alias_listeners += 1
            valid += 1
            if canonical_for_alias not in canonical_seen:
                log(
                    f"WARN: {location}: listener '{reg.expression}' uses legacy alias "
                    f"'{reg.event_name}' without canonical listener '{canonical_for_alias}'"
                )
            else:
                log(
                    f"OK: {location}: listener '{reg.expression}' uses legacy alias "
                    f"'{reg.event_name}' (canonical listener '{canonical_for_alias}' is present)"
                )
            continue

        valid += 1
        log(f"OK: {location}: listener '{reg.expression}' resolved to '{reg.event_name}'")

    log(
        "Frontend listener validation summary: "
        f"{len(registrations)} listeners checked, "
        f"{valid} valid, "
        f"{legacy_alias_listeners} using legacy aliases, "
        f"{unresolved} unresolved, "
        f"{failed} failed"
    )
    return errors


def parse_rust_event_constants(text: str) -> dict[str, str]:
    constants: dict[str, str] = {}
    for match in re.finditer(r'const\s+(EVENT_[A-Z0-9_]+)\s*:\s*&str\s*=\s*"([^"]+)";', text):
        constants[match.group(1)] = match.group(2)
    return constants


def resolve_rust_event_token(token: str, constants: dict[str, str]) -> str | None:
    value = token.strip()
    if not value:
        return None
    if (value.startswith('"') and value.endswith('"')) or (value.startswith("'") and value.endswith("'")):
        return value[1:-1]
    if value in constants:
        return constants[value]
    return None


def parse_rust_event_list_expression(expr: str, constants: dict[str, str]) -> list[str]:
    value = expr.strip()
    if value.startswith("&"):
        value = value[1:].strip()
    if not (value.startswith("[") and value.endswith("]")):
        return []
    inner = value[1:-1].strip()
    if not inner:
        return []

    events: list[str] = []
    for token in split_top_level_commas(inner):
        resolved = resolve_rust_event_token(token, constants)
        if resolved:
            events.append(resolved)
    return events


def _find_cfg_test_line(text: str) -> int | None:
    """Return the line number of the first top-level #[cfg(test)] attribute, or None."""
    match = re.search(r"^#\[cfg\(test\)\]", text, re.MULTILINE)
    if match is None:
        return None
    return line_number_for_offset(text, match.start())


def extract_rust_emission_sites(repo_root: Path, rust_file: Path) -> list[RustEmissionSite]:
    if not rust_file.exists():
        return []

    text = rust_file.read_text(encoding="utf-8")
    constants = parse_rust_event_constants(text)
    cfg_test_line = _find_cfg_test_line(text)
    sites: list[RustEmissionSite] = []

    # Match emit_with_shared_seq, emit_with_shared_seq_for_broadcaster,
    # and emit_with_existing_seq_to_all_windows (all have handle, events, payload args)
    for match in re.finditer(r"\bemit_with_(?:shared_seq|shared_seq_for_broadcaster|existing_seq_to_all_windows)\s*\(", text):
        paren_start = text.find("(", match.start())
        if paren_start == -1:
            continue
        try:
            call_expr, _ = extract_balanced_parentheses(text, paren_start)
        except ValueError:
            continue
        args = split_top_level_commas(call_expr[1:-1])
        if len(args) < 3:
            continue

        events = parse_rust_event_list_expression(args[1], constants)
        payload_expr = args[2].strip()
        line = line_number_for_offset(text, match.start())
        for event_name in events:
            sites.append(
                RustEmissionSite(
                    file=rust_file,
                    line=line,
                    event_name=event_name,
                    payload_expr=payload_expr,
                )
            )

    # Match any_identifier.emit(...) — covers handle.emit, app_handle.emit, app.emit, self.emit
    for match in re.finditer(r"[A-Za-z_][A-Za-z0-9_]*\.emit\s*\(", text):
        paren_start = text.find("(", match.start())
        if paren_start == -1:
            continue
        try:
            call_expr, _ = extract_balanced_parentheses(text, paren_start)
        except ValueError:
            continue
        args = split_top_level_commas(call_expr[1:-1])
        if len(args) < 2:
            continue
        event_name = resolve_rust_event_token(args[0], constants)
        if event_name is None:
            continue
        payload_expr = args[1].strip()
        line = line_number_for_offset(text, match.start())
        sites.append(
            RustEmissionSite(
                file=rust_file,
                line=line,
                event_name=event_name,
                payload_expr=payload_expr,
            )
        )

    # Exclude emit sites inside #[cfg(test)] blocks (test-only payloads)
    if cfg_test_line is not None:
        sites = [s for s in sites if s.line < cfg_test_line]

    return sites


def parse_rust_struct_field_types(text: str, struct_name: str) -> dict[str, str]:
    match = re.search(rf"\bpub\s+struct\s+{re.escape(struct_name)}\s*{{", text)
    if not match:
        return {}
    brace_start = text.find("{", match.start())
    if brace_start == -1:
        return {}
    try:
        body, _ = extract_balanced_braces(text, brace_start)
    except ValueError:
        return {}

    fields: dict[str, str] = {}
    for field_match in re.finditer(r"\bpub\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*([^,\n]+),", body):
        fields[field_match.group(1)] = field_match.group(2).strip()
    return fields


def parse_rust_function_signature_arg_types(text: str, fn_name: str) -> dict[str, str]:
    match = re.search(rf"\bfn\s+{re.escape(fn_name)}\s*\(", text)
    if not match:
        return {}
    paren_start = text.find("(", match.start())
    if paren_start == -1:
        return {}
    try:
        sig, _ = extract_balanced_parentheses(text, paren_start)
    except ValueError:
        return {}

    arg_types: dict[str, str] = {}
    for arg in split_top_level_commas(sig[1:-1]):
        if ":" not in arg:
            continue
        left, right = arg.split(":", 1)
        name = left.strip().split()[-1].strip()
        if name in {"self", "&self", "&mut", "&mutself"}:
            continue
        if not re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", name):
            continue
        arg_types[name] = right.strip()
    return arg_types


def parse_rust_function_return_type(text: str, fn_name: str) -> str | None:
    match = re.search(rf"\bfn\s+{re.escape(fn_name)}\s*\(", text)
    if not match:
        return None
    paren_start = text.find("(", match.start())
    if paren_start == -1:
        return None
    try:
        _sig, paren_end = extract_balanced_parentheses(text, paren_start)
    except ValueError:
        return None

    brace_start = text.find("{", paren_end)
    if brace_start == -1:
        return None
    between = text[paren_end + 1 : brace_start]
    arrow_index = between.find("->")
    if arrow_index == -1:
        return None

    return_type = between[arrow_index + 2 :].strip()
    if not return_type:
        return None
    return return_type


def normalize_rust_type_struct_name(rust_type: str) -> str | None:
    raw = re.sub(r"\s+", "", rust_type).lstrip("&")
    option_prefixes = ("Option<", "std::option::Option<")
    while any(raw.startswith(prefix) and raw.endswith(">") for prefix in option_prefixes):
        if raw.startswith("Option<"):
            raw = raw[len("Option<") : -1]
        elif raw.startswith("std::option::Option<"):
            raw = raw[len("std::option::Option<") : -1]

    if "<" in raw:
        raw = raw.split("<", 1)[0]
    if "::" in raw:
        raw = raw.split("::")[-1]
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", raw):
        return None
    return raw


def parse_rust_function_body(text: str, fn_name: str) -> str | None:
    match = re.search(rf"\bfn\s+{re.escape(fn_name)}\s*\(", text)
    if not match:
        return None
    brace_start = text.find("{", match.end())
    if brace_start == -1:
        return None
    try:
        body, _ = extract_balanced_braces(text, brace_start)
    except ValueError:
        return None
    return body


def rust_type_to_json_type(rust_type: str) -> str | None:
    raw = rust_type.strip()
    raw = re.sub(r"\s+", "", raw)
    raw = raw.removeprefix("&")

    if raw.startswith("Option<") and raw.endswith(">"):
        inner = raw[len("Option<") : -1]
        return rust_type_to_json_type(inner)

    if raw in {"String", "str"}:
        return "string"
    if raw in {"bool"}:
        return "boolean"
    if raw in {"u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize"}:
        return "integer"
    if raw in {"f32", "f64"}:
        return "number"
    if raw in {"Value", "serde_json::Value"}:
        return None
    return "object"


def infer_json_type_from_rust_expr(expr: str, arg_types: dict[str, str]) -> str | None:
    value = expr.strip()
    if not value:
        return None

    if (value.startswith('"') and value.endswith('"')) or (value.startswith("'") and value.endswith("'")):
        return "string"
    if value in {"true", "false"}:
        return "boolean"
    if re.fullmatch(r"-?[0-9]+", value):
        return "integer"
    if re.fullmatch(r"-?[0-9]+\.[0-9]+", value):
        return "number"
    if value.endswith(".to_rfc3339()") or value.endswith(".to_string()"):
        return "string"

    if value in arg_types:
        return rust_type_to_json_type(arg_types[value])

    head = value.split(".", 1)[0]
    tail = value.split(".", 1)[1] if "." in value else ""
    if head in arg_types:
        inferred = rust_type_to_json_type(arg_types[head])
        if inferred:
            # Prefer direct type mapping for primitive args.
            if inferred != "object":
                return inferred
        if tail.startswith("message"):
            return "string"
        if tail.startswith("recoverable"):
            return "boolean"
        if tail.startswith("state"):
            return "string"
        if tail.startswith("enabled"):
            return "boolean"
        if tail.startswith("detail"):
            return "string"

    return None


def parse_json_object_key_types(obj_text: str, arg_types: dict[str, str]) -> dict[str, str | None]:
    text = obj_text.strip()
    if not (text.startswith("{") and text.endswith("}")):
        return {}
    inner = text[1:-1].strip()
    inner = re.sub(r"//[^\n]*", "", inner)
    if not inner:
        return {}

    key_types: dict[str, str | None] = {}
    for entry in split_top_level_commas(inner):
        if ":" not in entry:
            continue
        key_raw, value_raw = entry.split(":", 1)
        key_match = re.match(r'\s*"([A-Za-z0-9_]+)"\s*$', key_raw.strip())
        if not key_match:
            continue
        key = key_match.group(1)
        key_types[key] = infer_json_type_from_rust_expr(value_raw, arg_types)
    return key_types


def infer_payload_shape_from_rust_function(
    rust_text: str,
    fn_name: str,
) -> tuple[str, dict[str, str | None]] | None:
    body = parse_rust_function_body(rust_text, fn_name)
    if body is None:
        return None
    arg_types = parse_rust_function_signature_arg_types(rust_text, fn_name)

    key_types: dict[str, str | None] = {}
    for macro_match in re.finditer(r"json!\s*\(\s*{", body):
        brace_start = body.find("{", macro_match.start())
        if brace_start == -1:
            continue
        try:
            obj_text, _ = extract_balanced_braces(body, brace_start)
        except ValueError:
            continue
        key_types.update(parse_json_object_key_types(obj_text, arg_types))

    for insert_match in re.finditer(
        r'insert\(\s*"([A-Za-z0-9_]+)"\.to_string\(\)\s*,\s*json!\((.*?)\)\s*\)',
        body,
        re.S,
    ):
        key = insert_match.group(1)
        value_expr = insert_match.group(2).strip()
        key_types[key] = infer_json_type_from_rust_expr(value_expr, arg_types)

    if not key_types:
        return_type = parse_rust_function_return_type(rust_text, fn_name)
        if return_type:
            struct_name = normalize_rust_type_struct_name(return_type)
            if struct_name:
                struct_shape = infer_payload_shape_from_rust_struct(rust_text, struct_name)
                if struct_shape is not None:
                    return (f"fn_return:{fn_name}->{struct_name}", struct_shape[1])
        return None
    return (f"fn:{fn_name}", key_types)


def infer_payload_shape_from_rust_struct(
    rust_text: str,
    struct_name: str,
) -> tuple[str, dict[str, str | None]] | None:
    field_types = parse_rust_struct_field_types(rust_text, struct_name)
    if not field_types:
        return None
    key_types = {field: rust_type_to_json_type(rust_type) for field, rust_type in field_types.items()}
    return (f"struct:{struct_name}", key_types)


def find_identifier_assignment_source(text: str, identifier: str, max_line: int) -> str | None:
    pattern = re.compile(rf"\blet\s+(?:mut\s+)?{re.escape(identifier)}\s*=\s*(.+?);", re.S)
    candidates: list[tuple[int, str]] = []
    for match in pattern.finditer(text):
        line = line_number_for_offset(text, match.start())
        if line <= max_line:
            candidates.append((line, match.group(1).strip()))
    if not candidates:
        return None
    _line, rhs = max(candidates, key=lambda item: item[0])
    return rhs


def resolve_schema_fragment(schema: dict[str, Any], root_schema: dict[str, Any]) -> dict[str, Any]:
    current = schema
    seen: set[str] = set()
    while isinstance(current, dict) and "$ref" in current:
        ref = current.get("$ref")
        if not isinstance(ref, str) or not ref.startswith("#/") or ref in seen:
            break
        seen.add(ref)
        node: Any = root_schema
        for token in ref[2:].split("/"):
            if not isinstance(node, dict) or token not in node:
                node = None
                break
            node = node[token]
        if not isinstance(node, dict):
            break
        current = node
    return current if isinstance(current, dict) else schema


def schema_allows_json_type(schema: dict[str, Any], root_schema: dict[str, Any], json_type: str) -> bool:
    resolved = resolve_schema_fragment(schema, root_schema)
    if "oneOf" in resolved and isinstance(resolved["oneOf"], list):
        return any(
            isinstance(option, dict) and schema_allows_json_type(option, root_schema, json_type)
            for option in resolved["oneOf"]
        )
    if "anyOf" in resolved and isinstance(resolved["anyOf"], list):
        return any(
            isinstance(option, dict) and schema_allows_json_type(option, root_schema, json_type)
            for option in resolved["anyOf"]
        )
    if "type" in resolved:
        schema_type = resolved["type"]
        if isinstance(schema_type, str):
            return schema_type == json_type or (schema_type == "integer" and json_type == "number")
        if isinstance(schema_type, list):
            return json_type in schema_type or (json_type == "number" and "integer" in schema_type)
    if "const" in resolved:
        const_val = resolved["const"]
        if isinstance(const_val, bool):
            return json_type == "boolean"
        if isinstance(const_val, int) and not isinstance(const_val, bool):
            return json_type == "integer"
        if isinstance(const_val, float):
            return json_type in {"number", "integer"}
        if isinstance(const_val, str):
            return json_type == "string"
    if "enum" in resolved and isinstance(resolved["enum"], list) and resolved["enum"]:
        sample = resolved["enum"][0]
        if isinstance(sample, bool):
            return json_type == "boolean"
        if isinstance(sample, int) and not isinstance(sample, bool):
            return json_type == "integer"
        if isinstance(sample, float):
            return json_type in {"number", "integer"}
        if isinstance(sample, str):
            return json_type == "string"
    return True


def infer_rust_payload_shape(
    rust_text: str,
    site: RustEmissionSite,
    site_file_text: str | None = None,
) -> tuple[str, dict[str, str | None]] | None:
    # Use file-specific text for line-relative assignment lookups, global text for definitions
    local_text = site_file_text if site_file_text is not None else rust_text
    expr = site.payload_expr.strip()
    fn_match = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\s*\(", expr)
    if fn_match:
        fn_name = fn_match.group(1)
        function_shape = infer_payload_shape_from_rust_function(rust_text, fn_name)
        if function_shape is not None:
            return function_shape

    if expr == "canonical_payload":
        shape = infer_payload_shape_from_rust_function(rust_text, "sidecar_status_payload_from_status_event")
        if shape is not None:
            return shape

    if expr.startswith("json!(") and expr.endswith(")"):
        inner = expr[len("json!(") : -1].strip()
        if inner.startswith("{") and inner.endswith("}"):
            return ("inline:json-object", parse_json_object_key_types(inner, {}))
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", inner):
            rhs = find_identifier_assignment_source(local_text, inner, site.line)
            if rhs:
                fn_match_rhs = re.match(r"(?:self\.)?([A-Za-z_][A-Za-z0-9_]*)\s*\(", rhs)
                if fn_match_rhs:
                    fn_name_rhs = fn_match_rhs.group(1)
                    shape = infer_payload_shape_from_rust_function(rust_text, fn_name_rhs)
                    if shape is not None:
                        return shape
                if rhs.startswith("model_status_event_payload("):
                    shape = infer_payload_shape_from_rust_struct(rust_text, "ModelStatusPayload")
                    if shape is not None:
                        return shape
                if rhs.startswith("ModelProgress"):
                    shape = infer_payload_shape_from_rust_struct(rust_text, "ModelProgress")
                    if shape is not None:
                        return shape

    # Bare identifier: trace assignment in the same file and try to resolve the RHS
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", expr):
        rhs = find_identifier_assignment_source(local_text, expr, site.line)
        if rhs:
            fn_match_rhs = re.match(r"(?:self\.)?([A-Za-z_][A-Za-z0-9_]*)\s*\(", rhs)
            if fn_match_rhs:
                fn_name = fn_match_rhs.group(1)
                shape = infer_payload_shape_from_rust_function(rust_text, fn_name)
                if shape is not None:
                    return shape
            if rhs.startswith("json!(") and rhs.endswith(")"):
                inner = rhs[len("json!(") : -1].strip()
                if inner.startswith("{") and inner.endswith("}"):
                    return ("inline:json-object", parse_json_object_key_types(inner, {}))

    return None


def validate_rust_event_payloads(repo_root: Path, events_contract: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    rust_src_dir = repo_root / "src-tauri" / "src"
    if not rust_src_dir.is_dir():
        return [f"missing Rust source directory: {rust_src_dir.relative_to(repo_root)}"]

    rust_files = sorted(rust_src_dir.glob("*.rs"))
    if not rust_files:
        return [f"{rust_src_dir.relative_to(repo_root)}: no Rust source files found"]

    # Per-file text for line-relative assignment lookups; concatenated for cross-file definitions
    file_texts: dict[Path, str] = {f: f.read_text(encoding="utf-8") for f in rust_files}
    rust_text = "\n".join(file_texts.values())

    sites: list[RustEmissionSite] = []
    for rust_file in rust_files:
        sites.extend(extract_rust_emission_sites(repo_root, rust_file))
    if not sites:
        return [f"{rust_src_dir.relative_to(repo_root)}: no Rust emission sites found"]

    canonical_names, _alias_map = tauri_event_name_maps(events_contract)
    canonical_schema_map: dict[str, dict[str, Any]] = {}
    for item in events_contract.get("items", []):
        if not isinstance(item, dict) or item.get("type") != "event":
            continue
        name = item.get("name")
        payload_schema = item.get("payload_schema")
        if isinstance(name, str) and isinstance(payload_schema, dict):
            canonical_schema_map[name] = payload_schema

    checked = 0
    passed = 0
    for site in sites:
        if site.event_name not in canonical_names:
            continue
        schema = canonical_schema_map.get(site.event_name)
        if schema is None:
            continue
        checked += 1
        rel = site.file.relative_to(repo_root)
        log(
            f"Rust emit site: {rel}:{site.line}: event '{site.event_name}' payload expr '{site.payload_expr}'"
        )

        site_file_text = file_texts.get(site.file, rust_text)
        inferred = infer_rust_payload_shape(rust_text, site, site_file_text)
        if inferred is None:
            errors.append(
                f"{rel}:{site.line}: unable to infer Rust payload shape for event '{site.event_name}'"
            )
            log(
                f"FAIL: {rel}:{site.line}: unable to infer Rust payload shape for event '{site.event_name}'"
            )
            continue

        source_name, key_types = inferred
        resolved_schema = resolve_schema_fragment(schema, events_contract)
        properties = resolved_schema.get("properties", {})
        if not isinstance(properties, dict):
            properties = {}
        required = resolved_schema.get("required", [])
        required_no_seq = {
            name for name in required if isinstance(name, str) and name != "seq"
        }
        emitted_keys = set(key_types.keys())

        site_errors: list[str] = []
        missing = sorted(required_no_seq - emitted_keys)
        if missing:
            site_errors.append(
                f"missing required field(s): {', '.join(missing)}"
            )

        if resolved_schema.get("additionalProperties") is False:
            extras = sorted(name for name in emitted_keys if name not in properties)
            if extras:
                site_errors.append(
                    f"unexpected field(s) not in schema: {', '.join(extras)}"
                )

        for key, inferred_type in key_types.items():
            if inferred_type is None:
                continue
            prop_schema = properties.get(key)
            if not isinstance(prop_schema, dict):
                continue
            if not schema_allows_json_type(prop_schema, events_contract, inferred_type):
                site_errors.append(
                    f"field '{key}' inferred as {inferred_type}, incompatible with schema"
                )

        if site_errors:
            for detail in site_errors:
                errors.append(
                    f"{rel}:{site.line}: event '{site.event_name}' via {source_name}: {detail}"
                )
            log(
                f"FAIL: {rel}:{site.line}: event '{site.event_name}' via {source_name} "
                f"({len(site_errors)} issue(s))"
            )
            continue

        passed += 1
        log(
            f"OK: {rel}:{site.line}: event '{site.event_name}' via {source_name}"
        )

    if checked == 0:
        errors.append(
            f"{rust_src_dir.relative_to(repo_root)}: no canonical tauri.events emissions found for validation"
        )
    log(f"Rust payload validation summary: {checked} checked, {passed} passed, {len(errors)} failed")
    return errors


def validate_instance_against_schema(
    instance: Any,
    schema: dict[str, Any],
    root_schema: dict[str, Any],
    prefix: str,
) -> list[str]:
    resolver = RefResolver.from_schema(root_schema)
    validator = Draft7Validator(schema, resolver=resolver)
    errors: list[str] = []
    for err in sorted(validator.iter_errors(instance), key=lambda e: list(e.path)):
        location = prefix
        if err.path:
            location = f"{prefix}." + ".".join(str(p) for p in err.path)
        errors.append(f"{location}: {err.message}")
    return errors


def parse_jsonl(path: Path) -> list[tuple[int, dict[str, Any]]]:
    rows: list[tuple[int, dict[str, Any]]] = []
    for idx, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line:
            continue
        rows.append((idx, json.loads(line)))
    return rows


def infer_method_for_response(
    msg_id: Any,
    comment: str,
    request_method_by_id: dict[Any, str],
    known_methods: set[str],
) -> str | None:
    method_name = request_method_by_id.get(msg_id)
    if method_name:
        return method_name
    match = METHOD_NAME_RE.search(comment)
    if not match:
        return None
    candidate = match.group(1)
    if candidate in known_methods:
        return candidate
    return None


def sidecar_contract_maps(sidecar_contract: dict[str, Any]) -> tuple[dict[str, dict[str, Any]], dict[str, dict[str, Any]], set[str]]:
    methods: dict[str, dict[str, Any]] = {}
    notifications: dict[str, dict[str, Any]] = {}
    required: set[str] = set()
    for item in sidecar_contract.get("items", []):
        if not isinstance(item, dict):
            continue
        item_type = item.get("type")
        name = item.get("name")
        if not isinstance(name, str):
            continue
        if item_type == "method":
            params_schema = item.get("params_schema", {})
            result_schema = item.get("result_schema", {})
            if not isinstance(params_schema, dict) or not isinstance(result_schema, dict):
                continue
            methods[name] = {"params_schema": params_schema, "result_schema": result_schema}
            if item.get("required") is True:
                required.add(name)
        elif item_type == "notification":
            params_schema = item.get("params_schema", {})
            if isinstance(params_schema, dict):
                notifications[name] = {"params_schema": params_schema}
    return methods, notifications, required


def extract_sidecar_handler_methods(server_text: str) -> set[str]:
    # Accept HANDLERS with any type annotation (dict[...], Dict[...]) or none at all
    match = re.search(r"^HANDLERS\s*(?::[^=]*)?\s*=\s*{", server_text, re.MULTILINE)
    if not match:
        return set()
    brace_start = server_text.find("{", match.start())
    if brace_start == -1:
        return set()
    try:
        body, _ = extract_balanced_braces(server_text, brace_start)
    except ValueError:
        return set()

    methods: set[str] = set()
    for entry in re.finditer(r'["\']([^"\']+)["\']\s*:', body):
        methods.add(entry.group(1))
    return methods


def validate_sidecar_handler_dispatch(repo_root: Path, sidecar_contract: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    server_file = repo_root / "sidecar" / "src" / "openvoicy_sidecar" / "server.py"
    if not server_file.exists():
        return [f"missing sidecar server source: {server_file.relative_to(repo_root)}"]

    server_text = server_file.read_text(encoding="utf-8")
    registered_methods = extract_sidecar_handler_methods(server_text)
    if not registered_methods:
        return [f"{server_file.relative_to(repo_root)}: unable to extract HANDLERS dispatch table"]

    methods, _notifications, required_methods = sidecar_contract_maps(sidecar_contract)
    required_sorted = sorted(required_methods)
    found = 0
    for method_name in required_sorted:
        if method_name in registered_methods:
            found += 1
            log(f"OK: sidecar handler dispatch includes required method '{method_name}'")
        else:
            log(f"FAIL: sidecar handler dispatch missing required method '{method_name}'")
            errors.append(
                f"{server_file.relative_to(repo_root)}: required sidecar method '{method_name}' missing from HANDLERS dispatch table"
            )

    unknown_handlers = sorted(name for name in registered_methods if name not in methods)
    for unknown in unknown_handlers:
        log(
            f"FAIL: sidecar HANDLERS includes method '{unknown}' not declared in sidecar.rpc contract"
        )
        errors.append(
            f"{server_file.relative_to(repo_root)}: HANDLERS method '{unknown}' not declared in sidecar.rpc contract"
        )

    log(
        "Sidecar handler dispatch summary: "
        f"{len(required_sorted)} required methods, {found} found, {len(required_sorted) - found} missing, "
        f"{len(unknown_handlers)} undeclared"
    )
    return errors


def validate_sidecar_examples_against_contract(repo_root: Path, sidecar_contract: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    examples_file = repo_root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
    if not examples_file.exists():
        return [f"missing examples file: {examples_file.relative_to(repo_root)}"]

    methods, notifications, required_methods = sidecar_contract_maps(sidecar_contract)
    known_methods = set(methods.keys())
    rows = parse_jsonl(examples_file)

    request_method_by_id: dict[Any, str] = {}
    seen_request_methods: set[str] = set()
    for _line, obj in rows:
        if obj.get("type") != "request":
            continue
        data = obj.get("data", {})
        msg_id = data.get("id")
        method = data.get("method")
        if isinstance(method, str):
            seen_request_methods.add(method)
            if msg_id is not None:
                request_method_by_id[msg_id] = method

    for line, obj in rows:
        msg_type = obj.get("type")
        data = obj.get("data", {})
        comment = str(obj.get("_comment", ""))

        if msg_type == "request":
            method = data.get("method")
            if not isinstance(method, str):
                continue
            if method not in methods:
                log(
                    f"WARN: shared/ipc/examples/IPC_V1_EXAMPLES.jsonl:{line}: unknown request method '{method}'"
                )
                continue
            params = data.get("params", {})
            params_schema = methods[method]["params_schema"]
            for err in validate_instance_against_schema(
                params,
                params_schema,
                sidecar_contract,
                f"{method}.params",
            ):
                errors.append(f"shared/ipc/examples/IPC_V1_EXAMPLES.jsonl:{line}: {err}")

        elif msg_type == "response":
            method = infer_method_for_response(data.get("id"), comment, request_method_by_id, known_methods)
            if method is None:
                continue
            if method not in methods:
                continue
            if "result" not in data:
                continue
            result_schema = methods[method]["result_schema"]
            for err in validate_instance_against_schema(
                data["result"],
                result_schema,
                sidecar_contract,
                f"{method}.result",
            ):
                errors.append(f"shared/ipc/examples/IPC_V1_EXAMPLES.jsonl:{line}: {err}")

        elif msg_type == "notification":
            method = data.get("method")
            if not isinstance(method, str):
                continue
            if method not in notifications:
                log(
                    f"WARN: shared/ipc/examples/IPC_V1_EXAMPLES.jsonl:{line}: unknown notification method '{method}'"
                )
                continue
            params = data.get("params", {})
            schema = notifications[method]["params_schema"]
            for err in validate_instance_against_schema(
                params,
                schema,
                sidecar_contract,
                f"{method}.params",
            ):
                errors.append(f"shared/ipc/examples/IPC_V1_EXAMPLES.jsonl:{line}: {err}")

    missing_required = sorted(required_methods - seen_request_methods)
    for method in missing_required:
        errors.append(
            "shared/ipc/examples/IPC_V1_EXAMPLES.jsonl: "
            f"missing request fixture for required sidecar method '{method}'"
        )

    return errors


def coerce_js_object_literal_to_json_text(expr: str) -> str:
    text = expr
    text = re.sub(r"\bas const\b", "", text)
    text = re.sub(r"\bundefined\b", "null", text)
    text = re.sub(r",\s*([}\]])", r"\1", text)
    text = re.sub(r"([{\[,]\s*)([A-Za-z_][A-Za-z0-9_]*)(\s*:)", r'\1"\2"\3', text)
    text = text.replace("'", '"')
    return text


def extract_event_payload_examples_from_test_file(path: Path) -> list[tuple[int, str, str]]:
    text = path.read_text(encoding="utf-8")
    pattern = re.compile(r"(emitMockEvent|fireMockEventWithLog)\s*\(\s*([\"'])([^\"']+)\2\s*,\s*{")
    examples: list[tuple[int, str, str]] = []

    for match in pattern.finditer(text):
        line_start = text.rfind("\n", 0, match.start()) + 1
        line_end = text.find("\n", line_start)
        if line_end == -1:
            line_end = len(text)
        call_line = text[line_start:line_end]
        before_call = call_line[: max(0, match.start() - line_start)]
        if EVENT_PAYLOAD_IGNORE_MARKER in before_call:
            continue

        prev_line_end = max(0, line_start - 1)
        prev_line_start = text.rfind("\n", 0, prev_line_end) + 1
        prev_line = text[prev_line_start:prev_line_end].strip()
        if EVENT_PAYLOAD_IGNORE_MARKER in prev_line:
            continue

        event_name = match.group(3)
        brace_start = match.end() - 1
        try:
            obj_text, _ = extract_balanced_braces(text, brace_start)
        except ValueError:
            continue
        line = line_number_for_offset(text, match.start())
        examples.append((line, event_name, obj_text))
    return examples


def validate_tauri_event_payload_examples(repo_root: Path, events_contract: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    _, schema_map = allowed_tauri_event_names(events_contract)
    fixture_file = repo_root / "src" / "hooks" / "useTauriEvents.test.ts"
    if not fixture_file.exists():
        return [f"missing event payload examples source: {fixture_file.relative_to(repo_root)}"]

    extracted = extract_event_payload_examples_from_test_file(fixture_file)
    if not extracted:
        return [f"{fixture_file.relative_to(repo_root)}: no emitMockEvent/fireMockEventWithLog payload examples found"]

    seen_event_names: set[str] = set()
    for line, event_name, payload_expr in extracted:
        seen_event_names.add(event_name)
        schema = schema_map.get(event_name)
        if schema is None:
            errors.append(f"{fixture_file.relative_to(repo_root)}:{line}: event '{event_name}' not declared in tauri.events contract")
            continue
        try:
            payload_json_text = coerce_js_object_literal_to_json_text(payload_expr)
            payload = json.loads(payload_json_text)
        except Exception as exc:
            errors.append(f"{fixture_file.relative_to(repo_root)}:{line}: unable to parse payload literal for '{event_name}': {exc}")
            continue

        for err in validate_instance_against_schema(
            payload,
            schema,
            events_contract,
            f"{event_name}.payload",
        ):
            errors.append(f"{fixture_file.relative_to(repo_root)}:{line}: {err}")

    for err in validate_legacy_alias_fixture_coverage(events_contract, seen_event_names):
        errors.append(f"{fixture_file.relative_to(repo_root)}: {err}")

    return errors


def validate_legacy_alias_fixture_coverage(
    events_contract: dict[str, Any],
    seen_event_names: set[str],
) -> list[str]:
    errors: list[str] = []
    for item in events_contract.get("items", []):
        if not isinstance(item, dict) or item.get("type") != "event":
            continue
        canonical = item.get("name")
        aliases = item.get("deprecated_aliases")
        if not isinstance(canonical, str) or not isinstance(aliases, list) or not aliases:
            continue

        if canonical not in seen_event_names:
            errors.append(
                f"missing payload fixture for canonical event '{canonical}' "
                f"(deprecated aliases: {', '.join(str(a) for a in aliases if isinstance(a, str))})"
            )

        for alias in aliases:
            if not isinstance(alias, str):
                continue
            if alias not in seen_event_names:
                errors.append(f"missing payload fixture for deprecated alias event '{alias}' (canonical: '{canonical}')")
    return errors


def run_self_test() -> list[str]:
    errors: list[str] = []

    sample_text = """
const EVENTS = {
  STATE_CHANGED: 'state:changed',
  STATE_CHANGED_LEGACY: 'state_changed',
} as const;
listen(EVENTS.STATE_CHANGED, () => {});
listen('sidecar:status', () => {});
    """.strip()
    parsed = extract_listen_event_names_from_text(sample_text)
    resolved = [name for _line, _expr, name in parsed if name]
    if "state:changed" not in resolved or "sidecar:status" not in resolved:
        errors.append("self-test: listener extraction failed")

    schema = {"type": "object", "required": ["state"], "properties": {"state": {"type": "string"}}}
    root = {"$schema": "http://json-schema.org/draft-07/schema#", "type": "object"}
    instance_ok = {"state": "idle"}
    instance_bad = {}
    if validate_instance_against_schema(instance_ok, schema, root, "x"):
        errors.append("self-test: expected valid instance to pass")
    if not validate_instance_against_schema(instance_bad, schema, root, "x"):
        errors.append("self-test: expected invalid instance to fail")

    payload_text = "{ seq: 1, state: 'idle', enabled: true }"
    try:
        parsed_payload = json.loads(coerce_js_object_literal_to_json_text(payload_text))
        if parsed_payload.get("state") != "idle":
            errors.append("self-test: payload parsing returned wrong value")
    except Exception as exc:
        errors.append(f"self-test: payload parsing failed: {exc}")

    return errors


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Validate contract schemas, generated outputs, and fixture parity.")
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT, help="Repository root path.")
    parser.add_argument("--self-test", action="store_true", help="Run internal validator self-tests only.")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    repo_root = args.repo_root.resolve()

    try:
        if args.self_test:
            log("Running self-test checks")
            self_test_errors = run_self_test()
            if self_test_errors:
                for err in self_test_errors:
                    print(f"ERROR: {err}")
                return 1
            log("Self-test passed")
            return 0

        log("Loading contract JSON files")
        contracts = {name: read_json(path) for name, path in CONTRACT_PATHS.items()}

        checks: list[tuple[str, list[str]]] = []
        checks.append(("Draft-07 schema fragment validation", validate_contract_schema_fragments(contracts)))
        checks.append(("Generator determinism (stable output; no timestamps or paths)", validate_generator_determinism(repo_root)))
        checks.append(("Generated artifacts up-to-date", validate_generated_files(repo_root)))
        checks.append(
            (
                "Derived fixture corpus generated from canonical IPC examples",
                validate_derived_fixture_corpus(repo_root),
            )
        )
        checks.append(
            (
                "Frontend listener event names declared in contract",
                validate_frontend_listener_events(repo_root, contracts["tauri.events"]),
            )
        )
        checks.append(
            (
                "Rust emitted event payloads match tauri.events contract",
                validate_rust_event_payloads(repo_root, contracts["tauri.events"]),
            )
        )
        checks.append(
            (
                "Required sidecar methods implemented in server dispatch",
                validate_sidecar_handler_dispatch(repo_root, contracts["sidecar.rpc"]),
            )
        )
        checks.append(
            (
                "Sidecar examples match sidecar RPC contract",
                validate_sidecar_examples_against_contract(repo_root, contracts["sidecar.rpc"]),
            )
        )
        checks.append(
            (
                "Event payload examples match tauri.events contract",
                validate_tauri_event_payload_examples(repo_root, contracts["tauri.events"]),
            )
        )

        total = len(checks)
        failures = 0
        all_errors: list[str] = []

        for check_name, errors in checks:
            if errors:
                failures += 1
                log(f"FAIL: {check_name} ({len(errors)} error(s))")
                for err in errors:
                    all_errors.append(f"{check_name}: {err}")
            else:
                log(f"PASS: {check_name}")

        log(f"Checks performed: {total}, passed: {total - failures}, failed: {failures}")
        if all_errors:
            for err in all_errors:
                print(f"ERROR: {err}")
            return 1
        return 0
    except Exception as exc:  # pragma: no cover - top-level safety net
        print(f"ERROR: script failure: {exc}")
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
