# Contracts Versioning Strategy

This document defines the dual-source-of-truth workflow for protocol contracts.

## Dual Source of Truth

1. `shared/ipc/IPC_PROTOCOL_V1.md` is the human-readable specification.
- It is the developer-facing source for semantics and examples.
- Protocol v1 is additive-only: no breaking removals or shape changes.
- Any IPC behavior change must be reflected here.

2. `shared/contracts/sidecar.rpc.v1.json` is the machine-readable mirror.
- It is the validator/generator input for sidecar RPC contract checks.
- Any additive IPC change must be updated here in the same PR as Markdown.

3. Tauri boundary contracts are authoritative for host/UI channels.
- `tauri.commands.v1.json` and `tauri.events.v1.json` define host-to-UI command/event shapes when present.
- Markdown docs may summarize these boundaries, but validation enforces contract correctness.

## Versioning Rules

- Keep top-level `version: 1` in every `shared/contracts/*.v1.json`.
- Maintain stable item names in each `items[]` collection.
- Use additive optional fields/params for forward compatibility.
- Represent legacy names explicitly via fields such as `deprecated_aliases`.

## Generated Artifacts Policy

Generated contract artifacts are committed and treated as read-only outputs.

- `src/types.contracts.ts` (TypeScript generated output, when generated)
- `src-tauri/src/contracts.rs` (Rust generated output, when generated)

Manual edits must go to handwritten wrappers and modules instead of generated outputs.

## Fixture Corpus Policy

- Canonical fixture corpus: `shared/ipc/examples/IPC_V1_EXAMPLES.jsonl`
- Any contract-specific examples directory is generated from canonical JSONL.
- Do not maintain independent manually-edited fixture corpora.

## Required Change Pattern

For any IPC change in v1 scope:

1. Update `shared/ipc/IPC_PROTOCOL_V1.md` (human-readable spec)
2. Update `shared/contracts/sidecar.rpc.v1.json` (machine mirror)
3. Regenerate any derived artifacts as required
4. Update fixtures/examples additively from canonical corpus
5. Add/adjust tests and validators in the same PR

## Drift Prevention Checklist

- No direct manual edits in generated contract artifacts.
- Markdown and JSON contract sources updated together.
- Compatibility alias windows respected per `shared/contracts/MIGRATION.md`.
- CI contract and migration guards pass before merge.
