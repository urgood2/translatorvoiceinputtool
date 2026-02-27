# Shared Contracts

This directory is the contracts-as-code boundary for host/UI/sidecar interfaces.

## Strategy

Contracts are versioned machine-readable artifacts that can be validated in CI and consumed by code generators.
Every `*.v1.json` contract is additive-only and follows the same baseline structure:

- Top-level `version: 1`
- Top-level `items[]` array with stable `name` values
- Explicit legacy alias tracking via `deprecated_aliases` arrays

Payload shapes are represented with JSON Schema draft-07 fragments and local `$id` values (no network refs).
Use local `$id`/`$ref` targets such as `./<file>.v1.json#/$defs/<fragment_name>`.

## Contract Files

- `sidecar.rpc.v1.json`: canonical sidecar RPC contract
- `tauri.commands.v1.json`: canonical Tauri command contract
- `tauri.events.v1.json`: canonical Tauri event contract
- `error.codes.v1.json`: canonical error code catalog contract
- `tauri_wire.v1.json`: Rust↔TS wire snapshot contract
- `MIGRATION.md`: compatibility windows and alias deprecation timeline
- `VERSIONING.md`: versioning and drift-prevention rules

## Fixture Corpus Policy

Canonical human-edited fixture corpus:

- `shared/ipc/examples/IPC_V1_EXAMPLES.jsonl`

Derived fixture corpora under `shared/contracts/examples/*.jsonl` are optional and must never be edited by hand.
If present, they must be generated from the canonical corpus via:

- `python scripts/gen_contract_examples.py`

CI/build validation enforces this via:

- `python scripts/gen_contract_examples.py --check` (invoked by `scripts/validate_contracts.py` when derived fixtures exist)

## Compatibility Policy

Legacy aliases are controlled by `shared/contracts/MIGRATION.md`.

- Legacy alias compatibility is currently retired (`compat_window_active: false`).
- Use canonical event names only; legacy alias names are unsupported.
- Document any future compatibility-window reactivation in `MIGRATION.md`.
- CI enforces compatibility-window guards via `scripts/check_contract_aliases.py` when active.
