# System Architecture and Data Flow

This reference captures the brownfield architecture diagram and runtime data flow from plan §2.
Use it before changing integration, IPC, sidecar, and frontend event handling.

## Architecture Diagram

```text
┌──────────────────────────────────────────────────────────────────────────┐
│                           Tauri Host (Rust)                             │
│                                                                          │
│  Contract layer (generated types + fixtures)                             │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │ shared/contracts/*  -> src-tauri/src/contracts.rs                 │  │
│  │                    -> src/types.contracts.ts (generated)          │  │
│  │                    -> src/types.ts (handwritten wrappers/exports) │  │
│  └────────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│  src-tauri/src/state.rs              src-tauri/src/config.rs             │
│  ┌──────────────────────┐            ┌────────────────────────────────┐  │
│  │ AppStateManager       │            │ AppConfig (schema v1)          │  │
│  │ Idle/Loading/...      │            │ atomic write + migration       │  │
│  └─────────┬────────────┘            └───────────────┬────────────────┘  │
│            │ broadcast(app events)                    │ apply live       │
│  ┌─────────▼──────────────────────────────────────────▼───────────────┐  │
│  │ IntegrationManager (src-tauri/src/integration.rs)                  │  │
│  │  - HotkeyManager (hotkey.rs)                                       │  │
│  │  - RecordingController (recording.rs)                              │  │
│  │  - InjectionController (injection.rs + focus.rs)                   │  │
│  │  - TranscriptHistory (history.rs)                                  │  │
│  │  - TrayManager (tray.rs) / OverlayManager (overlay.rs, new)        │  │
│  │  - SidecarSupervisor (supervisor.rs, new; watchdog.rs evolved)     │  │
│  │  - RpcClient (ipc/*) to sidecar                                    │  │
│  └───────────────┬───────────────────────────────────────┬────────────┘  │
│                  │ JSON-RPC calls + captured logs          │ Tauri events │
└──────────────────▼────────────────────────────────────────▼───────────────┘
                   │                                        │
┌──────────────────▼───────────────────┐     ┌─────────────▼──────────────┐
│          Python Sidecar              │     │   React Main + Overlay      │
│ sidecar/src/openvoicy_sidecar/       │     │ src/App.tsx + src/overlay   │
│  - audio.*, recording.*              │     │ Zustand store + hooks        │
│  - model.*, asr.*                    │     │ listens to canonical events  │
│  - replacements.*, status.get        │     │ only (legacy aliases retired)│
│  - (future) VAD + preprocess         │     └─────────────────────────────┘
└──────────────────────────────────────┘
```

## Data Flow (Happy Path)

1. Startup
- Rust loads config, starts sidecar via supervisor, runs `system.ping`/`system.info`, then emits status/state events.
- Rust requests `status.get` and `model.get_status` to initialize frontend-visible state.

2. Mic test
- UI calls `audio.meter_start` through Rust.
- Sidecar emits `event.audio_level`; Rust forwards `audio:level` with throttling and sequence metadata.

3. Start recording
- Rust creates `session_id` and starts recording through `recording.start`.
- State transitions to recording and emits recording/state events.

4. Stop and transcribe
- Rust calls `recording.stop`, transitions to transcribing, and awaits sidecar completion/error notifications.
- Sidecar performs normalize/macros/replacements; Rust does not re-apply replacements.
- Rust injects text, writes history entry, emits transcript completion, and returns to idle.

## Session Correlation Rules

- `session_id` is created by Rust at record start and used across all runtime calls/events.
- Every event includes monotonic `seq` from Rust runtime.
- Sidecar notifications must include `session_id`; Rust drops mismatched session events.
- `seq` resets on app restart, so frontend dedupe cannot assume cross-restart persistence.

## Guardrails

1. Keep host/sidecar/frontend boundaries explicit; avoid cross-layer behavior duplication.
2. Prefer additive IPC/event changes; use canonical event names unless a compatibility window is explicitly reactivated in `shared/contracts/MIGRATION.md`.
3. When adding lifecycle behavior, extend supervisor/integration flow instead of rewriting core orchestration.
