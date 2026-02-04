# OpenVoicy (MVP v0.1.0) — Master Implementation Plan (v2)
**Date:** 2026-02-04
**Goal:** Cross-platform (Windows/macOS/Linux) push-to-talk voice transcription (offline after first model download) that injects text into the currently focused input, with tray + settings + replacements.

---

## What changed since v1 (high-impact revisions)
This v2 plan strengthens real-world reliability and platform robustness while keeping the MVP scope intact:

1. **Stable microphone selection:** persist **device UID** (string), not process-local numeric IDs.
2. **Focus Guard for injection:** prevents "transcript injected into the wrong app" after latency/window switches.
3. **Wayland support improved:** prefer **XDG Desktop Portal GlobalShortcuts** on Wayland when available; fall back cleanly.
4. **ASR backend abstraction now:** product promise stays "offline after download," model/backend can swap without changing Rust/UI.
5. **Model download hardening:** mirrors, disk-space preflight, process lock, resumable downloads, atomic finalization.
6. **Protocol correctness:** JSON-RPC 2.0 **numeric error codes** + stable string error kinds in `error.data.kind`.
7. **Session IDs truly Rust-authoritative:** Rust generates `session_id` and passes it to sidecar.
8. **Setup UX win:** Mic **test + input level meter** via `event.audio_level`.
9. **Trust cues:** optional audible feedback (start/stop/error) + "Copy last transcript" hotkey is part of MVP defaults.
10. **More compelling replacements:** presets + deterministic macros (date/time).
11. **Hang resilience:** watchdog for non-crash sidecar hangs + suspend/resume revalidation.
12. **CI hardening:** schema/type drift checks, protocol parser fuzzing, and an "offline cached" integration test mode.

---

## 0) Scope, Principles, Definition of Done

### In-scope (MVP)
- Global **push-to-talk** hotkey: press/hold to record, release to stop & transcribe (with fallback **toggle** mode if an OS can't provide release events reliably).
  - "Hold" requires reliable key-down + key-up events. Verify press/release events on each OS early in M0.
  - If unreliable, ship toggle as the default on the affected OS and label it clearly in UI/tray/Diagnostics.
  - Debounce OS auto-repeat: ignore repeated key-down repeat events while already recording.
  - **Wayland note:** prefer portal-based global shortcuts when available (see Capability-driven Effective Mode).
- Offline transcription using a **pinned ASR backend** (primary target: NVIDIA Parakeet TDT 0.6B v3) cached locally after first download.
  - Product promise: **"offline after first download."**
  - Implementation promise: backend/model can change behind the same IPC contract.
  - "Offline after first download" means **no network calls** are required for subsequent transcriptions when model cache is present and passes manifest validation (hash + size).
  - Manual checklist includes an "offline verification" step: disable network at OS level and confirm transcription still succeeds with cached model.
- **Model management** (MVP UX):
  - Status surfaced in tray + UI: `missing → downloading → verifying → ready` (or `error`).
  - "Download model now" (proactive) so first dictation isn't the first time the user discovers a multi-GB download.
  - "Re-download / Purge cache" action to recover from corrupted caches.
  - Downloads are resumable when supported; always checksummed; finalized atomically (temp + verify + rename).
  - **Disk-space preflight** before download to fail early with clear remediation.
- **Text injection** into focused field (Unicode-safe, clipboard-paste default, optional restore clipboard) with **Focus Guard**.
  - Injection default: clipboard paste (set clipboard → short delay → synthesize paste shortcut).
  - **Focus Guard (default ON):** capture a focus signature at stop-time; if focus changed by injection-time, do **clipboard-only** and surface warning.
  - If paste cannot be performed: fall back to "clipboard-only" and surface an actionable error.
  - Configurable **injection suffix** (`"" | " " | "\n"`). Default `" "` to make continued typing feel natural.
  - Safety guard: **never inject into OpenVoicy itself** (settings window focused → clipboard-only + warning).
- **Audible cues** (optional, default ON): start / stop / error sound cues (and/or OS notification where available).
- **System tray** with status + basic menu (Show/Settings, Enable/Disable, Copy last transcript, Restart sidecar, Quit).
- **Settings UI**:
  - microphone selection (by stable UID)
  - microphone test + input level meter
  - hotkey config (hold/toggle, plus effective mode display)
  - injection options (delay, restore clipboard, suffix, clipboard-only, Focus Guard behavior)
  - replacement rules CRUD + import/export + preview + presets toggle
  - model status + actions (download, purge)
  - diagnostics + self-check
- **Transcript history (privacy-first):** in-memory ring buffer (default 20 entries) visible in UI with copy actions. (No disk persistence in MVP.)
- **Robustness**: sidecar supervision, restart/backoff, watchdog for hangs, suspend/resume revalidation, clear user errors, diagnostics, self-check.

### Out-of-scope (post-MVP)
- Wake word / always-listening mode
- Cloud sync
- Plugin ecosystem
- Full multi-model selector UI (backend swap exists; UI picker is post-MVP)
- Automatic updates
- Deep Wayland compositor-specific workarounds beyond standard portals (best-effort only)
- Persistent transcript history (opt-in later with explicit privacy messaging)

### Principles (implementation rules)
- **Stable contracts first:** IPC protocol is the integration boundary; no ad-hoc RPC methods beyond `IPC_PROTOCOL_V1.md`.
- **Capability-driven fallbacks:** runtime detection produces an *effective* hotkey and injection mode. UI must display both "configured" and "effective" modes (and reasons).
- **"Core loop works without UI":** UI must never be required to record/transcribe/inject; tray + hotkey are sufficient.
- **Fail safe, not silent:** if injection fails, put text on clipboard and surface a visible error with next steps.
- **Never mis-inject:** if focus changes between stop and inject, default behavior becomes clipboard-only with a clear warning.
- **Pinned reproducibility:** model artifact source and revision must be pinned; downloads must be resumable and checksummed.
  - Pin includes: (a) human-readable source identifier, (b) immutable revision identifier, and (c) one or more cryptographic digests (MVP: SHA-256).
  - Store in a checked-in manifest referenced by both Rust and sidecar.
  - Manifest also includes expected uncompressed sizes, file list, schema version, and **mirror URLs** per file.
  - Sidecar must enforce a **process-safe cache lock** for download/verify/purge operations.
  - `shared/model/MODEL_MANIFEST.json` schema is explicitly documented, including allowed hash algorithms (MVP: SHA-256) and exact cache layout expectations.
- **License & attribution compliance:** model + dependencies must permit redistribution for the intended release channel; required notices must ship with the app.
  - Checked-in: `docs/THIRD_PARTY_NOTICES.md` listing each redistributed dependency/model artifact with license + source + version/revision; included in release artifacts.
  - M0 includes a go/no-go check if licensing/redistribution terms are unclear or incompatible.
  - MVP assumes **direct distribution via GitHub Releases** unless explicitly changed (licensing obligations can differ by channel).
- **No surprise permissions:** clearly prompt for/describe required OS permissions (macOS Microphone + Accessibility; Windows microphone; Linux varies).
  - Blocked-permission states must be distinguishable and mapped to specific remediation text in Diagnostics.
  - Remediation text is version-controlled and keyed by stable internal error categories so messaging is consistent and testable.
- **Privacy by default:** do not persist transcripts to disk in MVP; diagnostics must avoid including transcript text unless user explicitly copies it.

### Assumptions & Constraints
- Tauri 2 is used for the desktop shell; global shortcut + tray are implemented via Tauri plugins where possible.
- Sidecar is shipped as a single executable (no system Python dependency).
- Python dependencies are pinned (exact versions) in `sidecar/pyproject.toml`.
- **CPU-first baseline:** MVP must work acceptably on CPU-only systems. GPU usage is best-effort only and must never be required.
- Model distribution must be feasible without interactive auth. If upstream requires auth, mirror or switch to an unauthenticated source for MVP (still pinned by revision + hashes).
- Avoid any runtime dependency that requires a background daemon/service installation for MVP.
- Standardize data dirs (documented + used consistently):
  - Config: OS app config directory (`OpenVoicy/config.json`)
  - Cache: OS cache directory (`OpenVoicy/models/...`)
  - Logs: OS log directory (`OpenVoicy/logs/...`)

### Definition of Done (MVP release)
- Fresh install → user configures mic/hotkey → model downloads with visible progress → hold hotkey → speak → release → transcription injected in any app (or clipboard-only with clear reason).
- No unhandled panics/crashes during 1-hour manual soak test.
- Sidecar crash or hang triggers visible error + one-click restart; app remains responsive.
- Builds produced for Windows/macOS/Linux; sidecar bundled; model downloaded on first run.
- "Known limitations" documented for at least Wayland injection/hotkey constraints and macOS permissions friction (Microphone + Accessibility).
- README or `docs/KNOWN_LIMITATIONS.md` includes "Supported Platforms & Limitations", referenced from Diagnostics.
- Self-check exists (tray or UI) that reports: hotkey mode effective, injection mode effective, mic permission, sidecar reachable, model status.
- Mic test works: input level meter responds on selected device.

---

## 1) Architecture (Single Responsibility + Clear Contracts)

### Components
1. **Tauri 2 (Rust) core**
   - Global hotkey handling (+ effective-mode fallback)
   - Sidecar lifecycle + IPC client
   - State machine (idle/loading_model/recording/transcribing/error)
   - Text injection (clipboard paste/typing/clipboard-only) + **Focus Guard**
   - Tray integration
   - Config persistence + migrations
   - Capability detection + permissions checks (where feasible)
   - Transcript history (in-memory ring buffer)
   - Watchdog (hang detection) + suspend/resume revalidation triggers
2. **Web UI (React + TypeScript + Tailwind via Vite)**
   - Settings + replacements CRUD + presets toggles
   - Model status + download/purge actions
   - Status indicator + transcript history + copy actions
   - Error surfaces + "copy diagnostics" + self-check panel
   - Mic test + level meter
3. **Sidecar (Python)**
   - JSON-RPC 2.0 server over stdin/stdout (NDJSON framing)
   - Audio device enumeration + capture
   - Audio preprocessing (downmix/resample/normalize/trim silence)
   - Model cache management (download/verify/purge, mirrors, locking)
   - ASR backend adapter (primary: Parakeet; CPU baseline; optional CUDA)
   - Postprocess + replacement engine + deterministic macros
   - Emits notifications for state/results/errors/progress + audio level meter

### Key design choices
- **IPC:** JSON-RPC 2.0 over newline-delimited JSON on stdin/stdout; supports request/response + notifications.
- **Startup handshake:** Rust calls `system.ping` + `system.info` early; mismatch is logged and surfaced in Diagnostics.
- **Model initialization is proactive:** Rust triggers `asr.initialize` in the background at startup (or via explicit UI action) and surfaces progress; recording should not be the first time initialization runs.
- **Injection default = clipboard paste** (best Unicode reliability); "restore clipboard" is configurable.
  - Paste is implemented as: (1) set clipboard text, (2) optional delay, (3) synthesize paste shortcut.
  - **Focus Guard:** capture focus signature at `recording.stop` and validate at inject-time.
  - If paste synthesis fails, do not retry indefinitely; fall back once to "clipboard-only" and surface an error.
  - Never inject into OpenVoicy itself (settings focused).
- **Config owned by Rust** (atomic writes); Rust pushes changes to sidecar via RPC (`audio.set_device`, `replacements.set_rules`, `asr.initialize`).
  - Single Rust-owned config schema (versioned) with explicit defaults; tests for default generation and migration.
  - Microphone selection is persisted by **device UID string**.

### Capability-driven "effective mode"
Rust computes effective behavior on each platform at runtime:
- Effective hotkey mode:
  - `hold` if reliable key-up events; otherwise `toggle`.
  - On Linux **Wayland**: prefer `org.freedesktop.portal.GlobalShortcuts` (toggle expected); if unavailable, degrade to documented limitations.
- Effective injection mode:
  - `clipboard_paste` if keystroke synthesis is available
  - `clipboard_only` if blocked (e.g., Wayland constraints) or permissions missing
- UI shows both configured and effective modes; Diagnostics includes the reasons.

### State machine contract (Rust-owned, source of truth)
- States: `idle → loading_model (optional) → recording → transcribing → idle` or `error`.
- Rust generates a **session_id** (UUID v4) and passes it to `recording.start`; it remains authoritative through completion/error.
- Rust ignores stale notifications (session mismatch) and must not inject twice for one session.
- Time-bound behavior (defaults; configurable; testable):
  - Max recording length: 60s (hard cap: 300s).
  - "Too short" threshold: 250ms (below this, treat as controlled no-op with user-facing message).
  - "No transcription event after stop" timeout: 60s (then transition to error with remediation).
- Rust serializes injection so transcripts cannot interleave.

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
│  │  ├─ capabilities.rs         # effective hotkey/injection mode + environment detection
│  │  ├─ history.rs              # in-memory transcript ring buffer
│  │  ├─ model.rs                # model init orchestration + model status cache
│  │  ├─ focus.rs                # focus signature capture + Focus Guard decisions
│  │  ├─ watchdog.rs             # ping/status hang detection + resume revalidation hooks
│  │  ├─ ipc/
│  │  │  ├─ mod.rs               # RpcClient + read loop
│  │  │  ├─ types.rs             # request/response/notifications + errors
│  │  ├─ sidecar.rs              # spawn/supervise/restart/backoff
│  │  ├─ recording.rs            # start/stop orchestration, session handling
│  │  ├─ injection.rs            # paste/type + clipboard restore + suffix + Focus Guard integration
│  │  ├─ hotkey.rs               # register hotkey, hold/toggle modes, copy-last hotkey
│  │  ├─ tray.rs                 # tray icon + menu, state mapping
│  │  └─ commands.rs             # Tauri commands for UI
│  └─ icons/                     # app + tray icons (idle/loading/recording/transcribing/error)
│
├─ src/                          # React UI
│  ├─ main.tsx
│  ├─ App.tsx
│  ├─ components/
│  │  ├─ StatusIndicator.tsx
│  │  ├─ Settings/
│  │  │  ├─ SettingsPanel.tsx
│  │  │  ├─ MicrophoneSelect.tsx
│  │  │  ├─ MicrophoneTest.tsx
│  │  │  ├─ HotkeyConfig.tsx
│  │  │  ├─ InjectionSettings.tsx
│  │  │  ├─ ModelSettings.tsx
│  │  │  ├─ HistoryPanel.tsx
│  │  │  ├─ SelfCheck.tsx
│  │  │  └─ Diagnostics.tsx
│  │  └─ Replacements/
│  │     ├─ ReplacementList.tsx
│  │     ├─ ReplacementEditor.tsx
│  │     ├─ ReplacementPreview.tsx
│  │     └─ PresetsPanel.tsx
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
│  │  ├─ audio.py                # devices + recorder (stable device UID)
│  │  ├─ meter.py                # mic level meter (rms/peak)
│  │  ├─ preprocess.py           # resample/downmix/normalize/silence trim
│  │  ├─ model.py                # manifest validation + download + purge + locking + mirrors
│  │  ├─ asr/
│  │  │  ├─ __init__.py          # backend selection + interface
│  │  │  ├─ base.py              # ASRBackend interface
│  │  │  ├─ parakeet.py          # Parakeet backend implementation
│  │  │  └─ fallback.py          # optional fallback backend stub (wired; may be off by default)
│  │  ├─ postprocess.py          # cleanup/casing/spacing
│  │  └─ replacements.py         # rules + macros + presets application
│  └─ tests/
│     ├─ test_protocol.py
│     ├─ test_preprocess.py
│     ├─ test_postprocess.py
│     ├─ test_replacements.py
│     └─ test_model_cache.py
│
├─ docs/
│  ├─ KNOWN_LIMITATIONS.md        # user-facing OS limitations + workarounds
│  ├─ MANUAL_CHECKLIST.md         # step-by-step validation script
│  ├─ PRIVACY.md                  # what is stored where (MVP: no transcript persistence)
│  ├─ THIRD_PARTY_NOTICES.md      # redistributed licenses/attributions
│  └─ DECISIONS/
│     └─ 0001-asr-backend.md      # decision record for primary/fallback ASR + licenses
│
├─ shared/
│  ├─ ipc/
│  │  ├─ IPC_PROTOCOL_V1.md       # authoritative contract + examples
│  │  └─ examples/
│  │     └─ IPC_V1_EXAMPLES.jsonl # machine-validated examples corpus
│  ├─ model/
│  │  └─ MODEL_MANIFEST.json      # pinned model source/revision/hashes/file list + mirrors
│  ├─ replacements/
│  │  ├─ TEST_VECTORS.json        # canonical inputs/expected outputs (pipeline semantics)
│  │  └─ PRESETS.json             # shipped preset rule sets (toggleable)
│  └─ schema/
│     ├─ ReplacementRule.schema.json
│     └─ AppConfig.schema.json
│
├─ scripts/
│  ├─ build-sidecar.(sh|ps1)      # PyInstaller build (per OS)
│  └─ bundle-sidecar.(sh|ps1)     # copy artifacts into Tauri resources
│
└─ .github/workflows/build.yml    # CI builds for all OS targets
```

Requirements:
- `shared/model/MODEL_MANIFEST.json` is mandatory for MVP.
- `shared/schema/*.schema.json` is the single source of truth for runtime validation and CI drift checks.
- CI validates `IPC_V1_EXAMPLES.jsonl` parses and matches the protocol's stated shapes.
- CI additionally enforces generated-type drift checks **or** schema-validation parity checks (Rust/TS/Python).

---

## 3) IPC Protocol v1 (Authoritative Contract)

### Transport
- **NDJSON**: one JSON object per line.
- Requests include `id`; responses match `id`.
- Notifications omit `id`.
- Messages MUST be single-line JSON (no embedded newlines); writer flushes after each line.
- Reader must tolerate partial reads and buffer until newline (`\n`; accept `\r\n`).
- Safety limits:
  - Enforce max inbound line length (1 MiB) on both sides; oversized lines are **fatal** (transition to error; remediation: restart sidecar).

### Standard shapes (JSON-RPC 2.0 aligned)
- Request: `{ jsonrpc:"2.0", id:string|number, method:string, params?:object }`
- Response: `{ jsonrpc:"2.0", id, result?:any, error?:{ code:number, message:string, data?:{ kind:string, details?:any } } }`
- Notification: `{ jsonrpc:"2.0", method:string, params:object }`

### Methods (Rust → Sidecar)

#### `system.ping` → `{ version: string, protocol: "v1" }`
- `version` is sidecar version string; Rust logs mismatch vs bundled expectation.

#### `system.info` → `{ version, protocol:"v1", capabilities, runtime }`
- `capabilities` example:
  - `cuda_available: boolean`
  - `supports_progress: boolean` (download/init progress)
  - `supports_model_purge: boolean`
  - `supports_silence_trim: boolean`
  - `supports_audio_meter: boolean`
- `runtime` example:
  - `python?: string`, `torch?: string`, `platform: string`

#### `system.shutdown` `{ reason?: string }` → `{ status:"shutting_down" }`
- Sidecar must stop recording if active, flush stderr, and exit cleanly.

#### `audio.list_devices` → `{ devices: [{ uid:string, name:string, is_default:boolean, default_sample_rate:number, channels:number }] }`
- `uid` is stable across sidecar restarts and reboots (best-effort, OS-provided).
- Rust MUST persist `device_uid` (string), not a process-local numeric ID.

#### `audio.set_device` `{ device_uid:string|null }` → `{ active_device_uid:string|null }`

#### `audio.meter_start` `{ device_uid?:string|null, interval_ms?:number }` → `{ status:"started" }`
- Starts emitting `event.audio_level` at `interval_ms` (default 80ms; clamp 30–250ms).
- Metering must be low-CPU and not require ASR/model load.

#### `audio.meter_stop` → `{ status:"stopped" }`

#### `model.get_status` → `{ model_id, revision, status, progress?, cache_path? }`
- `status`: `"missing"|"downloading"|"verifying"|"ready"|"error"`
- `progress` optional: `{ current:number, total?:number, unit:"bytes"|"files" }`

#### `model.purge_cache` `{ model_id?: string }` → `{ purged:boolean }`
- Used by UI "Re-download / Purge cache".
- If a model is currently in use, sidecar must reject with `E_NOT_READY` and a clear message.

#### `asr.initialize` `{ model_id: string, device_pref: "auto"|"cuda"|"cpu" }` → `{ status:"ready", model_id:string, device:"cuda"|"cpu" }`
- Idempotent; subsequent calls must return within 250ms when already initialized and ready (unless model/device changed).
- If initialization requires download, this call blocks (with long Rust timeout) and emits progress via notifications.

#### `recording.start` `{ session_id:string, device_uid?:string|null }` → `{ session_id:string }`
- Rust generates session_id (UUID v4) and remains authoritative.
- Sidecar MUST echo session_id and use it for all notifications.

#### `recording.stop` `{ session_id:string }` → `{ audio_duration_ms:number }`
- Must return quickly (<250ms). Transcription happens asynchronously afterward.

#### `recording.cancel` `{ session_id:string }` → `{ status:"cancelled" }`
- Discards buffered audio and MUST NOT emit `event.transcription_complete`.

#### `replacements.set_rules` `{ rules: ReplacementRule[] }` → `{ count:number }`
- `ReplacementRule` shape is normative (see schema + protocol doc).
- Evaluation order: apply-all in order, single pass, no recursion.

#### `status.get` → `{ state:"idle"|"loading_model"|"recording"|"transcribing"|"error", detail?:string, model?:ModelStatus }`
- `detail` is user-safe string. Deep diagnostics belong in logs.

### Notifications (Sidecar → Rust)
- `event.status_changed` `{ state, detail?, progress?, model? }`
  - `progress` uses cumulative bytes for downloads when `unit:"bytes"`.
  - `model` is the latest `ModelStatus` snapshot when relevant.
- `event.audio_level` `{ source:"meter"|"recording", session_id?:string, rms:number, peak:number }`
  - `rms` and `peak` are normalized floats 0..1.
- `event.transcription_complete` `{ session_id, text, confidence?:number, duration_ms:number }`
  - `text` is postprocessed and replacements-applied by sidecar.
  - `confidence` range: 0–1 if present.
  - `duration_ms` is transcription compute time (not audio duration).
- `event.transcription_error` `{ session_id, kind:string, message:string }`
  - `kind` is one of the stable error kinds below.

### Error kinds (stable strings in `error.data.kind` and in `event.transcription_error.kind`)
`E_METHOD_NOT_FOUND`, `E_INVALID_PARAMS`, `E_NOT_READY`,
`E_MIC_PERMISSION`, `E_DEVICE_NOT_FOUND`, `E_AUDIO_IO`,
`E_NETWORK`, `E_DISK_FULL`, `E_CACHE_CORRUPT`,
`E_MODEL_LOAD`, `E_TRANSCRIBE`, `E_INTERNAL`

### ReplacementRule schema (normative; shared by UI/Rust/Sidecar)
Source of truth: `shared/schema/ReplacementRule.schema.json` (referenced by protocol doc).

**Type (required fields):**
- `id: string` (stable identifier; UUID recommended)
- `enabled: boolean`
- `kind: "literal"|"regex"`
- `pattern: string` (non-empty)
- `replacement: string`
- `word_boundary: boolean` (applies to `kind:"literal"`; if `kind:"regex"`, ignored)
- `case_sensitive: boolean`
- `description?: string` (optional, user-facing label)
- `origin?: "user"|"preset"` (optional; UI uses for labeling and bulk toggles)

Semantics:
- Pipeline order (locked for MVP):
  1) postprocess normalization
  2) macro expansion
  3) replacements apply-all in order (single pass, no recursion)

Macros (MVP minimal set; deterministic):
- `{{date}}`, `{{time}}`, `{{datetime}}` (local timezone)
- Must be documented in `IPC_PROTOCOL_V1.md` and tested in `TEST_VECTORS.json`.

Constraints:
- Max rules: 500
- `pattern` max length: 256
- `replacement` max length: 1024
- Output max length: 50,000 chars (truncate with warning category)

Shared test vectors:
- `shared/replacements/TEST_VECTORS.json` is consumed by Python tests and UI tests to prevent drift.

**Example (literal, word-boundary):**
```json
{
  "id": "8c0c8f2e-8d8e-4d8f-9ab1-9a0f35e6f4a1",
  "enabled": true,
  "kind": "literal",
  "pattern": "brb",
  "replacement": "be right back",
  "word_boundary": true,
  "case_sensitive": false,
  "description": "Expand brb"
}
```

**Example (regex):**
```json
{
  "id": "f4f9c2a1-6f3d-4c2d-8fd2-0e7b8f6a9c1d",
  "enabled": true,
  "kind": "regex",
  "pattern": "\\bteh\\b",
  "replacement": "the",
  "word_boundary": false,
  "case_sensitive": false
}
```

### Timeout policy (Rust defaults; referenced by protocol doc)
- `system.ping` 1s
- `system.info` 2s
- `audio.list_devices` 2s
- `audio.set_device` 2s
- `audio.meter_start/stop` 2s
- `model.get_status` 2s
- `model.purge_cache` 10s
- `recording.start/stop/cancel` 2s
- `replacements.set_rules` 2s
- `asr.initialize` 20 minutes (first-run download)

Timeout failures:
- Short-method timeouts are recoverable by 1 retry.
- `asr.initialize` timeout is treated as fatal (transition to error; remediation: restart sidecar).

---

## 4) Milestones, Tasks, and Acceptance Criteria

### Milestone M0 — Project + Contract Lock (Day 0–1)
**Goal:** unblock parallel work with stable file layout + IPC contract + scaffolds.

- M0.1 Scaffold Tauri 2 + React/Vite/Tailwind; confirm dev run works.
  - AC: `tauri dev` launches; UI hot reload works; Rust command callable from UI.
  - AC: platform permissions stubs are present (macOS usage strings; Linux/Windows notes).
  - AC: README includes a minimal "smoke test" command list (dev run, unit tests).
- M0.2 Lock IPC: `shared/ipc/IPC_PROTOCOL_V1.md` + `shared/ipc/examples/IPC_V1_EXAMPLES.jsonl`.
  - AC: examples cover each method/notification + common error responses (numeric JSON-RPC error codes + `data.kind`).
  - AC: CI validates JSONL parses and matches stated shapes.
- M0.3 Sidecar skeleton + ping/info handlers.
  - AC: Rust spawns sidecar and calls `system.ping` and `system.info`.
  - AC: stdout is strict NDJSON; logs go to stderr.
- M0.4 Spike: platform capability verification (hotkey hold/release + injection) per OS.
  - AC: document behavior for Windows, macOS, X11, and Wayland (if available):
    - global shortcut press/release reliability
    - paste keystroke injection feasibility
    - required permissions
    - **Wayland:** portal GlobalShortcuts availability/behavior (toggle-only expected)
  - AC: define per-platform default effective modes and Diagnostics wording.
- M0.5 Spike: model source/revision + manifest + license go/no-go.
  - AC: produce `shared/model/MODEL_MANIFEST.json` with file list + sizes + SHA-256 hashes + mirror URLs.
  - AC: confirm redistribution terms; record in `docs/THIRD_PARTY_NOTICES.md`.
  - AC: decision record `docs/DECISIONS/0001-asr-backend.md` created describing primary + fallback backend.
  - AC: CI validates manifest parses and `asr.initialize.model_id` values match manifest entries.
- M0.6 Spike: sidecar packaging feasibility (audio + ML deps).
  - AC: build minimal sidecar binary running `system.ping` + `audio.list_devices` + `audio.meter_start`.
  - AC: capture binary size, startup time, and native deps.
  - AC: file a **hard go/no-go** issue for primary ASR packaging on each target OS/arch.
  - Contingency: fallback backend stays behind same IPC contract; UI/Rust do not change.

**Coordination gate:** "ping + info + device list + meter demo" merged before M1/M2 proceed.

---

### Milestone M1 — Sidecar MVP (Day 1–3)
**Goal:** reliable audio capture + model management + transcription + notifications.

- M1.1 JSON-RPC server loop with robust errors and clean EOF exit.
  - AC: unknown method → JSON-RPC error with numeric `code` and `data.kind=E_METHOD_NOT_FOUND`.
  - AC: invalid payload → `E_INVALID_PARAMS`.
  - AC: malformed JSON line handled without crashing; EOF → clean shutdown.
  - AC: enforce NDJSON flush-after-each-message; tests simulate partial reads and oversized lines.
- M1.2 Device enumeration + set device (stable UID).
  - AC: returns devices; handles no devices gracefully.
  - AC: stable device UIDs across runs (best-effort).
  - AC: invalid UID → `E_DEVICE_NOT_FOUND`.
- M1.3 Recorder with bounded memory.
  - AC: start/stop repeats; buffer capped; device disconnect → `E_AUDIO_IO`.
  - AC: capture at supported rate/channels and convert deterministically to 16kHz mono float32.
- M1.4 Audio preprocess (deterministic):
  - Downmix to mono, resample to 16k, DC offset removal, peak clamp, optional peak normalize.
  - Optional leading/trailing silence trim with energy threshold (default on; configurable).
  - AC: golden tests for resample + trim behavior.
- M1.5 Model cache + download/verify/purge (`model.get_status`, `model.purge_cache`).
  - AC: deterministic cache location; downloads are atomic (temp + verify + rename).
  - AC: downloads are resumable (HTTP range) when supported; otherwise restart cleanly.
  - AC: disk-space preflight before download; errors map to `E_DISK_FULL` with required bytes in details.
  - AC: cache lock prevents concurrent download/verify/purge across processes.
  - AC: manifest validation checks file existence + size + SHA-256.
  - AC: corrupted cache triggers controlled re-download with visible status updates.
- M1.6 ASR initialize + inference (backend-abstracted; CPU baseline; optional CUDA).
  - AC: `asr.initialize` loads once; idempotent fast path.
  - AC: emits `event.status_changed` progress during download/init.
  - AC: "offline after download" manual invariant verified.
- M1.7 Postprocess + macros + replacements pipeline.
  - AC: locked order + semantics; validated by shared test vectors.
  - AC: macros `{{date}}/{{time}}/{{datetime}}` covered by test vectors.
  - AC: preset rule sets are loadable and labeled as preset origin.
- M1.8 Notifications for status + completion/errors + audio meter.
  - AC: exactly one completion/error per session_id that reaches stop.
  - AC: `recording.stop` returns quickly; transcription async.
  - AC: `audio.meter_start` emits `event.audio_level` at requested cadence and stops correctly.

---

### Milestone M2 — Rust Core MVP (Day 1–3, parallel with M1)
**Goal:** supervise sidecar, orchestrate recording, inject text safely, tray/hotkey, history.

- M2.1 Sidecar manager: spawn, capture stdout/stderr, restart with backoff and max retries.
  - Defaults: max restart attempts 5; backoff exponential 250ms → 10s.
  - AC: deterministic "crash simulation" mode verifies restart/backoff without ML deps.
- M2.2 RPC client: correlation by id, timeouts, notifications.
  - AC: tolerates partial lines; oversized lines → fatal error state.
  - AC: per-method timeouts centralized.
- M2.3 Capability detection module (`capabilities.rs`).
  - AC: detects Wayland vs X11; checks for portal GlobalShortcuts; computes effective hotkey + injection mode and reason.
  - AC: exposed to UI + Diagnostics.
- M2.4 Recording controller + state machine (Rust authoritative session IDs).
  - AC: Rust generates `session_id` and passes into `recording.start`.
  - AC: prevents double-start/stop; stale notifications ignored.
- M2.5 Injection: clipboard paste default + optional restore; suffix support; **Focus Guard**; guard against self-injection.
  - AC: Unicode injection; fall back to clipboard-only on failure.
  - AC: Focus Guard defaults to clipboard-only when focus signature changes.
  - AC: configurable paste delay (default 40ms) clamped and tested.
- M2.6 Transcript history ring buffer (`history.rs`).
  - AC: stores last 20 transcripts in memory with metadata (timestamp, injected?, clipboard_only_reason?, error?).
  - AC: tray menu "Copy last transcript" works even without UI.
- M2.7 Global hotkey handling + audible cues.
  - AC: hold works where possible; toggle fallback where needed; UI shows effective mode.
  - AC: second hotkey: "Copy last transcript" (default enabled; configurable).
  - AC: optional start/stop/error audio cues (default ON; configurable).
- M2.8 Model orchestration (`model.rs`).
  - AC: on startup, Rust calls `model.get_status`; triggers `asr.initialize` in background if missing; surfaces progress.
  - AC: tray shows downloading/verifying states via `loading_model`.
- M2.9 Config persistence with migrations.
  - AC: atomic writes; corruption fallback; tests with temp dirs.
  - AC: microphone selection persists by `device_uid`.
- M2.10 Watchdog + resume revalidation (`watchdog.rs`).
  - AC: watchdog detects non-crash hangs (missed ping/status for N seconds) and restarts sidecar.
  - AC: on OS resume (where detectable) re-check sidecar reachable + device availability + model ready.

**Coordination gate:** "record loop + Focus Guard + injection stub mode without UI" demo merged before M3.

---

### Milestone M3 — UI MVP (Day 2–4)
**Goal:** configure without CLI; status visibility; model and history UX; setup confidence.

- M3.1 Status + transcript history view.
  - AC: shows recent transcripts with copy; indicates injected vs clipboard-only + reason.
- M3.2 Settings: mic, hotkey, injection, replacements, cues.
  - AC: settings apply live; rollback on failure.
- M3.3 Mic test + level meter.
  - AC: starts/stops meter; reacts to device changes; shows obvious "no signal" state.
- M3.4 Model settings: status/progress, download now, purge cache.
  - AC: exposes `model.get_status` and triggers `asr.initialize` / `model.purge_cache`.
- M3.5 Replacements manager: CRUD, presets, import/export, preview via local mirror.
  - AC: validated against shared test vectors to prevent drift.
  - AC: preset rule sets can be enabled/disabled and clearly labeled.
- M3.6 Diagnostics + Self-check.
  - AC: one text blob for bug reports; bounded size; redacts sensitive paths and transcript contents.
  - AC: self-check reports effective modes, permissions hints, sidecar/model status, focus-guard mode.

---

### Milestone M4 — End-to-End Integration + Hardening (Day 4–5)
**Goal:** ship-grade MVP behavior and error handling.

- M4.1 Wire hotkey → record → transcribe → inject (E2E).
  - AC: works without UI open; tray reflects states.
- M4.2 Error handling matrix implemented:
  - no mic, mic permission denied, sidecar crash/hang, model download fail, model load fail, hotkey conflict, injection blocked, focus changed, rapid press/release.
  - AC: each yields user-actionable message; deterministic reproduction steps in manual checklist.
- M4.3 Logging with ring-buffer "recent logs" for diagnostics.
  - Defaults: 500 lines or 256 KiB.
  - AC: bounded by tests; redact sensitive items.
- M4.4 Manual checklist finalized (`docs/MANUAL_CHECKLIST.md`).
  - Includes first-run model download + offline verification; OS-specific permission steps; Wayland limitations; Focus Guard behavior.

---

### Milestone M5 — Packaging + CI (Day 5–7)
**Goal:** reproducible builds for all platforms with bundled sidecar.

- M5.1 Build sidecar per OS (PyInstaller).
  - AC: app runs without system Python; sidecar starts on first launch.
  - AC: CPU-only baseline artifact exists for each OS.
  - AC: document GPU status (supported/unsupported/experimental) per artifact.
- M5.2 Tauri bundling config ships sidecar as externalBin/resources.
  - AC: startup self-check verifies sidecar exists/executable and surfaces quarantine/permission remediation.
- M5.3 CI build matrix + tests.
  - Runs `cargo test`, `pytest`, schema drift checks, protocol example validation.
  - Adds: protocol parser fuzz tests (Rust + sidecar).
  - Adds: "offline cached" integration test mode (cache present, network disallowed).
  - Artifacts include build manifest (versions + git SHA + timestamp).

---

## 5) Parallel Execution (3–5 Agents)

### 3 agents
- Agent A (Rust core + Integration): M2 + M4 wiring
- Agent B (Sidecar + ASR): M1
- Agent C (UI + CI/QA): M3 + M5 scaffolding + tests

### 4 agents (recommended)
- Agent A (Rust IPC/sidecar/state/model/watchdog): M2.1–M2.4 + M2.8 + M2.10
- Agent B (Rust hotkey/tray/injection/focus/history/config): M2.5–M2.7 + M2.9
- Agent C (Sidecar protocol/audio/preprocess/replacements/meter): M1.1–M1.4 + M1.7–M1.8
- Agent D (ML/model cache/packaging/decision record): M1.5–M1.6 + M5.1 + docs/DECISIONS

Hard coordination gates:
1. IPC contract locked + examples corpus validated
2. ping + info + device list + meter demo
3. record loop + Focus Guard + stub injection demo
4. ASR returns text demo
5. E2E inject without UI demo

Each gate includes a concrete demo script and a log/screenshot artifact.

---

## 6) Risk Mitigation (Must-Haves)

- **License/redistribution constraints:** decided in M0.5; ship notices doc; decision record for backend choice(s).
- **Wayland hotkeys/injection:** detect Wayland; prefer portal GlobalShortcuts; degrade to toggle and/or clipboard-only when needed; warn proactively.
- **macOS permissions:** detect and provide step-by-step remediation for Microphone + Accessibility.
- **Model size/download failures:** explicit downloading/verifying state; mirrors; atomic downloads; resumable when possible; cache locking; disk preflight; clear disk/network errors.
- **Sidecar crash loops/hangs:** exponential backoff + capped retries; watchdog restarts on hang; visible restart action.
- **Injection edge cases:** serialize injections; clipboard restore best-effort; never inject into OpenVoicy UI; suffix configurable; Focus Guard prevents mis-injection.
- **Replacement safety:** validate rules; avoid recursion; shared test vectors; presets clearly labeled.
- **Privacy:** no transcript persistence by default; diagnostics avoids transcript text.

---

## 7) Work Tracking

- Create epics: `M0 Contract`, `M1 Sidecar`, `M2 Rust Core`, `M3 UI`, `M4 Hardening`, `M5 Packaging/CI`.
- Each issue includes: dependencies (gate), how tested (unit/manual/CI), and proof artifact.
- Packaging/model distribution issues explicitly call out licensing implications and reference `docs/THIRD_PARTY_NOTICES.md`.
