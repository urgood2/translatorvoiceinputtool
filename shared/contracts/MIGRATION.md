# Contracts Migration and Compatibility Window

This document records legacy alias compatibility requirements, retirement status,
and migration guidance for canonical-only event usage.

<!-- COMPATIBILITY_WINDOW_MARKER_START -->
compat_window_active: false
minimum_release_cycles: 1
enforced_by: scripts/check_contract_aliases.py
retired_at: 2026-02-26
<!-- COMPATIBILITY_WINDOW_MARKER_END -->

## Alias Matrix

| Legacy Alias | Canonical Replacement | Compatibility Notes |
|---|---|---|
| `state_changed` | `state:changed` | Retired. Legacy alias is unsupported as of `retired_at` and canonical-only listeners are required. |
| `transcription:complete` | `transcript:complete` | Retired. Legacy alias is unsupported as of `retired_at`. |
| `transcription:error` | `transcript:error` | Retired. Legacy alias is unsupported as of `retired_at`. |
| `status:changed` | `sidecar:status` | Retired. Legacy alias is unsupported as of `retired_at`. |
| `model:status` legacy shape | `model:status` canonical shape | Retired. Legacy payload shape is unsupported as of `retired_at`; canonical payload is required. |

## Retirement Criteria

Legacy alias support was retired after all criteria below were satisfied:

1. Frontend listeners default to canonical event names and payloads.
2. Contract fixtures/examples explicitly cover both canonical and legacy names during the active window.
3. A deliberate follow-up change removes legacy names and fixture coverage together in one reviewed PR.
4. At least one release cycle has shipped with dual support before removal.

## Timeline and Migration Note

- Compatibility window is inactive (`compat_window_active: false`).
- Legacy aliases were maintained for at least one release cycle before retirement.
- Legacy event names listed above are unsupported as of `2026-02-26`.
- Producers and consumers must use canonical event names only:
  - `state:changed`
  - `transcript:complete`
  - `transcript:error`
  - `sidecar:status`

## CI Guard

CI runs `scripts/check_contract_aliases.py` while `compat_window_active: true`.

- If guarded legacy aliases disappear before migration is marked complete, CI fails.
- With the window inactive, the guard records retirement metadata and alias mapping history.
