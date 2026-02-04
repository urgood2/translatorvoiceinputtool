# OpenVoicy (MVP v0.1.0) — Master Implementation Plan  
**Date:** 2026-02-04  
**Goal:** Cross-platform (Windows/macOS/Linux) push-to-talk voice transcription (offline after first model download) that injects text into the currently focused input, with tray + settings + replacements.

---

## 0) Scope, Principles, Definition of Done

### In-scope (MVP)
- Global **push-to-talk** hotkey: press/hold to record, release to stop & transcribe (with fallback **toggle** mode if an OS can’t provide release events reliably).
  - Implementation note (testable): “hold” requires reliable key-down + key-up events; verify Tauri 2 global shortcut plugin provides press/release events on each OS early in M0. If not, ship toggle as default on the affected OS and clearly label it in UI/tray.
  - Implementation note (added for specificity): “release stops” means the first `key_up` after a `key_down` starts a stop request; ignore repeated `key_down` key repeat events and debounce OS auto-repeat.
- Offline transcription using **NVIDIA Parakeet V3 0.6B** (model cached locally after first download).
  - Implementation note (testable): “offline after first download” means *no network calls* required for subsequent transcriptions when model is present and valid (checksum match).
  - Implementation note (added for testability): add an explicit “offline verification” mode in the manual checklist: disable network at OS level and confirm transcription still succeeds with cached model.
- **Text injection** into focused field (Unicode-safe, clipboard-paste default, optional restore clipboard).
  - Implementation note (added for clarity): “injection succeeded” means the target app receives the full Unicode text (not just clipboard update). If paste cannot be performed, fall back to “clipboard-only” with an error surface explaining why.
- **System tray** with status + basic menu.
- **Settings UI**: microphone selection, hotkey config, injection options, replacement rules CRUD.
- **Robustness**: sidecar supervision, restart/backoff, clear user errors, diagnostics.

### Out-of-scope (post-MVP)
- Wake word / always-listening mode, cloud sync, plugins, multi-model selector beyond Parakeet, automatic updates, deep Wayland portal workarounds (best-effort only).

### Principles (implementation rules)
- **Stable contracts first:** IPC protocol is the integration boundary; no ad-hoc RPC methods beyond `IPC_PROTOCOL_V1.md`.
- **“Core loop works without UI”:** UI must never be required to record/transcribe/inject; tray + hotkey are sufficient.
- **Fail safe, not silent:** if injection fails, put text on clipboard and surface a visible error with next steps.
- **Pinned reproducibility:** model artifact source and revision must be pinned; downloads must be resumable and checksummed.
  - Implementation note (testable): pin includes (a) human-readable source identifier, (b) immutable revision identifier, and (c) one or more cryptographic digests for the downloaded artifact(s). Store this in a checked-in manifest referenced by both Rust and sidecar.
  - Implementation note (added for completeness): manifest also includes expected uncompressed size(s), file list, and a “current schema version” so future changes are migration-safe and testable.
- **No surprise permissions:** clearly prompt for/describe required OS permissions (macOS Microphone + Accessibility; Windows microphone; Linux varies).
  - Implementation note (added for testability): blocked-permission states must be distinguishable and mapped to specific remediation text in Diagnostics.

### Assumptions & Constraints (added for specificity/testability)
- Tauri 2 is used for the desktop shell; global shortcut + tray are implemented via Tauri plugins where possible.
- Sidecar is shipped as a single executable (no system Python dependency).
- “CUDA” is only expected on NVIDIA-capable systems; macOS must be CPU-only unless explicitly supported and documented.
- Model distribution must be feasible without requiring interactive auth (if an upstream source requires auth, mirror or switch to an unauthenticated source for MVP, while still pinning revision + hashes in the manifest).

### Definition of Done (MVP release)
- Fresh install → user can configure mic/hotkey → hold hotkey → speak → release → transcription injected in any app.
- No unhandled panics/crashes during 1-hour manual soak test.
- Sidecar crash triggers visible error + one-click restart; app remains responsive.
- Builds produced for Windows/macOS/Linux; sidecar bundled; model downloaded on first run.
- “Known limitations” documented for at least Wayland injection/hotkey constraints and macOS permissions friction (Microphone + Accessibility).
- Added requirement (for release clarity): include an explicit “Supported Platforms & Limitations” section in the README or `docs/KNOWN_LIMITATIONS.md`, referenced from the UI Diagnostics view.

---

## 1) Architecture (Single Responsibility + Clear Contracts)

### Components
1. **Tauri 2 (Rust) core**
   - Global hotkey handling
   - Sidecar lifecycle + IPC client
   - State machine (idle/recording/transcribing/error)
   - Text injection (clipboard paste/typing)
   - Tray integration
   - Config persistence + migrations
2. **Web UI (React + TypeScript + Tailwind via Vite)**
   - Settings + replacements CRUD
   - Status indicator + last transcript
   - Error surfaces + “copy diagnostics”
3. **Python sidecar**
   - JSON-RPC 2.0 server over stdin/stdout (NDJSON framing)
   - Audio device enumeration + capture (sounddevice)
   - ASR load + inference (NeMo/torch; CPU fallback)
   - Postprocess + replacement engine
   - Emits notifications for state/results/errors

### Key design choices
- **IPC:** JSON-RPC 2.0 over newline-delimited JSON on stdin/stdout; supports request/response + notifications.
- **E2E flow runs in Rust**: hotkey triggers sidecar; Rust injects text; UI is optional for core loop.
- **Injection default = clipboard paste** (most reliable for Unicode); “restore clipboard” is configurable.
  - Implementation note (testable): “clipboard paste” requires simulating paste keystroke (`Ctrl+V` on Windows/Linux; `Cmd+V` on macOS) and may require Accessibility permissions (macOS) and may be limited on Wayland. Detect and message these cases explicitly.
  - Implementation note (added for specificity): paste is implemented as (1) set clipboard text, (2) optional small delay, (3) synthesize paste shortcut. If paste synthesis fails, do not retry indefinitely; fall back once to “clipboard-only” and surface an error.
- **Config owned by Rust** (atomic writes); Rust pushes changes to sidecar via RPC (`audio.set_device`, `replacements.set_rules`, `asr.initialize`).
  - Implementation note (added for testability): define a single Rust-owned config schema (versioned) with explicit defaults; add tests for default generation and migration.

### State machine contract (Rust-owned, source of truth)
- States: `idle → loading (optional) → recording → transcribing → idle` or `error`.
- A **session_id** is created by Rust at `recording.start` return and remains authoritative through completion/error.
- Rust must ignore stale notifications (session mismatch) and must not inject twice for one session.
- Time-bound behavior (testability):
  - Rust must enforce per-method RPC timeouts and surface timeouts as user-actionable errors (e.g., “Sidecar unresponsive; restart sidecar”).
  - Rust must serialize “injection” so transcripts cannot interleave.
  - Added requirement (for determinism): define explicit max durations for each phase (e.g., max recording length; max “no notification after stop” wait before error), configurable with defaults and covered by tests where feasible.

---

## 2) Repository Structure (Consistent Naming)

```
/
├─ src-tauri/
│  ├─ Cargo.toml
│  ├─ tauri.conf.json
│  ├─ src/
│  │  ├─ main.rs
│  │  ├─ state.rs                # AppState + state machine
│  │  ├─ config.rs               # load/save/migrate AppConfig (atomic)
│  │  ├─ ipc/
│  │  │  ├─ mod.rs               # RpcClient + read loop
│  │  │  ├─ types.rs             # request/response/notifications + errors
│  │  ├─ sidecar.rs              # spawn/supervise/restart/backoff
│  │  ├─ recording.rs            # start/stop orchestration, session handling
│  │  ├─ injection.rs            # paste/type + clipboard restore
│  │  ├─ hotkey.rs               # register hotkey, hold/toggle modes
│  │  ├─ tray.rs                 # tray icon + menu, state mapping
│  │  └─ commands.rs             # Tauri commands for UI
│  └─ icons/                     # app + tray icons (idle/recording/transcribing/error)
│
├─ src/                          # React UI
│  ├─ main.tsx
│  ├─ App.tsx
│  ├─ components/
│  │  ├─ StatusIndicator.tsx
│  │  ├─ Settings/
│  │  │  ├─ SettingsPanel.tsx
│  │  │  ├─ MicrophoneSelect.tsx
│  │  │  ├─ HotkeyConfig.tsx
│  │  │  ├─ InjectionSettings.tsx
│  │  │  └─ Diagnostics.tsx
│  │  └─ Replacements/
│  │     ├─ ReplacementList.tsx
│  │     ├─ ReplacementEditor.tsx
│  │     └─ ReplacementPreview.tsx
│  ├─ stores/appStore.ts
│  ├─ types.ts
│  └─ styles/globals.css
│
├─ sidecar/
│  ├─ pyproject.toml
│  ├─ src/openvoicy_sidecar/
│  │  ├─ __main__.py             # entry point
│  │  ├─ server.py               # JSON-RPC loop + dispatch
│  │  ├─ protocol.py             # message parsing + helpers
│  │  ├─ audio.py                # devices + recorder
│  │  ├─ asr.py                  # Parakeet loader + transcribe
│  │  ├─ postprocess.py          # cleanup/casing/spacing
│  │  └─ replacements.py         # rules + macros
│  └─ tests/
│     ├─ test_protocol.py
│     ├─ test_postprocess.py
│     └─ test_replacements.py
│
├─ shared/
│  └─ ipc/IPC_PROTOCOL_V1.md      # authoritative contract + examples
│
├─ scripts/
│  ├─ build-sidecar.(sh|ps1)      # PyInstaller build (per OS)
│  └─ bundle-sidecar.(sh|ps1)     # copy artifacts into Tauri resources
│
└─ .github/workflows/build.yml    # CI builds for all OS targets
```

Added recommendation (for completeness; still consistent with structure):  
- Add `shared/model/MODEL_MANIFEST.json` (or `.toml`) referenced by both Rust and sidecar, containing pinned source/revision/hashes/expected file list.

---

## 3) IPC Protocol v1 (Authoritative Contract)

### Transport
- **NDJSON**: one JSON object per line.
- Requests include `id`; responses match `id`.
- Notifications omit `id`.
- Implementation requirement: messages MUST be single-line JSON (no embedded newlines); writer must flush after each line; reader must tolerate partial reads and buffer until newline.
- Safety limits (testable):
  - Enforce a maximum inbound line length (e.g., 1 MiB) on both sides; oversized lines produce a controlled error and transition to `error` state (to avoid memory blowups).
  - Reject/ignore messages that are not `jsonrpc:"2.0"`.
  - Added requirement (for testability): both sides must treat trailing whitespace as acceptable; newline delimiters are `\n` (accept `\r\n` on input).

### Standard shapes
- Request: `{ jsonrpc:"2.0", id:string|number, method:string, params?:object }`
- Response: `{ jsonrpc:"2.0", id, result?:any, error?:{ code:string, message:string, details?:any } }`
- Notification: `{ jsonrpc:"2.0", method:string, params:object }`

### Methods (Rust → Python)
- `system.ping` → `{ version: string, protocol: "v1" }`
  - Added requirement (for clarity): `version` is sidecar version string; Rust compares and logs mismatch if it differs from bundled expectation.
- `asr.initialize` `{ model: "parakeet-v3-0.6b", device_pref: "auto"|"cuda"|"cpu" }` → `{ status:"ready" }`
  - Requirement: `asr.initialize` is idempotent; subsequent calls must be fast and must not reload weights unless model/device preference changed.
  - Implementation note (testable): define “fast” as returning within 250ms when already initialized and ready.
  - Added requirement (for completeness): if initialization requires download, `asr.initialize` may return a controlled “initializing” error (e.g., `E_NOT_READY`) only if the protocol explicitly documents it; otherwise it must block (with a long Rust timeout) and emit status updates via notifications.
- `audio.list_devices` → `{ devices: [{ id:number, name:string, is_default:boolean, sample_rate:number, channels:number }] }`
  - Added requirement (for clarity): `id` is sidecar-local and stable for the process lifetime.
- `audio.set_device` `{ device_id:number|null }` → `{ active_device_id:number|null }`
- `recording.start` `{ device_id?:number|null }` → `{ session_id:string }`
  - Added requirement (for testability): `session_id` is unique per process (UUID v4 recommended) and never reused.
- `recording.stop` `{ session_id:string }` → `{ audio_duration_ms:number }`
- `recording.cancel` `{ session_id:string }` → `{ status:"cancelled" }`
  - Added requirement (for clarity): cancel discards buffered audio and MUST NOT emit `event.transcription_complete`; it may emit `event.status_changed` back to `idle`.
- `replacements.set_rules` `{ rules: ReplacementRule[] }` → `{ count:number }`
  - Implementation note (testable): define `ReplacementRule` shape in `IPC_PROTOCOL_V1.md` explicitly (fields, types, constraints) so UI/Rust/Python validate identically.
  - Added requirement (for clarity): rule evaluation order (top-to-bottom) is documented and tested.
- `status.get` → `{ state:"idle"|"loading"|"recording"|"transcribing"|"error", detail?:string }`
  - Added requirement (for completeness): document whether `detail` is user-facing vs dev-facing; recommended: user-safe string, with deeper diagnostics captured separately in logs.

### Notifications (Python → Rust)
- `event.status_changed` `{ state, detail? }`
  - Added recommendation (for testability, backward-compatible): allow optional `progress?: { current:number, total?:number, unit?:"bytes"|"steps" }` to support model download/initialization progress without inventing new notification types.
- `event.transcription_complete` `{ session_id, text, confidence?:number, duration_ms:number }`
  - Added requirement (for clarity): `text` is already postprocessed and replacements-applied by sidecar; Rust must not re-run replacements (to avoid divergence).
  - Added requirement (for testability): define `confidence` range (0–1) if present; define whether `duration_ms` is transcription compute time (recommended) vs audio duration (audio already returned by `recording.stop`).
- `event.transcription_error` `{ session_id, code:string, message:string }`

### Error codes (stable strings)
- `E_METHOD_NOT_FOUND`, `E_INVALID_PARAMS`, `E_NOT_READY`, `E_MIC_PERMISSION`, `E_DEVICE_NOT_FOUND`, `E_AUDIO_IO`, `E_MODEL_LOAD`, `E_TRANSCRIBE`, `E_INTERNAL`

### Contract clarifications (to make implementation testable)
- Sidecar must emit **exactly one** of `event.transcription_complete` or `event.transcription_error` per `session_id` that reaches `recording.stop`.
- `recording.stop` must return quickly (bounded time, e.g., <250ms) and transcription must happen asynchronously afterward.
- Rust RPC client timeouts must be explicit per method (e.g., `system.ping` short; `asr.initialize` long) and surfaced as actionable error UI.
- Concurrency rules (testable):
  - Sidecar must reject or deterministically handle overlapping sessions (e.g., if `recording.start` called while already recording, return `E_INVALID_PARAMS` or `E_NOT_READY` with clear message).
  - Rust must not issue `recording.stop` for an unknown or already-stopped `session_id`; if it happens, handle the error without panic.
- Added requirement (for completeness/testability): define canonical timeouts in one place (Rust defaults), referenced by the protocol doc:
  - Example defaults (can be tuned later): `system.ping` 1s, `audio.list_devices` 2s, `audio.set_device` 2s, `recording.start/stop/cancel` 2s, `replacements.set_rules` 2s, `asr.initialize` 10–20 minutes (first-run download).

---

## 4) Milestones, Tasks, and Acceptance Criteria (Optimized for 3–5 Agents)

### Milestone M0 — Project + Contract Lock (Day 0–1)
**Goal:** unblock parallel work with stable file layout + IPC contract + scaffolds.

- M0.1 Create/confirm scaffolding for Tauri 2 + React/Vite/Tailwind; confirm dev run works.
  - AC: `tauri dev` launches window; UI hot reload works; Rust command callable from UI.
  - AC: platform permissions stubs are present (at minimum, documented placeholders for macOS microphone/accessibility usage strings and Linux/Windows notes).
  - AC (added for completeness): include a minimal “smoke test” command list in README (dev run, unit tests) so new contributors can validate quickly.
- M0.2 Write `shared/ipc/IPC_PROTOCOL_V1.md` (final method names + payloads + examples).
  - AC: All teams implement against this contract; no ad-hoc methods.
  - AC: Includes at least one example message for each method/notification and one example error response per common error type.
  - AC (added for completeness/testability): includes explicit `ReplacementRule` schema, size limits, and stated per-method timeout guidance (even if exact timeout values live in Rust config).
  - AC (added for testability): add a CI step that validates every JSON example in the doc parses and matches the stated shapes (even if only via lightweight parsing).
- M0.3 Add sidecar skeleton + ping handler.
  - AC: Rust spawns sidecar and successfully calls `system.ping`.
  - AC (added for clarity): ping demo includes a failing-path example (e.g., sidecar missing) that produces a user-actionable error and does not hang.
  - AC (added for specificity): confirm stdout is strictly NDJSON protocol and that any logs go to stderr; Rust must not treat stderr as protocol.

**Coordination checkpoint:** “Ping demo” merged before M1/M2 proceed.

---

### Milestone M1 — Python Sidecar MVP (Day 1–3)
**Goal:** reliable audio capture + transcription + notifications.

- M1.1 JSON-RPC server loop (`server.py`, `protocol.py`) with robust errors and clean EOF exit.
  - AC: unknown method returns `E_METHOD_NOT_FOUND`; invalid payload returns `E_INVALID_PARAMS`.
  - AC: malformed JSON line returns `E_INVALID_PARAMS` (when possible) and does not crash the process; EOF triggers clean shutdown with exit code 0 (unless in an internal fatal state).
  - AC (added for testability): enforce NDJSON single-line output and flush-after-each-message; add tests that simulate partial reads/writes and oversized lines.
  - AC (added for clarity): server implements graceful shutdown on stdin close and on an internal “shutdown” signal (even if not exposed as a public RPC method for MVP).
- M1.2 Device enumeration + set device (`audio.list_devices`, `audio.set_device`).
  - AC: returns devices; handles “no devices” gracefully (empty list + `status=error` detail).
  - AC: `audio.set_device` validates device existence and returns `E_DEVICE_NOT_FOUND` for invalid IDs.
  - AC (added for clarity): device IDs are stable for the life of the process; if the underlying library only provides index-based IDs, document and treat IDs as best-effort per run.
  - AC (added for testability): add a unit-testable abstraction for “device list provider” so device validation logic can be tested without physical devices.
- M1.3 Recorder (push-to-talk) with bounded memory (max seconds; ring buffer/deque) at 16kHz mono float32.
  - AC: start/stop works repeatedly; no buffer growth beyond configured cap; device disconnect returns `E_AUDIO_IO`.
  - AC: if the input device does not support 16kHz mono, recorder must still operate by capturing at a supported rate/channels and converting deterministically to 16kHz mono float32 (conversion strategy documented and unit-tested at the boundary level).
  - AC (added for testability): “max seconds” is a config value with a default (documented) and a hard upper bound; stopping at cap produces a deterministic status/error message.
  - AC (added for clarity): document the exact conversion path (resample algorithm choice; downmix strategy) and ensure it is deterministic across platforms.
- M1.4 Parakeet loader + inference (`asr.initialize`, internal transcribe) with CUDA/CPU fallback.
  - AC: model loads once per process; on failure emits `event.status_changed=error` with actionable detail; CPU fallback works.
  - AC: model download/cache location is deterministic and documented; partial downloads are resumable (or safely retried) and failures are surfaced with next steps (disk space, network, permissions).
  - AC (added for clarity): `device_pref:"auto"` chooses CUDA when available, otherwise CPU; any additional accelerators (e.g., MPS) must be explicitly documented if supported.
  - AC (added for reproducibility): model artifacts are validated via checksums from the pinned manifest; corrupted cache triggers a controlled re-download and a visible status update.
- M1.5 Postprocess + replacements pipeline (macros `@@date`, `@@time`, `@@datetime`; snippet word-boundary replacements).
  - AC: unit tests for replacements + postprocess; prevents recursive replacement loops (max depth or single-pass guarantees).
  - AC: replacement rules validation rejects invalid patterns/empty keys and reports `E_INVALID_PARAMS` with details.
  - AC (added for clarity): macros use local system time; output formats are documented and covered by tests (including timezone/locale invariance if applicable).
  - AC (added for testability): define whether replacements run before or after postprocess and lock that order with tests.
- M1.6 Notifications emitted for status transitions + transcription completion/errors.
  - AC: on stop, sidecar returns quickly and later emits exactly one completion/error for the session.
  - AC: `event.status_changed` is emitted on entering/exiting `recording` and `transcribing`, and on `error` with `detail` suitable for UI display.
  - AC (added for testability): notification ordering constraints are documented (e.g., `recording → transcribing → idle/error`) and verified by tests in `test_protocol.py` or integration-like harness.
  - AC (added for clarity): define what sidecar emits on cancellation (recommended: `recording → idle`, no transcription events).

---

### Milestone M2 — Rust Core MVP (Day 1–3, parallel with M1)
**Goal:** supervise sidecar, orchestrate recording, inject text, tray/hotkey.

- M2.1 Sidecar manager (`sidecar.rs`): spawn, capture stdout/stderr, restart with backoff and max retries.
  - AC: crash → auto-restart up to N times; then hard error state + tray shows error.
  - AC: stdout is reserved for NDJSON only; stderr is captured for diagnostics without breaking protocol parsing.
  - AC (added for specificity): define N and backoff parameters in config (defaults documented), and ensure restarts stop when user quits/restarts explicitly.
  - AC (added for testability): add a deterministic “crash simulation” mode in dev builds (or a test-only sidecar stub) so restart/backoff logic can be verified without ML dependencies.
- M2.2 RPC client (`ipc/mod.rs` + `ipc/types.rs`): correlation by `id`, timeouts, notification fanout.
  - AC: can handle concurrent calls safely (or explicitly serialized); notifications forwarded to app state.
  - AC: parser tolerates split/partial lines and rejects oversized lines with a controlled error (to avoid memory blowups).
  - AC (added for testability): per-method timeouts are configurable; implement deterministic cancellation/cleanup when timeouts occur (no leaked pending promises/handles).
  - AC (added for clarity): strict separation of concerns: NDJSON framing/parsing is independent from JSON-RPC dispatch/awaiting responses.
- M2.3 Recording controller (`recording.rs`) + state machine (`state.rs`).
  - AC: prevents double-start/double-stop; session_id tracked; stale notifications ignored.
  - AC: rapid press/release produces a deterministic result (either a short transcription or a controlled “too short” user message) without deadlock.
  - AC (added for clarity): define “too short” threshold (ms) and make it configurable; document whether audio is still sent to ASR below the threshold.
  - AC (added for testability): add unit tests for state transitions (including error transitions) and for stale-notification handling.
- M2.4 Text injection (`injection.rs`): clipboard paste default + optional restore; fallback to typing when configured.
  - AC: Unicode injection works in browsers/editors; injection failures copy to clipboard + notify.
  - AC: injection is serialized (no interleaving) and configurable with a small “paste delay” to accommodate apps that need focus settle time.
  - AC (added for specificity): paste shortcut is OS-specific (`Ctrl+V` vs `Cmd+V`); typing fallback documents limitations (IME, dead keys) and is opt-in if unreliable.
  - AC (added for completeness): add a “clipboard-only” mode for environments where keystroke injection is blocked; ensure UI/tray communicates that paste is not attempted in this mode.
- M2.5 Global hotkey (`hotkey.rs`) using Tauri global shortcut plugin.
  - AC: press starts recording; release stops (or toggle fallback mode works); hotkey changes persist and apply without restart.
  - AC: hotkey conflicts are detected where possible and surfaced as a user-actionable error (choose another hotkey).
  - AC (added for testability): on OSes without reliable release events, app defaults to toggle mode and UI labels it; hotkey event behavior is documented in Diagnostics.
  - AC (added for clarity): document supported key combos and any OS restrictions (e.g., reserved system shortcuts).
- M2.6 System tray (`tray.rs`): idle/recording/transcribing/error; menu: Show/Settings, Restart Sidecar, Quit.
  - AC: tray always reflects current state; Restart Sidecar recovers from error.
  - AC (added for clarity): Restart Sidecar also clears stale session state safely (no accidental injection after restart).
  - AC (added for testability): tray menu items are enabled/disabled deterministically by state (e.g., “Restart Sidecar” disabled if sidecar is already healthy).
- M2.7 Config persistence (`config.rs`) with atomic writes + versioned migrations.
  - AC: first run creates defaults; subsequent runs load; corruption fallback to last-known-good.
  - AC: config schema includes (at minimum) mic device selection, hotkey + mode, injection mode + restore clipboard, replacements list, and logging/diagnostics settings; migration tests cover at least one prior version.
  - AC (added for specificity): config file location is OS-appropriate (AppData/Application Support/XDG); atomic write strategy is documented and covered by tests using temp dirs.
  - AC (added for completeness): explicitly define config versioning scheme (integer `schema_version`) and a stable JSON layout so import/export is predictable.

**Coordination checkpoint:** “Record loop without ASR” (start/stop + status changes) merged before M3.

---

### Milestone M3 — UI MVP (Day 2–4)
**Goal:** configure the app without touching CLI; status visibility.

- M3.1 Status indicator (idle/recording/transcribing/error) + last transcript display.
  - AC: UI updates within 200ms of status change events.
  - AC (added for testability): UI update latency target is measured from receipt of `event.status_changed` in Rust to store update render in UI (instrument with simple timestamps in dev mode).
  - AC (added for clarity): last transcript view includes “copy” action and displays whether it was injected vs clipboard-only fallback.
- M3.2 Settings panel: microphone selection, hotkey picker (hold/toggle mode), injection delay, restore clipboard toggle.
  - AC: all settings persist; invalid hotkeys blocked; mic list reflects `audio.list_devices`.
  - AC (added for clarity): settings changes apply live (push to sidecar / update hotkey registration) and failures roll back with visible error.
  - AC (added for testability): settings UI includes a “Test microphone” action (optional for MVP) or at minimum a clear indicator of the currently active device and whether sidecar accepted it.
- M3.3 Replacements manager: CRUD, enable/disable, import/export JSON, preview box (“input → processed output”).
  - AC: saves rules to config; pushes rules to sidecar via `replacements.set_rules`; preview uses same engine path (sidecar call or local mirror).
  - AC (added for testability): import validates schema and shows exact row-level errors (which rule failed and why) without losing existing rules.
  - AC (added for clarity): export format is versioned and includes enough metadata to re-import safely (e.g., `schema_version`, optional `name` fields).
- M3.4 Diagnostics view: “Copy diagnostics” (versions, protocol, last error, sidecar status).
  - AC: produces a single text blob suitable for bug reports.
  - AC: includes OS + app version + sidecar version + model status (downloaded/initializing/ready/error) and last N lines of logs (bounded).
  - AC (added for clarity): includes whether running under Wayland/X11, current hotkey mode (hold/toggle), and injection mode (clipboard/typing/clipboard-only fallback).
  - AC (added for testability): ensure diagnostics blob size is bounded (bytes) and redacts any potentially sensitive environment details beyond what is needed (e.g., avoid full home paths if not required; if paths included, include only the model/config directory basename).

---

### Milestone M4 — End-to-End Integration + Hardening (Day 4–5)
**Goal:** ship-grade MVP behavior and error handling.

- M4.1 Wire hotkey → start/stop → transcription notifications → injection (Rust-owned E2E).
  - AC: end-to-end works without UI open; tray reflects states.
  - AC (added for testability): add a “sidecar stub mode” (optional) that returns deterministic text so injection/hotkey can be validated in CI or on developer machines without ML.
- M4.2 Error handling matrix implemented end-to-end:
  - No microphone, mic permission denied, sidecar crash, model load fail, hotkey conflict, injection blocked, rapid press/release.
  - AC: every case yields user-actionable message; no deadlocks.
  - AC (added for testability): each case has a deterministic reproduction step in the manual checklist and a corresponding expected tray + UI message.
  - AC (added for clarity): map each error to a stable error code category (even if internal), so UI messaging remains consistent across refactors.
- M4.3 Logging (Rust + sidecar) with ring-buffer “recent logs” for diagnostics.
  - AC: logs accessible via Diagnostics view; sidecar stderr captured with prefix.
  - AC (added for specificity): ring buffer size is bounded by lines and bytes; logs redact obvious secrets (if any ever appear) and avoid unbounded binary dumps.
  - AC (added for testability): verify via unit test that log ring buffer never exceeds configured bounds.
- M4.4 Tests + manual checklist
  - Python unit tests: protocol parsing, postprocess, replacements.
  - Rust tests: config load/save/migrate; IPC parsing; injection mode selection (mocked).
  - Manual checklist: cross-app injection (VS Code, browser, terminal), long recording, replacements, restart recovery.
  - AC: manual checklist is written as a runnable, step-by-step script with expected outcomes and at least one “known limitation” callout for Wayland/macOS permissions.
  - AC (added for completeness): checklist includes “first run model download” path and “offline after download” verification (disable network, confirm transcription still works).
  - AC (added for clarity): checklist explicitly calls out OS-specific permission steps (macOS: Microphone + Accessibility; Windows: microphone privacy settings; Linux: desktop environment notes).

---

### Milestone M5 — Packaging + CI (Day 5–7)
**Goal:** reproducible builds for all platforms with bundled sidecar.

- M5.1 Build sidecar binary (PyInstaller) per OS; ensure runtime deps included.
  - AC: app runs without system Python; sidecar starts on first launch.
  - AC: packaged sidecar can download/cache model in an app-writable directory; errors are reported cleanly when blocked by permissions.
  - AC (added for clarity): packaging strategy documents whether GPU acceleration is included in shipped artifacts; if not, app must still meet MVP requirements on CPU-only.
  - AC (added for completeness): document supported CPU architectures (x86_64 vs arm64) per OS and whether separate artifacts are produced.
- M5.2 Tauri bundling configuration (`tauri.conf.json`) to ship sidecar in resources/externalBin.
  - AC: `tauri build` produces installable artifacts.
  - AC: per-OS resource paths are verified at runtime (clear error if missing/corrupt).
  - AC (added for testability): add a startup self-check that verifies sidecar binary exists and is executable (and on macOS, not blocked by quarantine), producing a clear remediation message.
- M5.3 CI workflow: build matrix for Windows/macOS/Linux; artifact upload.
  - AC: builds succeed on CI; version stamping consistent across Rust/UI/sidecar.
  - AC: CI runs unit tests (`cargo test`, `pytest`) and fails fast on protocol/schema mismatches.
  - AC (added for testability): CI includes a contract check step that validates `IPC_PROTOCOL_V1.md` examples are parseable JSON and that any generated schemas (if added) match code expectations.
  - AC (added for clarity): CI artifacts include a build manifest (versions + git SHA + build timestamp) for traceability.

---

## 5) Parallel Execution (3–5 Agents)

### 3 agents
- **Agent A (Rust core + Integration):** M2 + M4 wiring
- **Agent B (Python sidecar + ASR):** M1
- **Agent C (UI + QA/CI):** M3 + M5 scaffolding + test harness

### 4 agents (recommended)
- **Agent A (Rust IPC/sidecar/state):** M2.1–M2.3
- **Agent B (Rust hotkey/tray/injection):** M2.4–M2.6
- **Agent C (Python audio/protocol/postprocess/replacements):** M1.1–M1.3 + M1.5–M1.6
- **Agent D (ML/ASR + packaging hooks):** M1.4 + M5.1

### 5 agents
Add **Agent E (UI/QA)** split into UI vs CI/tests.

**Hard coordination gates**
1. M0.2 IPC contract locked
2. M0.3 ping demo
3. M2.3 “record loop without ASR” demo
4. M1.4 “ASR returns text” demo
5. M4.1 “E2E inject without UI” demo

Added requirement (to improve parallelizability):  
- Each gate has a concrete demo script (inputs/commands + expected output) and a minimal video/screenshot/log snippet artifact, referenced from the corresponding bd issue.

---

## 6) Risk Mitigation (Must-Haves)

- **Wayland hotkeys/injection:** document best-effort; prioritize X11; implement toggle mode fallback.
  - Requirement: detect Wayland at runtime and proactively warn users about limitations; provide “clipboard-only” safe fallback behavior when injection is blocked.
  - Added requirement (for clarity): explicitly document whether global shortcuts are supported on Wayland in the chosen stack; if not, default to “app-focused hotkey only” or provide a visible fallback (still within MVP constraints).
- **macOS permissions:** detect and show step-by-step instructions for Microphone + Accessibility; tray shows blocked state.
  - Requirement: include required permission strings/entitlements in packaging; verify blocked states are distinguishable (mic vs accessibility).
  - Added requirement (for testability): include a diagnostics check that can detect whether Accessibility permission is granted (where feasible) and provide a direct “Open System Settings” hint.
- **Model size/download failures:** explicit “Downloading model…” state; retry; clear cache path messaging.
  - Requirement: download progress/state is surfaced to tray/UI; failures include at least (disk space, network, permissions) hints.
  - Added requirement (for completeness): ensure downloads are atomic (download to temp + checksum validate + rename) so partial/corrupt files never appear as “cached”.
- **Sidecar crash loops:** exponential backoff + capped retries; visible “Restart sidecar” action.
- **Injection edge cases:** default to clipboard paste; serialize injections to avoid interleaving; clipboard restore best-effort.
  - Requirement: when restoring clipboard fails, do not block injection; log and surface only if user opted into strict restore.
  - Added requirement (for clarity): define “strict restore” behavior precisely (e.g., if strict, failure becomes a non-fatal warning vs hard error).
- **Replacement safety:** avoid recursive cascades; validate rules; reject invalid JSON with clear error.

---

## 7) Work Tracking (bd)

- Create bd epics: `M0 Contract`, `M1 Sidecar`, `M2 Rust Core`, `M3 UI`, `M4 Hardening`, `M5 Packaging/CI`.
- For each task above, create a bd issue with: owner stream, dependencies, acceptance criteria, and a short demo script.
- Added requirement (to improve parallelizability/completeness): every issue explicitly notes (a) which coordination gate it depends on, (b) how it is tested (unit/manual/CI), and (c) what artifact proves completion (screenshot/log snippet/demo steps).
- Added requirement (for implementation readiness): bd issues for “unknowns” (Wayland shortcut feasibility, model distribution source feasibility, PyInstaller + ML deps feasibility) must be created and prioritized into M0/M1/M5 as explicit spikes with go/no-go criteria and documented outcomes.