# Master Plan — Voice Input Tool (OpenVoicy): Usability & Polish + Multilingual ASR

## 1. Executive Summary
### Problem statement
The repo has solid primitives (Tauri host, Rust state machine + integration orchestrator, Python sidecar with JSON‑RPC, shared IPC/schema docs), but the product experience is not yet cohesive:
- UI is functional but not “app‑grade” (no top‑level navigation, limited status UX).
- Feedback loops are incomplete (recording overlay missing; audio cues stubbed).
- Tray UX is minimal (no mode/mic/recent submenus; limited dynamic state).
- Several contracts are out of sync (event names/payloads, command stubs vs integration code, sidecar missing `status.get`, sidecar transcription pipeline bugs around replacements).

### Solution overview
Ship a polished, cross‑platform dictation app by sequencing work into:
1) **Contracts & baseline stabilization** (align Rust↔UI and Rust↔sidecar contracts; fix blocking bugs),
2) **UI/UX overhaul** (tabs + status dashboard + improved settings/history/replacements),
3) **System feedback** (overlay + tray enhancements + real audio cues),
4) **Multilingual expansion** (catalog + optional Whisper support, additive IPC),
5) **Onboarding + theming**, and
6) **CI/packaging hardening** (Windows/macOS/Linux).

All changes respect guardrails:
- IPC Protocol V1 changes are **additive only** (`shared/ipc/IPC_PROTOCOL_V1.md`)
- `AppState` semantics remain intact (`src-tauri/src/state.rs`)
- Config remains schema v1 with **additive defaults** (`shared/schema/AppConfig.schema.json`, `src-tauri/src/config.rs`)

### Key innovations
- **Contracts-first integration**: one consistent Tauri command/event surface for both main UI and overlay.
- **Session-correlated, event-driven pipeline**: `session_id` flows through recording→transcription→UI, stale events ignored.
- **Minimal overlay bundle**: separate Vite entry to keep CPU/bundle size low.
- **Dynamic tray menu** derived from live state + config + history + devices.
- **Migration-aware onboarding**: onboarding only for true first-run installs; upgrades don’t surprise existing users.
- **Optional Whisper backend** (faster‑whisper) as a capability-detected fallback path.

---

## 2. Core Architecture
### System diagram
```text
┌──────────────────────────────────────────────────────────────────────────┐
│                           Tauri Host (Rust)                              │
│                                                                          │
│  src-tauri/src/state.rs           src-tauri/src/config.rs                │
│  ┌────────────────────┐           ┌───────────────────────────────────┐ │
│  │ AppStateManager     │           │ AppConfig (schema v1)             │ │
│  │ Idle/Loading/...    │           │ atomic write + migration          │ │
│  └───────┬────────────┘           └───────────────┬───────────────────┘ │
│          │ broadcast(StateEvent)                   │ apply live changes  │
│  ┌───────▼─────────────────────────────────────────▼───────────────────┐ │
│  │ src-tauri/src/integration.rs (IntegrationManager)                   │ │
│  │  - HotkeyManager (src-tauri/src/hotkey.rs)                          │ │
│  │  - RecordingController (src-tauri/src/recording.rs)                 │ │
│  │  - Injection (src-tauri/src/injection.rs)                           │ │
│  │  - TranscriptHistory (src-tauri/src/history.rs)                     │ │
│  │  - Tray/Overlay managers                                            │ │
│  │  - RpcClient (src-tauri/src/ipc/*) to sidecar                        │ │
│  └───────────────┬───────────────────────────────────────┬─────────────┘ │
│                  │ JSON-RPC NDJSON over stdio             │ Tauri events   │
└──────────────────▼───────────────────────────────────────▼───────────────┘
                   │                                       │
                   │                                       │
┌──────────────────▼───────────────────┐     ┌────────────▼─────────────┐
│          Python Sidecar              │     │  React Main + Overlay     │
│ sidecar/src/openvoicy_sidecar/      │     │ src/App.tsx + src/overlay │
│  - audio.*, recording.*             │     │ Zustand store + hooks      │
│  - model.*, asr.*                   │     │ listens to Tauri events    │
│  - replacements.*, status.get       │     └────────────────────────────┘
└──────────────────────────────────────┘
```

### Design principles
1. **One source of truth**: state = `AppStateManager`; config = `AppConfig`; sidecar truth = JSON‑RPC + notifications.
2. **Additive compatibility**: IPC v1 and config v1 only add optional params/fields with defaults.
3. **Fail-soft UX**: degrade gracefully (clipboard-only injection, overlay disabled, whisper unavailable).
4. **Privacy by default**: transcript history is in-memory ring buffer unless later made opt-in persistent.
5. **Cross-platform first**: explicit handling for Windows/macOS/Linux differences in tray, overlay, permissions.

### Data flow (happy path)
1. Startup: Rust loads config → starts sidecar → `system.ping`/`system.info` → `status.get`/`model.get_status` → emits `state_changed`, `sidecar:status`, `model:status`.
2. Mic test: UI `start_mic_test` → sidecar `audio.meter_start` → sidecar emits `event.audio_level` (meter) → Rust forwards as `audio:level`.
3. Record: hotkey start → `RecordingController::start()` → sidecar `recording.start(session_id)` → emits `event.audio_level` (recording) → overlay waveform.
4. Stop/transcribe: hotkey stop → sidecar `recording.stop` (audio_duration_ms) + state → `Transcribing` → sidecar emits `event.transcription_complete(duration_ms=compute)` → Rust injects text, stores history, emits `transcript:complete { entry }`, returns to `Idle`.

---

## 3. Data Models
### 3.1 AppConfig (schema v1, additive-only)
**Files**
- `shared/schema/AppConfig.schema.json`
- `src-tauri/src/config.rs`
- `src/types.ts`

**Additive fields (target)**
- `ui.theme: "system" | "light" | "dark"` (default `"system"`)
- `ui.onboarding_completed: boolean` (default `false` for new installs; **true on migration** when missing)
- `ui.overlay_enabled: boolean` (default `true`)
- `model.language: string | null` (default `null`)

**Validation rules**
- Existing clamps remain (`paste_delay_ms`, window dims, non-empty hotkeys).
- New enums invalid → safe defaults (`theme="system"`, `overlay_enabled=true`).
- `model.language`: `null` or ISO 639-1 or `"auto"`; invalid → `null`.

**Entity relationships**
- `AppConfig.replacements[]` conforms to `shared/schema/ReplacementRule.schema.json`.
- `AppConfig.presets.enabled_presets[]` references preset IDs served by sidecar (`replacements.get_presets`).
- `AppConfig.model.model_id` references entries in model catalog (below).

### 3.2 Model catalog + manifests (multi-model, backward compatible)
**Files**
- Keep: `shared/model/MODEL_MANIFEST.json` (default model, validation scripts keep working)
- Add: `shared/model/MODEL_CATALOG.json`
- Add: `shared/model/manifests/<model_id>.json` (one per model)

**Catalog entry (type)**
```ts
export type ModelFamily = 'parakeet' | 'whisper';

export interface ModelCatalogEntry {
  model_id: string;
  family: ModelFamily;
  display_name: string;
  description: string;
  supported_languages: string[];   // includes "auto" when relevant
  default_language: string;        // usually "auto"
  size_bytes?: number;
  license_spdx?: string;
  manifest_path: string;           // relative to shared/model/
}

export interface ModelCatalog {
  schema_version: 1;
  models: ModelCatalogEntry[];
}
```

**Manifest loading rules**
- Sidecar loads manifests from packaged resources first; falls back to repo paths in dev.
- Rust exposes catalog to UI via Tauri command `get_model_catalog` (catalog embedded at compile time).

### 3.3 Transcript history (in-memory)
**Rust**: `src-tauri/src/history.rs` `TranscriptEntry` + `TranscriptHistory`.
**TS**: `src/types.ts` `TranscriptEntry` must match Rust serialization.

**Relationship**
- One history entry per successful transcription; includes `injection_result` derived from injection module.

### 3.4 Replacement rules + presets
- Schema: `shared/schema/ReplacementRule.schema.json`
- Presets source: `shared/replacements/PRESETS.json`
- Sidecar must load presets at startup and serve via `replacements.get_presets`.

**Blocking bug fixes (must be planned)**
- `sidecar/src/openvoicy_sidecar/notifications.py` must stop importing non-existent `get_current_rules` and must handle `process_text` tuple return correctly.

### 3.5 IPC protocol entities (JSON-RPC v1)
Authoritative: `shared/ipc/IPC_PROTOCOL_V1.md`

**Additive extension**
- `asr.initialize.params.language?: string | null`

### 3.6 Tauri events (Rust → UI/overlay)
**Canonical event names + payloads**
- `state_changed`: `{ state, enabled, detail?, timestamp }`
- `model:status`: `{ model_id, status, revision?, cache_path?, progress?, error? }`
- `model:progress`: `{ current, total?, unit, current_file?, files_completed?, files_total?, stage? }`
- `audio:level`: `{ source:'meter'|'recording', session_id?, rms:number, peak:number }`
- `transcript:complete`: `{ entry: TranscriptEntry }`
- `app:error`: `{ message:string, recoverable:boolean }`
- `sidecar:status`: `{ state:string, restart_count:number, message? }`

---

## 4. CLI/API Surface
### 4.1 Developer CLI
- Beads:
  - `bd onboard`, `bd ready`, `bd show <id>`, `bd update <id> --status in_progress`, `bd close <id>`, `bd sync`
- Dev/build:
  - `bun run tauri dev`, `bun run build`, `bun run test`, `bun run lint`
- Schema/tools:
  - `python shared/schema/validate.py --self-test`
  - `python shared/schema/validate.py --test-vectors`
  - `python scripts/validate_model_manifest.py`
  - `python scripts/validate_ipc_examples.py`
- Sidecar packaging:
  - `./scripts/build-sidecar.sh`, `./scripts/build-sidecar.ps1`
  - `./scripts/bundle-sidecar.sh --target <triple>`, `./scripts/bundle-sidecar.ps1 -Target <triple>`
- E2E scripts:
  - `./scripts/e2e/run-all.sh`, `./scripts/e2e/test-full-flow.sh`, `./scripts/e2e/test-error-recovery.sh`, `./scripts/e2e/test-offline.sh`

### 4.2 Tauri Command API (UI → Rust)
Implementation target: `src-tauri/src/commands.rs` (delegate to `IntegrationState` + sidecar RPC; remove TODO stubs)

For each command: **Usage** (`invoke`) and **Output** (JSON).

- `get_app_state`
  - Usage: `invoke<StateEvent>('get_app_state')`
  - Output: `{ "state":"idle", "enabled":true, "detail":null, "timestamp":"..." }`
- `get_capabilities`
  - Usage: `invoke<Capabilities>('get_capabilities')`
  - Output: `Capabilities`
- `get_capability_issues`
  - Usage: `invoke<CapabilityIssue[]>('get_capability_issues')`
  - Output: `CapabilityIssue[]`
- `can_start_recording`
  - Usage: `invoke<void>('can_start_recording')`
  - Output (success): `null`
  - Output (error): `CannotRecordReason` (serialized)
- `run_self_check`
  - Usage: `invoke<SelfCheckResult>('run_self_check')`
  - Output: `SelfCheckResult`
- `get_config`
  - Usage: `invoke<AppConfig>('get_config')`
  - Output: `AppConfig`
- `update_config`
  - Usage: `invoke<void>('update_config', { config })`
  - Output: `null`
- `reset_config_to_defaults`
  - Usage: `invoke<AppConfig>('reset_config_to_defaults')`
  - Output: `AppConfig`
- `list_audio_devices`
  - Usage: `invoke<AudioDevice[]>('list_audio_devices')`
  - Output: `[{ uid, name, is_default, sample_rate, channels }]`
- `set_audio_device`
  - Usage: `invoke<string>('set_audio_device', { deviceUid: string|null })`
  - Output: `"default"` or device UID
- `start_mic_test` / `stop_mic_test`
  - Usage: `invoke<void>('start_mic_test')`
  - Output: `null`
- `get_model_status`
  - Usage: `invoke<ModelStatus>('get_model_status')`
  - Output: `{ model_id, status, progress?, error?, revision?, cache_path? }`
- `download_model`
  - Usage: `invoke<void>('download_model')`
  - Output: `null` (progress via events)
- `purge_model_cache`
  - Usage: `invoke<void>('purge_model_cache', { modelId?: string })`
  - Output: `null`
- `get_model_catalog` (new)
  - Usage: `invoke<ModelCatalog>('get_model_catalog')`
  - Output: `{ schema_version:1, models:[...] }`
- `get_transcript_history`
  - Usage: `invoke<TranscriptEntry[]>('get_transcript_history')`
  - Output: `TranscriptEntry[]`
- `copy_transcript`
  - Usage: `invoke<void>('copy_transcript', { id })`
  - Output: `null`
- `copy_last_transcript`
  - Usage: `invoke<string|null>('copy_last_transcript')`
  - Output: `"..."` or `null`
- `clear_history`
  - Usage: `invoke<void>('clear_history')`
  - Output: `null`
- `get_hotkey_status`
  - Usage: `invoke<HotkeyStatus>('get_hotkey_status')`
  - Output: `{ primary, copy_last, mode, registered, ... }`
- `set_hotkey`
  - Usage: `invoke<void>('set_hotkey', { primary, copyLast })`
  - Output: `null`
- `get_replacement_rules`
  - Usage: `invoke<ReplacementRule[]>('get_replacement_rules')`
  - Output: `ReplacementRule[]`
- `set_replacement_rules`
  - Usage: `invoke<void>('set_replacement_rules', { rules })`
  - Output: `null` (must also push to sidecar `replacements.set_rules`)
- `preview_replacement`
  - Usage: `invoke<string>('preview_replacement', { input, rules })`
  - Output: `"processed text"` (or replace with sidecar-driven preview for parity)
- `get_available_presets`
  - Usage: `invoke<PresetInfo[]>('get_available_presets')`
  - Output: `PresetInfo[]`
- `load_preset`
  - Usage: `invoke<ReplacementRule[]>('load_preset', { presetId })`
  - Output: `ReplacementRule[]`
- `toggle_enabled` / `is_enabled` / `set_enabled`
  - Usage: `invoke<boolean>('toggle_enabled')`, `invoke<boolean>('is_enabled')`, `invoke<void>('set_enabled',{enabled})`
  - Output: `boolean` / `null`
- `generate_diagnostics` / `get_recent_logs`
  - Usage: `invoke<DiagnosticsReport>('generate_diagnostics')`, `invoke<LogEntry[]>('get_recent_logs',{count})`
  - Output: report / log entries

### 4.3 Tauri Events (Rust → UI/overlay)
- `state_changed`: `StateEvent`
- `model:status`: `ModelStatus`
- `model:progress`: `Progress`
- `audio:level`: `AudioLevelEvent` (includes `source` and optional `session_id`)
- `transcript:complete`: `{ entry: TranscriptEntry }`
- `app:error`: `{ message, recoverable }`
- `sidecar:status`: `{ state, restart_count, message? }`

### 4.4 Sidecar JSON-RPC (Rust → Sidecar) + output formats
Transport: NDJSON over stdio.

**System**
- `system.ping` → `{ version, protocol }`
- `system.info` → `{ version, protocol, capabilities, runtime }`
- `system.shutdown` → `{ status:"shutting_down" }`

**Audio**
- `audio.list_devices` → `{ devices:[{ uid,name,is_default,default_sample_rate,channels }] }`
- `audio.set_device { device_uid }` → `{ active_device_uid }`
- `audio.meter_start { device_uid?, interval_ms? }` → `{ running:true, interval_ms }`
- `audio.meter_stop` → `{ stopped:true }`
- `audio.meter_status` → `{ running:boolean, interval_ms? }`

**Recording**
- `recording.start { session_id, device_uid? }` → `{ session_id }`
- `recording.stop { session_id }` → `{ audio_duration_ms }`
- `recording.cancel { session_id }` → `{ status:"cancelled" }`
- `recording.status` → `{ status:"recording"|"stopped", ... }`

**Model**
- `model.get_status` (additive params `{ model_id? }`) → `{ model_id, revision, status, cache_path, progress?, error? }`
- `model.purge_cache { model_id? }` → `{ purged:true }`

**ASR**
- `asr.initialize { model_id, device_pref, language? }` → `{ status:"ready", model_id, device }`
- `asr.status` → `{ state, model_id?, device?, ready }`
- `asr.transcribe { audio_path }` → `{ text, language?, confidence?, duration_ms? }`

**Replacements**
- `replacements.get_rules` → `{ rules:[...] }`
- `replacements.set_rules { rules }` → `{ count }`
- `replacements.get_presets` → `{ presets:[...] }`
- `replacements.get_preset_rules { preset_id }` → `{ preset, rules }`
- `replacements.preview` → `{ result, truncated }`

**Status**
- `status.get` → `{ state, detail?, model? }` (must be implemented to match spec)

### 4.5 Sidecar notifications (Sidecar → Rust)
- `event.status_changed`
- `event.audio_level`
- `event.transcription_complete`
- `event.transcription_error`

---

## 5. Error Handling
Failure modes + recovery strategies:

- Sidecar spawn/IPC failure: emit `sidecar:status` failed; tray offers Restart; UI shows banner; watchdog retries with backoff.
- Microphone permission denied (`E_MIC_PERMISSION`): show OS-specific steps; allow retry.
- Device not found (`E_DEVICE_NOT_FOUND`): fall back to default device and update config.
- Audio I/O errors (`E_AUDIO_IO`): stop metering/recording; show actionable error; retry.
- Model download issues:
  - `E_DISK_FULL`: show required/available; suggest purge or free space.
  - `E_CACHE_CORRUPT`: suggest purge + reinitialize.
  - `E_NETWORK`: retry; keep progress state consistent.
- Injection failures: record `HistoryInjectionResult::ClipboardOnly` or `Error`; never lose transcript text.
- Overlay issues (click-through/always-on-top quirks): auto-disable overlay via `ui.overlay_enabled=false`; fallback to tray + cues.
- IPC drift (older sidecar): when `language` rejected, retry initialize without it; surface “Whisper not supported in this build”.

---

## 6. Integration Points
### External dependencies
- Rust: `tauri`, `global_hotkey`, `png`, `rodio` (audio cues), platform-specific injection/backends in `src-tauri/src/injection.rs`.
- Python: `sounddevice`, `numpy`, `scipy`; optional `faster-whisper` (and `ctranslate2`) for Whisper; optional heavy deps for Parakeet if bundled.
- Model download sources: HuggingFace URLs in manifests; license/attribution captured in `docs/THIRD_PARTY_NOTICES.md`.

### Secrets handling
- Default mirrors `auth_required=false`.
- Optional HF token support via env var `HF_TOKEN` in sidecar download logic (never stored in config; redacted in logs/diagnostics).

### Config management
- Disk persistence in `src-tauri/src/config.rs` (atomic writes, corruption backup).
- Live application of config via new `IntegrationManager::apply_config(config: &AppConfig)` called from `update_config`.

---

## 7. Storage & Persistence
- Config: platform dir `OpenVoicy/config.json` (plus `.tmp`, `.corrupt`).
- Model cache: `~/.cache/openvoicy/models/<model_id>/...` (macOS/Windows variants per sidecar).
- Transcript history: in-memory only (`src-tauri/src/history.rs`).
- Presets/manifests: canonical in `shared/`; packaged copies included in sidecar and (catalog) in Rust resources.
- Frontend build output: `dist/`; overlay adds an additional built HTML entry.
- Logs: in-memory ring buffer (`src-tauri/src/log_buffer.rs`) exposed via `get_recent_logs`.

---

## 8. Implementation Roadmap
Phased delivery with dependencies and complexity (S/M/L). Designed for 3–5 parallel agents.

### Phase 0 — Contracts & Baseline Stabilization (L)
Dependencies: none (must land first).

- P0.1 Sidecar spec compliance + blocking bug fixes (M)
  - Implement `status.get` in `sidecar/src/openvoicy_sidecar/server.py`.
  - Fix replacements integration in `sidecar/src/openvoicy_sidecar/notifications.py` (missing `get_current_rules`, tuple return).
  - Load presets on startup from `shared/replacements/PRESETS.json` (packaged resource path in release).
  - Add regression tests under `sidecar/tests/` for the above.
- P0.2 Rust↔UI contracts (L)
  - Make `src-tauri/src/commands.rs` delegate to `IntegrationState` + sidecar RPC (remove TODO `NotImplemented` paths for devices/model/meter/presets).
  - Standardize Tauri event names/payloads to match `src/hooks/useTauriEvents.ts` (or update hook/tests to the chosen canonical contract).
  - Fix sidecar notification parsing in `src-tauri/src/integration.rs` to match `IPC_PROTOCOL_V1.md`.
- P0.3 Schema/type alignment (M)
  - Add config fields: `ui.theme`, `ui.onboarding_completed`, `ui.overlay_enabled`, `model.language` across `shared/schema/AppConfig.schema.json`, `src-tauri/src/config.rs`, `src/types.ts`.
  - Update `shared/schema/validate.py` self-tests/examples accordingly.

### Phase 1 — UI Tabs + Status Dashboard + History Improvements (M)
Depends on: Phase 0 contracts.

- P1.1 Top-level tabs: `src/components/Layout/TabBar.tsx`, `TabPanel.tsx`; wire in `src/App.tsx`.
- P1.2 Status dashboard: `src/components/Status/StatusDashboard.tsx` (state, hotkey/mode, last transcript, model + sidecar badges).
- P1.3 History: move to `src/components/History/HistoryPanel.tsx`; add search + clear-all confirm.
- P1.4 Replacements tab: integrate `ReplacementList` + `PresetsPanel`; add tab badge counts.

### Phase 2 — Tray Enhancements (M/L)
Depends on: Phase 0 contracts.

- P2.1 Dynamic tray menu builder (`src-tauri/src/tray_menu.rs`) with:
  - enabled toggle, mode submenu, recent transcripts submenu, microphone submenu, open settings/about/quit.
- P2.2 Rebuild triggers on config/history/device changes; add Rust unit tests.

### Phase 3 — Audio Cues (M)
Depends on: Phase 0 contracts.

- P3.1 Implement real audio playback (`src-tauri/src/audio_cue.rs`, `rodio`), use existing `src-tauri/sounds/*.wav` and add cancel cue.
- P3.2 Wire into start/stop/cancel/error with timing to reduce beep capture; respects `audio.audio_cues_enabled`.

### Phase 4 — Recording Overlay (M/L)
Depends on: Phase 0 contracts (needs `state_changed` + `audio:level`).

- P4.1 Add overlay window config to `src-tauri/tauri.conf.json`; implement `src-tauri/src/overlay.rs` show/hide/position/click-through gated by `ui.overlay_enabled`.
- P4.2 Add Vite multi-page build (`vite.config.ts`) + `overlay.html` + `src/overlay/*` UI (pill, timer, waveform).

### Phase 5 — Model Catalog + Optional Whisper Support (L)
Depends on: Phase 0 contracts + schema updates.

- P5.1 Add `shared/model/MODEL_CATALOG.json` + manifests; expose catalog via new Tauri command `get_model_catalog`.
- P5.2 Sidecar ASR backend dispatch by `family` (Parakeet vs Whisper); implement Whisper backend (`faster-whisper`) with optional `language` param.
- P5.3 IPC additive: `asr.initialize.language?`; host retries without language if unsupported.
- P5.4 Update UI `src/components/Settings/ModelSettings.tsx` for model selection + language dropdown (Whisper only).

### Phase 6 — Onboarding + Theme Override (M)
Depends on: schema updates in Phase 0.

- P6.1 Onboarding wizard (`src/components/Onboarding/*`) gated by `ui.onboarding_completed` with migration-safe defaulting.
- P6.2 Theme preference (`ui.theme`) with Tailwind `darkMode:'class'`, `src/hooks/useTheme.ts`, and Settings toggle.

### Phase 7 — CI/Packaging Hardening (M/L)
Runs continuously; must be green before release.

- Ensure sidecar packaging includes manifests + presets as data.
- Ensure overlay build works in CI builds.
- Stabilize tests across OS matrix in `.github/workflows/test.yml` and `.github/workflows/build.yml`.

### Parallelization (5 agents)
- Agent A (Contracts/Plumbing): Phase 0 + command/event/type alignment + CI fixes.
- Agent B (Main UI): Phase 1 + Phase 6 UI work.
- Agent C (Tray/Audio): Phase 2 + Phase 3.
- Agent D (Overlay): Phase 4.
- Agent E (Whisper/Model Catalog/Packaging): Phase 5 + sidecar packaging updates.

---

## 9. Testing Strategy
- Frontend: Vitest + Testing Library (`src/tests/*`, `src/hooks/*test.ts`)
  - Tabs keyboard nav, dashboard rendering per state, history search/clear/copy, replacements badge counts, onboarding flow, theme toggling, overlay timer/waveform logic.
- Rust: `cargo test` in `src-tauri`
  - Tray menu builder structure, config migration defaults, notification parsing, audio cue gating (mocked).
- Sidecar: `pytest sidecar/tests`
  - Regression tests for replacements integration, `status.get`, presets loading; whisper dispatch/language parameter (mocked model).
- E2E: `scripts/e2e/run-all.sh` plus targeted OS smoke checks for overlay/tray.
- Test data: no large audio/model artifacts committed; use generated audio for meter tests and mock transcription for unit tests; optional local-only whisper smoke fixture.

---

## 10. Comparison & Trade-offs
### Why this approach
- Contracts-first stabilization prevents drift between UI, overlay, tray, Rust integration, and sidecar.
- Additive-only protocol/schema evolution preserves upgrade safety.
- Event-driven updates reduce polling complexity and keep UI responsive.
- Catalog + per-model manifests scale to future models without breaking the default manifest tooling.

### Trade-offs
- Overlay click-through/always-on-top is inherently OS-fragile; mitigated via `ui.overlay_enabled` and graceful fallback.
- Audio cues may still be picked up acoustically by microphones; delaying start reduces risk but cannot eliminate it.
- Whisper increases packaging complexity and binary size; treated as optional capability with clear UX when unavailable.
- In-memory history resets on restart (privacy-first); persistence should be a future opt-in feature if needed.