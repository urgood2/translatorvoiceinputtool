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
- `error.codes.v1.json`: canonical error code catalog contract
- `tauri_wire.v1.json`: Rust↔TS wire snapshot contract
- `MIGRATION.md`: compatibility windows and alias deprecation timeline
- `VERSIONING.md`: versioning and drift-prevention rules

## Compatibility Policy

Legacy aliases are controlled by `shared/contracts/MIGRATION.md`.

- Do not remove legacy aliases silently.
- Remove aliases only when documented criteria are met.
- CI enforces compatibility-window guards via `scripts/check_contract_aliases.py`.
