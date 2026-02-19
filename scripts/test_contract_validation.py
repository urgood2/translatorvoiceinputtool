#!/usr/bin/env python3
"""Comprehensive contract validation suite with category logging.

This script complements scripts/validate_contracts.py by emitting explicit
category summaries for schema, fixture, generator, cross-reference, and
drift-prevention checks.
"""

from __future__ import annotations

import argparse
import json
import tempfile
from pathlib import Path
from typing import Any

import validate_contracts as vc


REPO_ROOT = Path(__file__).resolve().parents[1]
EXPECTED_EVENT_ALIASES = {
    "state:changed": "state_changed",
    "transcript:complete": "transcription:complete",
    "transcript:error": "transcription:error",
    "sidecar:status": "status:changed",
}


def log(message: str) -> None:
    print(f"[CONTRACT_VALIDATION] {message}")


def iter_key_values(node: Any, key: str) -> list[Any]:
    values: list[Any] = []
    if isinstance(node, dict):
        if key in node:
            values.append(node[key])
        for value in node.values():
            values.extend(iter_key_values(value, key))
    elif isinstance(node, list):
        for item in node:
            values.extend(iter_key_values(item, key))
    return values


def load_contracts(repo_root: Path) -> dict[str, dict[str, Any]]:
    contracts_root = repo_root / "shared" / "contracts"
    return {
        "tauri.commands": vc.read_json(contracts_root / "tauri.commands.v1.json"),
        "tauri.events": vc.read_json(contracts_root / "tauri.events.v1.json"),
        "sidecar.rpc": vc.read_json(contracts_root / "sidecar.rpc.v1.json"),
    }


def validate_schema_category(contracts: dict[str, dict[str, Any]]) -> tuple[list[str], list[str]]:
    summaries: list[str] = []
    errors: list[str] = []

    errors.extend(vc.validate_contract_schema_fragments(contracts))

    for contract_name, contract in contracts.items():
        version = contract.get("version")
        if version != 1:
            errors.append(f"{contract_name}: expected version=1, got {version!r}")

        items = contract.get("items")
        if not isinstance(items, list):
            errors.append(f"{contract_name}: missing items[] array")
            continue
        if not items:
            errors.append(f"{contract_name}: items[] is empty")
            continue

        names = [item.get("name") for item in items if isinstance(item, dict)]
        if not all(isinstance(name, str) and name for name in names):
            errors.append(f"{contract_name}: one or more items have missing/empty name")
        if len(names) != len(set(names)):
            errors.append(f"{contract_name}: duplicate item names detected")

        ref_values = [value for value in iter_key_values(contract, "$ref") if isinstance(value, str)]
        bad_refs = [value for value in ref_values if not value.startswith("#/")]
        if bad_refs:
            errors.append(f"{contract_name}: found non-local $ref values: {', '.join(sorted(set(bad_refs))[:3])}")

        id_values = [value for value in iter_key_values(contract, "$id") if isinstance(value, str)]
        remote_ids = [value for value in id_values if value.startswith("http://") or value.startswith("https://")]
        if remote_ids:
            errors.append(f"{contract_name}: found remote $id values: {', '.join(sorted(set(remote_ids))[:3])}")

        summaries.append(f"{contract_name}: VALID ({len(items)} items)")

    alias_map: dict[str, list[str]] = {}
    for item in contracts["tauri.events"].get("items", []):
        if isinstance(item, dict) and isinstance(item.get("name"), str):
            aliases = item.get("deprecated_aliases")
            alias_map[item["name"]] = aliases if isinstance(aliases, list) else []

    for canonical, alias in EXPECTED_EVENT_ALIASES.items():
        aliases = alias_map.get(canonical, [])
        if alias not in aliases:
            errors.append(
                f"tauri.events: '{canonical}' must declare deprecated_aliases including '{alias}'"
            )

    alias_count = sum(len(aliases) for aliases in alias_map.values())
    summaries.append(f"tauri.events aliases: {alias_count} declared")
    return summaries, errors


def validate_fixture_category(
    repo_root: Path, contracts: dict[str, dict[str, Any]]
) -> tuple[list[str], list[str]]:
    summaries: list[str] = []
    errors: list[str] = []

    expected_fixture = repo_root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
    if vc.EXAMPLES_PATH != expected_fixture:
        errors.append(
            "validate_contracts EXAMPLES_PATH does not point to canonical fixture source "
            "shared/ipc/examples/IPC_V1_EXAMPLES.jsonl"
        )

    if not expected_fixture.exists():
        errors.append("missing canonical fixture source: shared/ipc/examples/IPC_V1_EXAMPLES.jsonl")
    else:
        line_count = sum(1 for _ in expected_fixture.open("r", encoding="utf-8"))
        summaries.append(f"IPC_V1_EXAMPLES.jsonl: {line_count} entries scanned")

    errors.extend(vc.validate_sidecar_examples_against_contract(repo_root, contracts["sidecar.rpc"]))
    errors.extend(vc.validate_tauri_event_payload_examples(repo_root, contracts["tauri.events"]))

    examples_file = repo_root / "src" / "hooks" / "useTauriEvents.test.ts"
    extracted = vc.extract_event_payload_examples_from_test_file(examples_file)
    seen_names = {event_name for _line, event_name, _payload in extracted}

    for required_name in ["transcript:complete", "transcription:complete", "state:changed", "model:status"]:
        if required_name not in seen_names:
            errors.append(f"useTauriEvents.test.ts: missing drift-guard payload fixture for '{required_name}'")

    model_payloads: list[dict[str, Any]] = []
    for line, event_name, payload_expr in extracted:
        if event_name != "model:status":
            continue
        try:
            payload = json.loads(vc.coerce_js_object_literal_to_json_text(payload_expr))
            model_payloads.append(payload)
        except Exception as exc:  # pragma: no cover - defensive parse guard
            errors.append(f"useTauriEvents.test.ts:{line}: unable to parse model:status payload: {exc}")

    if not model_payloads:
        errors.append("useTauriEvents.test.ts: missing model:status payload fixture")
    elif not any("status" in payload for payload in model_payloads):
        errors.append("useTauriEvents.test.ts: model:status fixtures must include 'status' field")

    events_contract_text = (repo_root / "shared" / "contracts" / "tauri.events.v1.json").read_text(
        encoding="utf-8"
    )
    if '"error_detail"' in events_contract_text:
        errors.append("tauri.events contract should not include legacy 'error_detail' field")
    if '"detail"' not in events_contract_text:
        errors.append("tauri.events contract must include canonical 'detail' field")

    summaries.append(f"event payload fixtures: {len(extracted)} examples")
    return summaries, errors


def validate_generator_category(repo_root: Path) -> tuple[list[str], list[str]]:
    summaries: list[str] = []
    errors: list[str] = []

    errors.extend(vc.validate_generator_determinism(repo_root))
    errors.extend(vc.validate_generated_files(repo_root))

    summaries.append("gen_contracts_ts.py + gen_contracts_rs.py deterministic")
    summaries.append("generated artifacts match committed files")
    return summaries, errors


def validate_cross_reference_category(
    repo_root: Path, contracts: dict[str, dict[str, Any]]
) -> tuple[list[str], list[str]]:
    summaries: list[str] = []
    errors: list[str] = []

    frontend_errors = vc.validate_frontend_listener_events(repo_root, contracts["tauri.events"])
    rust_errors = vc.validate_rust_event_payloads(repo_root, contracts["tauri.events"])
    handler_errors = vc.validate_sidecar_handler_dispatch(repo_root, contracts["sidecar.rpc"])

    errors.extend(frontend_errors)
    errors.extend(rust_errors)
    errors.extend(handler_errors)

    summaries.append(f"frontend listener checks: {'OK' if not frontend_errors else 'FAIL'}")
    summaries.append(f"rust emission checks: {'OK' if not rust_errors else 'FAIL'}")
    summaries.append(f"sidecar dispatch checks: {'OK' if not handler_errors else 'FAIL'}")
    return summaries, errors


def validate_drift_prevention_category() -> tuple[list[str], list[str]]:
    summaries: list[str] = []
    errors: list[str] = []

    # Ensure sidecar handler validation is contract-driven (no hard-coded allowlist).
    with tempfile.TemporaryDirectory() as tmpdir:
        root = Path(tmpdir)
        server_file = root / "sidecar" / "src" / "openvoicy_sidecar" / "server.py"
        server_file.parent.mkdir(parents=True, exist_ok=True)
        server_file.write_text(
            "\n".join(
                [
                    "def handle_custom(request):",
                    "    return {}",
                    "HANDLERS: dict[str, object] = {",
                    "  'custom.method': handle_custom,",
                    "}",
                ]
            ),
            encoding="utf-8",
        )
        contract = {
            "items": [
                {
                    "type": "method",
                    "name": "custom.method",
                    "required": True,
                    "params_schema": {"type": "object"},
                    "result_schema": {"type": "object"},
                }
            ]
        }
        derived_errors = vc.validate_sidecar_handler_dispatch(root, contract)
        if derived_errors:
            errors.append(
                "sidecar handler validation should be contract-driven; synthetic contract method failed validation"
            )

    # Ensure adding a handler without contract declaration fails validation.
    with tempfile.TemporaryDirectory() as tmpdir:
        root = Path(tmpdir)
        server_file = root / "sidecar" / "src" / "openvoicy_sidecar" / "server.py"
        server_file.parent.mkdir(parents=True, exist_ok=True)
        server_file.write_text(
            "\n".join(
                [
                    "def handle_ping(request):",
                    "    return {}",
                    "def handle_shadow(request):",
                    "    return {}",
                    "HANDLERS: dict[str, object] = {",
                    "  'system.ping': handle_ping,",
                    "  'shadow.method': handle_shadow,",
                    "}",
                ]
            ),
            encoding="utf-8",
        )
        contract = {
            "items": [
                {
                    "type": "method",
                    "name": "system.ping",
                    "required": True,
                    "params_schema": {"type": "object"},
                    "result_schema": {"type": "object"},
                }
            ]
        }
        unknown_errors = vc.validate_sidecar_handler_dispatch(root, contract)
        if not any("not declared in sidecar.rpc contract" in err for err in unknown_errors):
            errors.append(
                "sidecar handler validation must fail when HANDLERS contains methods missing from contract"
            )

    summaries.append("allowlists derived from contract schemas")
    summaries.append("undeclared sidecar handlers fail validation")
    return summaries, errors


def run(repo_root: Path) -> int:
    contracts = load_contracts(repo_root)
    categories = [
        ("Schema Validation", lambda: validate_schema_category(contracts)),
        ("Fixture Validation", lambda: validate_fixture_category(repo_root, contracts)),
        ("Generator Validation", lambda: validate_generator_category(repo_root)),
        ("Cross-Reference Validation", lambda: validate_cross_reference_category(repo_root, contracts)),
        ("Drift Prevention", validate_drift_prevention_category),
    ]

    all_errors: list[str] = []
    for category_name, validate in categories:
        log(f"Category: {category_name}")
        summaries, errors = validate()
        for summary in summaries:
            log(f"  {summary}")
        if errors:
            for error in errors:
                log(f"  FAIL: {error}")
            all_errors.extend(f"{category_name}: {error}" for error in errors)
        else:
            log("  PASS")

    if all_errors:
        log(f"FAILED ({len(all_errors)} issue(s))")
        return 1

    log("ALL CHECKS PASSED")
    return 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run comprehensive contract validation suite.")
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT, help="Repository root path.")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    return run(args.repo_root.resolve())


if __name__ == "__main__":
    raise SystemExit(main())
