# File Ownership Rules for Parallel Agent Work

This document defines file ownership and coordination rules for concurrent workstreams.

## Agent Assignments

- Agent A: Phase 0 plumbing (contracts, supervisor, session gating, P0.1-P0.5)
- Agent B: Phase 1 UI (tabs/dashboard/history/replacements) and Phase 6 UI
- Agent C: Tray and cues (Phase 2.1 and 2.2)
- Agent D: Overlay (Phase 2.3)
- Agent E: Models, Whisper, and packaging (Phase 4 and Phase 7)

## Single-Owner Files

Only one agent may edit these files at a time.

1. `src-tauri/src/integration.rs` and IPC bridging files: Agent A during Phase 0
2. `src/hooks/useTauriEvents.ts`: Agent A during Phase 0, then Agent B during Phase 1
3. `src/types.ts`: Agent A during Phase 0, then Agent B during Phase 1
4. `src-tauri/src/commands.rs`: Agent A during Phase 0, then individual PR ownership
5. `shared/ipc/IPC_PROTOCOL_V1.md`: Agent A during Phase 0
6. `shared/contracts/*`: Agent A during Phase 0
7. `sidecar/src/openvoicy_sidecar/server.py`: Agent A during Phase 0

## Shared Files Requiring Coordination

- `package.json` and `src-tauri/Cargo.toml`: keep changes minimal and merge-friendly
- `.github/workflows/*`: Agent E owns, all others coordinate via Agent Mail before edits
- `vite.config.ts`: Agent D owns overlay changes, then Agent B owns UI-driven updates

## File Reservation Protocol

1. Before editing any single-owner file, reserve it via MCP Agent Mail.
2. Include the Beads issue ID in the reservation reason (for example `translatorvoiceinputtool-2gj.23`).
3. Release reservations immediately after merge or when handing off.
4. If reservation conflict occurs, do not edit; coordinate via Agent Mail thread and sequence work.

## Merge Conflict Mitigation

- Each agent works on a feature branch.
- Rebase frequently on `main`.
- PR review must verify ownership compliance for touched files.
- If two agents need the same file, sequence PRs; do not parallel-edit the same single-owner file.

## Operational Notes

- Beads is the task status source of truth.
- Agent Mail is the coordination and reservation source of truth.
- Use a shared thread keyed by Beads ID for handoffs and conflict resolution.
