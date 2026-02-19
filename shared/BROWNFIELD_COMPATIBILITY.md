# Brownfield Compatibility Reference

This reference summarizes module-level compatibility constraints from `planning/PLAN.md` (Appendix A).
Use it before touching any existing module to avoid greenfield rewrites and protocol drift.

## Module Impact Map

| Module | Impact | Notes |
|---|---|---|
| `src-tauri/src/state.rs` | No semantic changes | `AppState` enum untouched; may add metadata to `StateEvent`. |
| `src-tauri/src/config.rs` | Additive fields only | New optional fields with defaults; `validate_and_clamp` extended. |
| `src-tauri/src/history.rs` | Extended `TranscriptEntry` | New optional fields; ring buffer `max_size` becomes configurable. |
| `src-tauri/src/integration.rs` | Orchestrator role preserved | Session gating and supervisor wiring added. |
| `src-tauri/src/commands.rs` | Remove TODOs, add new commands | Existing signatures stable; new commands additive. |
| `src-tauri/src/watchdog.rs` | Evolved into supervisor | Same crate; enhanced with circuit breaker behavior. |
| `src-tauri/src/injection.rs` | Minor: `app_overrides` support | Existing injection flow preserved. |
| `src-tauri/src/tray.rs` | Dynamic menu builder | Extends existing tray behavior. |
| `src/hooks/useTauriEvents.ts` | Listen to `state:changed` + legacy | Keep `state_changed` alias during compatibility window. |
| `src/types.ts` | Extended with new types | Existing types remain stable and backward compatible. |
| `shared/ipc/IPC_PROTOCOL_V1.md` | Additive only | IPC v1 stays locked; only additive optional fields/params. |
| `shared/schema/AppConfig.schema.json` | Additive fields only | `additionalProperties: false` requires explicit schema additions. |
| `sidecar/` | Bug fixes plus new methods | Keep behavior stable while adding missing methods additively. |

## New Modules (No Brownfield Conflict)

| Module | Intent |
|---|---|
| `src-tauri/src/supervisor.rs` | Supervisor layer; no rewrite of `integration.rs` or `watchdog.rs` semantics. |
| `src-tauri/src/overlay.rs` | Overlay window management; config-gated and disable-friendly. |
| `src-tauri/src/audio_cue.rs` | Audio cue behavior that respects existing `audio.audio_cues_enabled` settings. |

## Critical Implementation Rules

1. Do not propose greenfield rewrites when extension/refactor is feasible.
2. Map every planned task to existing files/modules before implementation.
3. Include migration, risk, and testing steps for any runtime behavior change.
4. Every additive IPC change must update both `shared/ipc/IPC_PROTOCOL_V1.md` and `shared/contracts/sidecar.rpc.v1.json` in the same PR.
5. Generated files (`src/types.contracts.ts`, `src-tauri/src/contracts.rs`) are committed read-only artifacts; manual edits belong in wrappers/modules around them.

## Integrity Guard

- CI runs `python3 scripts/check_brownfield_compatibility.py`.
- The guard fails if required module mappings are removed, mapped paths disappear, or critical rules drift.
- Regression tests for the guard live in `scripts/tests/test_check_brownfield_compatibility.py`.
