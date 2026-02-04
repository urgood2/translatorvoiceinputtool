# OpenVoicy (MVP v0.1.0) — Master Implementation Plan  
**Date:** 2026-02-04  
**Goal:** Cross-platform (Windows/macOS/Linux) push-to-talk voice transcription (offline after first model download) that injects text into the currently focused input, with tray + settings + replacements.

---

## 0) Scope, Principles, Definition of Done

### In-scope (MVP)
- Global **push-to-talk** hotkey: press/hold to record, release to stop & transcribe (with fallback **toggle** mode if an OS can’t provide release events reliably).
- Offline transcription using **NVIDIA Parakeet V3 0.6B** (model cached locally after first download).
- **Text injection** into focused field (Unicode-safe, clipboard-paste default, optional restore clipboard).
- **System tray** with status + basic menu.
- **Settings UI**: microphone selection, hotkey config, injection options, replacement rules CRUD.
- **Robustness**: sidecar supervision, restart/backoff, clear user errors, diagnostics.

### Out-of-scope (post-MVP)
- Wake word / always-listening mode, cloud sync, plugins, multi-model selector beyond Parakeet, automatic updates, deep Wayland portal workarounds (best-effort only).

### Definition of Done (MVP release)
- Fresh install → user can configure mic/hotkey → hold hotkey → speak → release → transcription injected in any app.
- No unhandled panics/crashes during 1-hour manual soak test.
- Sidecar crash triggers visible error + one-click restart; app remains responsive.
- Builds produced for Windows/macOS/Linux; sidecar bundled; model downloaded on first run.

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
- **Config owned by Rust** (atomic writes); Rust pushes changes to sidecar via RPC (`audio.set_device`, `replacements.set_rules`, `asr.initialize`).

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

---

## 3) IPC Protocol v1 (Authoritative Contract)

### Transport
- **NDJSON**: one JSON object per line.
- Requests include `id`; responses match `id`.
- Notifications omit `id`.

### Standard shapes
- Request: `{ jsonrpc:"2.0", id:string|number, method:string, params?:object }`
- Response: `{ jsonrpc:"2.0", id, result?:any, error?:{ code:string, message:string, details?:any } }`
- Notification: `{ jsonrpc:"2.0", method:string, params:object }`

### Methods (Rust → Python)
- `system.ping` → `{ version: string, protocol: "v1" }`
- `asr.initialize` `{ model: "parakeet-v3-0.6b", device_pref: "auto"|"cuda"|"cpu" }` → `{ status:"ready" }`
- `audio.list_devices` → `{ devices: [{ id:number, name:string, is_default:boolean, sample_rate:number, channels:number }] }`
- `audio.set_device` `{ device_id:number|null }` → `{ active_device_id:number|null }`
- `recording.start` `{ device_id?:number|null }` → `{ session_id:string }`
- `recording.stop` `{ session_id:string }` → `{ audio_duration_ms:number }`
- `recording.cancel` `{ session_id:string }` → `{ status:"cancelled" }`
- `replacements.set_rules` `{ rules: ReplacementRule[] }` → `{ count:number }`
- `status.get` → `{ state:"idle"|"loading"|"recording"|"transcribing"|"error", detail?:string }`

### Notifications (Python → Rust)
- `event.status_changed` `{ state, detail? }`
- `event.transcription_complete` `{ session_id, text, confidence?:number, duration_ms:number }`
- `event.transcription_error` `{ session_id, code:string, message:string }`

### Error codes (stable strings)
- `E_METHOD_NOT_FOUND`, `E_INVALID_PARAMS`, `E_NOT_READY`, `E_MIC_PERMISSION`, `E_DEVICE_NOT_FOUND`, `E_AUDIO_IO`, `E_MODEL_LOAD`, `E_TRANSCRIBE`, `E_INTERNAL`

---

## 4) Milestones, Tasks, and Acceptance Criteria (Optimized for 3–5 Agents)

### Milestone M0 — Project + Contract Lock (Day 0–1)
**Goal:** unblock parallel work with stable file layout + IPC contract + scaffolds.

- M0.1 Create/confirm scaffolding for Tauri 2 + React/Vite/Tailwind; confirm dev run works.
  - AC: `tauri dev` launches window; UI hot reload works; Rust command callable from UI.
- M0.2 Write `shared/ipc/IPC_PROTOCOL_V1.md` (final method names + payloads + examples).
  - AC: All teams implement against this contract; no ad-hoc methods.
- M0.3 Add sidecar skeleton + ping handler.
  - AC: Rust spawns sidecar and successfully calls `system.ping`.

**Coordination checkpoint:** “Ping demo” merged before M1/M2 proceed.

---

### Milestone M1 — Python Sidecar MVP (Day 1–3)
**Goal:** reliable audio capture + transcription + notifications.

- M1.1 JSON-RPC server loop (`server.py`, `protocol.py`) with robust errors and clean EOF exit.
  - AC: unknown method returns `E_METHOD_NOT_FOUND`; invalid payload returns `E_INVALID_PARAMS`.
- M1.2 Device enumeration + set device (`audio.list_devices`, `audio.set_device`).
  - AC: returns devices; handles “no devices” gracefully (empty list + `status=error` detail).
- M1.3 Recorder (push-to-talk) with bounded memory (max seconds; ring buffer/deque) at 16kHz mono float32.
  - AC: start/stop works repeatedly; no buffer growth beyond configured cap; device disconnect returns `E_AUDIO_IO`.
- M1.4 Parakeet loader + inference (`asr.initialize`, internal transcribe) with CUDA/CPU fallback.
  - AC: model loads once per process; on failure emits `event.status_changed=error` with actionable detail; CPU fallback works.
- M1.5 Postprocess + replacements pipeline (macros `@@date`, `@@time`, `@@datetime`; snippet word-boundary replacements).
  - AC: unit tests for replacements + postprocess; prevents recursive replacement loops (max depth or single-pass guarantees).
- M1.6 Notifications emitted for status transitions + transcription completion/errors.
  - AC: on stop, sidecar returns quickly and later emits exactly one completion/error for the session.

---

### Milestone M2 — Rust Core MVP (Day 1–3, parallel with M1)
**Goal:** supervise sidecar, orchestrate recording, inject text, tray/hotkey.

- M2.1 Sidecar manager (`sidecar.rs`): spawn, capture stdout/stderr, restart with backoff and max retries.
  - AC: crash → auto-restart up to N times; then hard error state + tray shows error.
- M2.2 RPC client (`ipc/mod.rs` + `ipc/types.rs`): correlation by `id`, timeouts, notification fanout.
  - AC: can handle concurrent calls safely (or explicitly serialized); notifications forwarded to app state.
- M2.3 Recording controller (`recording.rs`) + state machine (`state.rs`).
  - AC: prevents double-start/double-stop; session_id tracked; stale notifications ignored.
- M2.4 Text injection (`injection.rs`): clipboard paste default + optional restore; fallback to typing when configured.
  - AC: Unicode injection works in browsers/editors; injection failures copy to clipboard + notify.
- M2.5 Global hotkey (`hotkey.rs`) using Tauri global shortcut plugin.
  - AC: press starts recording; release stops (or toggle fallback mode works); hotkey changes persist and apply without restart.
- M2.6 System tray (`tray.rs`): idle/recording/transcribing/error; menu: Show/Settings, Restart Sidecar, Quit.
  - AC: tray always reflects current state; Restart Sidecar recovers from error.
- M2.7 Config persistence (`config.rs`) with atomic writes + versioned migrations.
  - AC: first run creates defaults; subsequent runs load; corruption fallback to last-known-good.

**Coordination checkpoint:** “Record loop without ASR” (start/stop + status changes) merged before M3.

---

### Milestone M3 — UI MVP (Day 2–4)
**Goal:** configure the app without touching CLI; status visibility.

- M3.1 Status indicator (idle/recording/transcribing/error) + last transcript display.
  - AC: UI updates within 200ms of status change events.
- M3.2 Settings panel: microphone selection, hotkey picker (hold/toggle mode), injection delay, restore clipboard toggle.
  - AC: all settings persist; invalid hotkeys blocked; mic list reflects `audio.list_devices`.
- M3.3 Replacements manager: CRUD, enable/disable, import/export JSON, preview box (“input → processed output”).
  - AC: saves rules to config; pushes rules to sidecar via `replacements.set_rules`; preview uses same engine path (sidecar call or local mirror).
- M3.4 Diagnostics view: “Copy diagnostics” (versions, protocol, last error, sidecar status).
  - AC: produces a single text blob suitable for bug reports.

---

### Milestone M4 — End-to-End Integration + Hardening (Day 4–5)
**Goal:** ship-grade MVP behavior and error handling.

- M4.1 Wire hotkey → start/stop → transcription notifications → injection (Rust-owned E2E).
  - AC: end-to-end works without UI open; tray reflects states.
- M4.2 Error handling matrix implemented end-to-end:
  - No microphone, mic permission denied, sidecar crash, model load fail, hotkey conflict, injection blocked, rapid press/release.
  - AC: every case yields user-actionable message; no deadlocks.
- M4.3 Logging (Rust + sidecar) with ring-buffer “recent logs” for diagnostics.
  - AC: logs accessible via Diagnostics view; sidecar stderr captured with prefix.
- M4.4 Tests + manual checklist
  - Python unit tests: protocol parsing, postprocess, replacements.
  - Rust tests: config load/save/migrate; IPC parsing; injection mode selection (mocked).
  - Manual checklist: cross-app injection (VS Code, browser, terminal), long recording, replacements, restart recovery.

---

### Milestone M5 — Packaging + CI (Day 5–7)
**Goal:** reproducible builds for all platforms with bundled sidecar.

- M5.1 Build sidecar binary (PyInstaller) per OS; ensure runtime deps included.
  - AC: app runs without system Python; sidecar starts on first launch.
- M5.2 Tauri bundling configuration (`tauri.conf.json`) to ship sidecar in resources/externalBin.
  - AC: `tauri build` produces installable artifacts.
- M5.3 CI workflow: build matrix for Windows/macOS/Linux; artifact upload.
  - AC: builds succeed on CI; version stamping consistent across Rust/UI/sidecar.

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

---

## 6) Risk Mitigation (Must-Haves)

- **Wayland hotkeys/injection:** document best-effort; prioritize X11; implement toggle mode fallback.
- **macOS permissions:** detect and show step-by-step instructions for Microphone + Accessibility; tray shows blocked state.
- **Model size/download failures:** explicit “Downloading model…” state; retry; clear cache path messaging.
- **Sidecar crash loops:** exponential backoff + capped retries; visible “Restart sidecar” action.
- **Injection edge cases:** default to clipboard paste; serialize injections to avoid interleaving; clipboard restore best-effort.
- **Replacement safety:** avoid recursive cascades; validate rules; reject invalid JSON with clear error.

---

## 7) Work Tracking (bd)

- Create bd epics: `M0 Contract`, `M1 Sidecar`, `M2 Rust Core`, `M3 UI`, `M4 Hardening`, `M5 Packaging/CI`.
- For each task above, create a bd issue with: owner stream, dependencies, acceptance criteria, and a short demo script.