# Contracts Migration and Compatibility Window

This document defines legacy alias compatibility requirements and removal gates.

<!-- COMPATIBILITY_WINDOW_MARKER_START -->
compat_window_active: false
minimum_release_cycles: 1
enforced_by: scripts/check_contract_aliases.py
retired_at: 2026-02-26
<!-- COMPATIBILITY_WINDOW_MARKER_END -->

## Alias Matrix

| Legacy Alias | Canonical Replacement | Compatibility Notes |
|---|---|---|
| `state_changed` | `state:changed` | Legacy frontend listener alias remains until canonical listener is defaulted in frontend and fixtures cover both names. |
| `transcription:complete` | `transcript:complete` | Keep legacy emission/listener path during transition; remove only with coordinated fixture and frontend updates. |
| `transcription:error` | `transcript:error` | Same compatibility window as transcription complete. |
| `status:changed` | `sidecar:status` | Legacy sidecar status alias remains while frontend and integration continue migration. |
| `model:status` legacy shape | `model:status` canonical shape | Maintain support for legacy model payload shape until migration criteria and release window complete. |

## Removal Criteria

Each alias can be removed only when all criteria below are satisfied:

1. Frontend listeners default to canonical event names and payloads.
2. Contract fixtures/examples explicitly cover both canonical and legacy names during the active window.
3. A deliberate follow-up change removes legacy names and fixture coverage together in one reviewed PR.
4. At least one release cycle has shipped with dual support before removal.

## Timeline

- Compatibility window is active now.
- Aliases stay for at least one release cycle after canonical listeners and fixture coverage are in place.
- Earliest removal is the first release after all criteria above are met.

## CI Guard

CI runs `scripts/check_contract_aliases.py` while `compat_window_active: true`.

- If guarded legacy aliases disappear before migration is marked complete, CI fails.
- To intentionally retire aliases, update this document in the same PR (including marker state and rationale).
