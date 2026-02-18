# Shared Contracts

This directory contains versioned cross-runtime contracts used by Rust host and Python sidecar.

- `sidecar.rpc.v1.json`: canonical sidecar RPC schema
- `error.codes.v1.json`: canonical error kind/code mapping
- `MIGRATION.md`: compatibility windows and alias deprecation timeline

## Compatibility Policy

Legacy aliases are controlled by `shared/contracts/MIGRATION.md`.

- Do not remove legacy aliases silently.
- Remove aliases only when the documented criteria are met.
- CI enforces compatibility-window guards via `scripts/check_contract_aliases.py`.
