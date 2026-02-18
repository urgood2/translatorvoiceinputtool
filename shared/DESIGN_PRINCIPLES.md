# Design Principles and Non-Goals

This reference captures the governing principles and explicit non-goals for implementation work.
Use this before making architectural decisions or evaluating trade-offs.

## Design Principles

1. Contracts are code, not prose.
- Schemas, generated types, and fixtures are the operational source of truth.

2. One source of truth per concern.
- Runtime state: `AppStateManager`
- Configuration: `AppConfig`
- Sidecar behavior: JSON-RPC methods plus notifications

3. Deterministic state transitions.
- State-machine and session gating prevent stale or race-driven UI state.

4. Fail-soft behavior.
- Degrade gracefully (clipboard-only injection, overlay disabled, optional model capabilities).

5. Observability by default.
- Correlation IDs and diagnostics should allow root-cause analysis without guesswork.

6. Privacy-first defaults.
- Transcript persistence remains opt-in and defaults to in-memory behavior.

7. Additive compatibility.
- IPC v1 and config v1 changes remain additive via optional fields and defaults.

8. Cross-platform first.
- Handle Windows/macOS/Linux differences explicitly (tray, overlay, permissions).

## Explicit Non-Goals

- Cloud or hosted ASR (runtime remains local/offline).
- Always-on wake-word/hotword mode for this release.
- Full voice-command framework beyond lightweight macros.

## Success Metrics

- Time-to-first-dictation: under 2 minutes on clean install.
- Crash-loop resilience: restart failures never wedge UI recovery paths.
- Session correctness: zero stale/wrong-session text injections.
- Stop-to-injection latency: median under 1.2 seconds for short utterances after warmup.
- Idle overhead: overlay+tray idle CPU under 1 percent on typical laptops.

## Key Trade-Offs

- Contracts-as-code increases up-front effort but removes long-term drift.
- VAD helps UX but must remain opt-in to avoid surprise truncation.
- Encrypted persistence adds complexity; privacy-first default stays memory-only.
- Whisper/model breadth increases package size; treat as optional capability.
- Overlay always-on-top/click-through remains OS-fragile; fallback behavior is required.
- Audio cues can leak acoustically into microphones; mitigation reduces but does not remove risk.
- In-memory history resets on restart by design; persistence is explicit opt-in.
- Manifest-driven model catalogs keep future growth additive and backward-compatible.
