# Master Plan — Voice Input Tool (OpenVoicy): Contracts-First Dictation App + Multilingual ASR (Revised)

**Plan version:** 2.0  
**Last updated:** 2026-02-14  
**Scope:** Desktop (Windows/macOS/Linux) cross‑platform dictation with a polished UX, resilient orchestration, and multilingual local ASR.

## Guardrails (non‑negotiable)
- **IPC Protocol V1 is additive-only** (`shared/ipc/IPC_PROTOCOL_V1.md`). New methods/fields are optional; older sidecars remain usable.
- **Config schema stays v1 and additive-only** with safe defaults (`shared/schema/AppConfig.schema.json`, `src-tauri/src/config.rs`).
- **`AppState` semantics stay intact** (`src-tauri/src/state.rs`). We can add details/metadata but do not change meaning of existing states.

---

## 1. Executive Summary

### Problem statement
The repo already has solid primitives (Tauri host, Rust state machine + integration orchestrator, Python sidecar with JSON‑RPC, shared schemas), but the product still has “death-by-a-thousand-cuts” risk:
- **Contract drift** between docs ↔ Rust ↔ UI ↔ sidecar creates invisible breakage (event names/payloads, command stubs, missing `status.get`).
- **Reliability gaps** (sidecar restarts, stale transcription events, device changes) can lead to wrong injections or confusing UI state.
- **User experience** is missing “app-grade” polish: strong status UX, recording controls everywhere (UI/tray/overlay), onboarding, accessibility.
- **Multilingual ASR** is planned, but model management and language UX need more rigor (integrity, install states, per-session language).

### Solution overview
Ship a polished, cross‑platform dictation app by sequencing work into:
1) **Contracts-as-code + baseline stabilization** (single source of truth, contract tests, sidecar spec compliance, supervisor),  
2) **Recording controls + UI/UX coherence** (tabs, dashboard, history + replacements parity),  
3) **System feedback** (overlay, tray, audio cues) built on the same event stream,  
4) **Multilingual expansion** (model catalog, language selection, optional Whisper support),  
5) **Optional power features** (VAD auto‑stop, encrypted persistent history, macros), and  
6) **CI/packaging hardening** for Windows/macOS/Linux.

### Key innovations (revised)
- **Contracts-as-code**: one validated contract spec generates Rust + TS types and test vectors (stops drift permanently).
- **Session + sequence correlated pipeline**: `session_id` + monotonic `seq` on all record/transcribe events; stale events are ignored by design.
- **Supervisor-driven sidecar lifecycle**: health checks, crash-loop protection, structured logs captured for diagnostics.
- **Integrity-verified model installs**: checksums, atomic installs, resumable downloads, and explicit installed/available states.
- **Parity text pipeline**: preview uses the exact same pipeline as injection; rules/presets/macros produce traceable metadata.
- **Privacy-first with opt-in power**: history stays in memory by default; encrypted persistence is explicitly opt‑in.

### Success metrics (what “better” means)
- **Time-to-first-dictation:** < 2 minutes on a clean install (onboarding includes mic + hotkey + model readiness).
- **Crash-loop resilience:** sidecar restart loop never wedges the UI; recovery UI action is always available.
- **No “wrong session” injections:** 0 stale transcription injections due to `session_id`/`seq` gating.
- **Latency:** stop→injection median < 1.2s on a typical laptop for short utterances (after model warm).
- **CPU idle:** overlay + tray idle CPU near zero; audio meter updates throttled.

### Explicit non-goals (for focus)
- Cloud/hosted ASR (everything remains offline/local).
- Always-on wake word / hotword mode (can be a future module, not part of this release).
- Full-fledged voice command framework (we will support lightweight macros only).

---

## 2. Core Architecture

### System diagram (updated)
```text
┌──────────────────────────────────────────────────────────────────────────┐
│                           Tauri Host (Rust)                              │
│                                                                          │
│  Contract layer (generated types + fixtures)                             │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │ shared/contracts/*  →  src-tauri/src/contracts.rs  +  src/types.ts  │  │
│  └────────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│  src-tauri/src/state.rs              src-tauri/src/config.rs             │
│  ┌──────────────────────┐            ┌────────────────────────────────┐  │
│  │ AppStateManager       │            │ AppConfig (schema v1)          │  │
│  │ Idle/Loading/...      │            │ atomic write + migration       │  │
│  └─────────┬────────────┘            └───────────────┬────────────────┘  │
│            │ broadcast(app events)                    │ apply live       │
│  ┌─────────▼──────────────────────────────────────────▼───────────────┐  │
│  │ IntegrationManager (orchestrator)                                  │  │
│  │  - HotkeyManager                                                   │  │
│  │  - RecordingController (UI/tray/hotkey all call same actions)      │  │
│  │  - InjectionController (clipboard restore + retries)               │  │
│  │  - TranscriptHistory (memory; optional encrypted disk)             │  │
│  │  - TrayManager / OverlayManager                                    │  │
│  │  - SidecarSupervisor + RpcClient (NDJSON JSON-RPC over stdio)       │  │
│  └───────────────┬───────────────────────────────────────┬────────────┘  │
│                  │ JSON-RPC calls + captured logs          │ Tauri events  │
└──────────────────▼────────────────────────────────────────▼───────────────┘
                   │                                        │
┌──────────────────▼───────────────────┐     ┌─────────────▼──────────────┐
│          Python Sidecar              │     │   React Main + Overlay      │
│ sidecar/src/openvoicy_sidecar/      │     │ src/App.tsx + src/overlay   │
│  - audio.*, recording.*             │     │ Zustand store + hooks        │
│  - model.*, asr.*                   │     │ listens to canonical events  │
│  - (optional) VAD + preprocess       │     │ (and legacy aliases)        │
└──────────────────────────────────────┘     └─────────────────────────────┘
```

### Design principles (updated)
1. **Contracts are code, not prose**: schemas + generated types + fixtures are the source of truth.
2. **Deterministic state transitions**: state machine + session gating prevents racey “phantom” UI updates.
3. **Fail-soft behavior**: degrade gracefully (clipboard-only injection; overlay disabled; whisper unavailable).
4. **Observability by default**: every session has correlation IDs; diagnostics capture enough to debug without guesswork.
5. **Privacy-first defaults**: no transcript persistence unless explicitly enabled.

### Contracts & versioning strategy (new)
- Add `shared/contracts/` containing:
  - `tauri.commands.v1.json` (command names, params, results)
  - `tauri.events.v1.json` (event names, payload schemas)
  - `sidecar.rpc.v1.json` (JSON-RPC methods + params/results)
  - `examples/*.jsonl` (golden messages used by validators/tests)
- Add generators:
  - `scripts/gen_contracts_ts.py` → updates `src/types.contracts.ts`
  - `scripts/gen_contracts_rs.py` → updates `src-tauri/src/contracts.rs`
- Add validators in CI:
  - `python scripts/validate_contracts.py`
  - `python scripts/validate_ipc_examples.py` (existing; extended)

### Session correlation & sequencing (new)
- `session_id` is created by Rust at recording start and is used everywhere: recording, audio levels, transcription, injection, history.
- Every event sent to UI includes monotonic `seq` (per-app runtime) to allow deterministic ordering / dedupe.
- Sidecar notifications include `session_id` when relevant; Rust drops any notification that does not match the current active session.

### Data flow (happy path, clarified)
1. Startup:
   - Rust loads config → starts **SidecarSupervisor**
   - Supervisor runs `system.ping`/`system.info` → emits `sidecar:status`
   - Rust pulls `status.get` + `model.get_status(model_id?)` → emits `state:changed`, `sidecar:status`, `model:status`
2. Mic test:
   - UI `start_mic_test` → sidecar `audio.meter_start` → sidecar notifies `event.audio_level`
   - Rust forwards as `audio:level` (throttled, includes `seq`)
3. Start recording (from hotkey OR UI OR tray OR overlay):
   - Rust creates `session_id` → `RecordingController::start(session_id)`
   - sidecar `recording.start(session_id, device_uid?, vad?)`
   - Rust emits `recording:status { phase:"recording", session_id }` and `state:changed`
4. Stop/transcribe:
   - `RecordingController::stop(session_id)`
   - sidecar `recording.stop(session_id)` → returns `{ audio_path, audio_duration_ms }`
   - Rust transitions to `Transcribing` → sidecar `asr.transcribe(session_id, audio_path, language?)`
   - Rust applies replacements/macros (or uses sidecar pipeline if configured) → injects → stores history
   - Rust emits `transcript:complete { entry }` and returns to `Idle`

---

## 3. Data Models

### 3.1 AppConfig (schema v1, additive-only)
**Files**
- `shared/schema/AppConfig.schema.json`
- `src-tauri/src/config.rs`
- `src/types.ts`

**Additive fields (revised target)**
UI:
- `ui.theme: "system" | "light" | "dark"` (default `"system"`)
- `ui.onboarding_completed: boolean` (default `false` for new installs; **true on migration** when missing)
- `ui.overlay_enabled: boolean` (default `true`)
- `ui.locale: string | null` (default `null`; future-proof for UI localization)
- `ui.reduce_motion: boolean` (default `false`)

Model / language:
- `model.language: "auto" | string` (default `"auto"`)  
  - If ISO 639-1: request that language when supported.
  - `"auto"`: sidecar auto-detect where supported.
- `model.preferred_device: "auto" | "cpu" | "gpu"` (default `"auto"`) (mapped to `device_pref`)

Audio:
- `audio.trim_silence: boolean` (default `true`)
- `audio.vad_enabled: boolean` (default `false`) (opt-in; reduces accidental cutoffs)
- `audio.vad_silence_ms: number` (default `1200`, clamp 400–5000)
- `audio.vad_min_speech_ms: number` (default `250`, clamp 100–2000)

History:
- `history.persistence_mode: "memory" | "disk"` (default `"memory"`)
- `history.max_entries: number` (default `100`, clamp 10–2000)
- `history.encrypt_at_rest: boolean` (default `true` when `disk`, ignored otherwise)

Injection:
- `injection.restore_clipboard: boolean` (default `true`)
- `injection.app_overrides?: Record<string, { paste_delay_ms?: number; use_clipboard_only?: boolean }>` (default missing)

**Validation rules**
- All new enums invalid → safe defaults.
- Numbers clamped to sane ranges with explicit logging.
- Missing nested objects are treated as defaults (no “null object” traps).

### 3.2 Model catalog + manifests (hardened)
**Files**
- Keep: `shared/model/MODEL_MANIFEST.json` (default model for compatibility)
- Add: `shared/model/MODEL_CATALOG.json`
- Add: `shared/model/manifests/<model_id>.json`

**Catalog entry**
```ts
export type ModelFamily = 'parakeet' | 'whisper';

export interface ModelCatalogEntry {
  model_id: string;
  family: ModelFamily;
  display_name: string;
  description: string;

  supported_languages: string[];  // include "auto" when relevant
  default_language: string;       // usually "auto"

  size_bytes?: number;
  license_spdx?: string;

  manifest_path: string;          // relative to shared/model/
}
```

**Manifest additions (integrity + multi-mirror)**
- `files[]` entries include:
  - `path`
  - `urls[]` (ordered mirrors)
  - `size_bytes`
  - `sha256`
- Sidecar install rules:
  - download to `.partial` → verify hash/size → atomic rename
  - install is atomic per model ID (no half-installed visible state)
  - resume supported when server allows (best effort)

### 3.3 Transcript history (memory by default, disk optional)
**Rust**
- `src-tauri/src/history.rs` `TranscriptEntry` + `TranscriptHistory`.

**Entry fields (revised)**
- `id`
- `timestamp`
- `session_id`
- `raw_text`
- `final_text` (after replacements/macros)
- `language?: string`
- `confidence?: number`
- `timings?: { audio_ms?: number; transcribe_ms?: number; inject_ms?: number }`
- `injection_result` (ClipboardOnly / Injected / Error + details)

Disk persistence (opt-in):
- Encrypted JSONL file under app data, key stored in OS keychain where available.
- If keychain unavailable → fall back to “disk but not encrypted” **only** if user explicitly allows.

### 3.4 Replacement rules + presets + macros
- Schema: `shared/schema/ReplacementRule.schema.json`
- Presets: `shared/replacements/PRESETS.json` (embedded into app + sidecar)

**Revised pipeline**
- Preview and apply must be identical (no “preview lies”).
- Pipeline output includes metadata:
  - `applied_rules_count`
  - `applied_presets[]`
  - `truncated: boolean`

**Sidecar bug fixes remain required**
- Fix imports and tuple handling in `sidecar/src/openvoicy_sidecar/notifications.py`.
- Add unit tests for `process_text` and preset loading.

### 3.5 IPC protocol entities (JSON-RPC v1, additive)
Authoritative: `shared/ipc/IPC_PROTOCOL_V1.md`

**Additive extensions (revised)**
- `recording.start.params.vad?: { enabled:boolean; silence_ms:number; min_speech_ms:number }`
- `recording.stop.result.audio_path?: string` (added field; old clients ignore)
- `asr.initialize.params.language?: string | "auto" | null`
- `asr.transcribe.params.session_id?: string`
- Standard error payloads: `{ code, message, details?, recoverable? }` (consistent across host + sidecar)

### 3.6 Tauri events (Rust → UI/overlay)
**Canonical events (revised naming)**
- `state:changed`: `{ seq, state, enabled, detail?, timestamp }`
- `recording:status`: `{ seq, phase:"idle"|"recording"|"transcribing", session_id?, started_at?, audio_ms? }`
- `model:status`: `{ seq, model_id, status, revision?, cache_path?, progress?, error? }`
- `model:progress`: `{ seq, model_id, current, total?, unit, stage?, current_file?, files_completed?, files_total? }`
- `audio:level`: `{ seq, source:"meter"|"recording", session_id?, rms:number, peak:number }`
- `transcript:complete`: `{ seq, entry: TranscriptEntry }`
- `transcript:error`: `{ seq, session_id?, error }`
- `app:error`: `{ seq, error }`
- `sidecar:status`: `{ seq, state:"starting"|"ready"|"failed"|"restarting"|"stopped", restart_count:number, message? }`

**Compatibility**
- For one release cycle, also emit legacy `state_changed` with the same payload (additive aliasing).

---

## 4. CLI/API Surface

### 4.1 Developer CLI (revised additions)
- Contracts:
  - `python scripts/validate_contracts.py`
  - `python scripts/gen_contracts_ts.py`
  - `python scripts/gen_contracts_rs.py`
- Sidecar:
  - `python -m openvoicy_sidecar.self_test` (fast sanity check; used in CI)
- Existing dev/build/test commands remain.

### 4.2 Tauri Command API (UI → Rust)
Implementation target: `src-tauri/src/commands.rs` (no TODO stubs; all delegate to IntegrationManager)

**Recording (new)**
- `start_recording`
  - Usage: `invoke<void>('start_recording')`
  - Output: `null` (events deliver session/status)
- `stop_recording`
  - Usage: `invoke<void>('stop_recording')`
- `cancel_recording`
  - Usage: `invoke<void>('cancel_recording')`

**Sidecar lifecycle (new)**
- `restart_sidecar`
  - Usage: `invoke<void>('restart_sidecar')`

**Model APIs (revised)**
- `get_model_status`
  - Usage: `invoke<ModelStatus>('get_model_status', { modelId?: string })`
- `download_model`
  - Usage: `invoke<void>('download_model', { modelId?: string, force?: boolean })`
- `get_model_catalog` (unchanged)

**Replacements (revised for parity)**
- Remove Rust-only `preview_replacement`. Replace with:
  - `preview_replacement` → calls sidecar `replacements.preview` (or shared Rust pipeline if that becomes authoritative).
  - Output: `{ result, truncated, applied_rules_count }`

**History (extended)**
- Existing `get_transcript_history`, `copy_*`, `clear_history` remain.
- New:
  - `export_history`
    - Usage: `invoke<string>('export_history', { format:"md"|"csv" })`
    - Output: file path for user to open (and UI can “reveal in folder”)
  - `import_history` (optional)
    - Usage: `invoke<void>('import_history', { path })`

_All existing commands remain; any signature changes are additive-only (optional params)._

### 4.3 Tauri events
See §3.6. UI must subscribe to canonical names, but tolerate legacy aliases during migration.

### 4.4 Sidecar JSON-RPC (Rust → Sidecar) + formats
Transport: NDJSON over stdio.

**System**
- `system.ping`
- `system.info`
- `system.shutdown`

**Status**
- `status.get` → `{ state, detail?, model? }` (**must exist**)

**Audio**
- `audio.list_devices`
- `audio.set_device`
- `audio.meter_start` / `audio.meter_stop` / `audio.meter_status`

**Recording**
- `recording.start { session_id, device_uid?, vad? }`
- `recording.stop { session_id }` → `{ audio_duration_ms, audio_path? }`
- `recording.cancel { session_id }`
- `recording.status`

**Model**
- `model.get_status { model_id? }`
- `model.install { model_id }` (download + verify + ready)
- `model.purge_cache { model_id? }`

**ASR**
- `asr.initialize { model_id, device_pref, language? }`
- `asr.status`
- `asr.transcribe { session_id?, audio_path, language? }`

**Replacements**
- `replacements.get_rules`
- `replacements.set_rules`
- `replacements.get_presets`
- `replacements.get_preset_rules`
- `replacements.preview`

### 4.5 Sidecar notifications (Sidecar → Rust)
- `event.status_changed`
- `event.audio_level` (includes `session_id?`)
- `event.model_progress` (optional future)
- `event.transcription_progress` (optional future)
- `event.transcription_complete` (**must include `session_id`**, `text`, `language?`, `confidence?`, `duration_ms?`)
- `event.transcription_error` (**must include `session_id?`**, standardized error)

---

## 5. Error Handling (revised)

### Standard error object
Use one shape everywhere (Tauri command errors, app events, sidecar errors):
```ts
type AppError = {
  code: string;           // stable identifier, e.g. "E_MIC_PERMISSION"
  message: string;        // user-readable summary
  details?: unknown;      // structured payload for diagnostics
  recoverable: boolean;
};
```

### Recovery strategies (expanded)
- **Sidecar spawn/IPC failure**: Supervisor emits `sidecar:status=failed`; UI shows banner + “Restart sidecar” button; tray mirrors it. Crash-loop protection uses exponential backoff + circuit breaker (requires manual restart after N rapid failures).
- **Mic permission denied** (`E_MIC_PERMISSION`): actionable OS-specific steps + “recheck” flow.
- **Device hot-swap**: on device removal, immediately stop recording (clipboard preserves transcript if already done); fall back to default device and emit `app:error` with guidance.
- **Model install issues**:
  - `E_DISK_FULL`: show required/available space; offer purge.
  - `E_CACHE_CORRUPT`: suggest purge + reinstall (hash mismatch triggers this).
  - `E_NETWORK`: retry with backoff; allow “offline mode” (keep existing installs usable).
- **Injection failures**: never lose transcript; store `ClipboardOnly` with reason; include “copy again” actions.
- **Overlay issues**: auto-disable overlay (set `ui.overlay_enabled=false`) only after repeated failures; always allow re-enable.

---

## 6. Integration Points (revised)

### Dependencies
- Rust: `tauri`, `global_hotkey`, injection backend(s), optional `rodio` for cues.
- Python: `sounddevice`, `numpy`; optional Whisper stack; optional VAD deps (kept lightweight).
- Contracts tooling: python scripts + CI validators.

### Security / privacy
- Never store tokens in config; redact in logs/diagnostics.
- Never log full transcripts by default (only lengths/hashes unless user enables debug).
- Model/license attribution stays in `docs/THIRD_PARTY_NOTICES.md`.

---

## 7. Storage & Persistence (revised)
- Config: platform dir `OpenVoicy/config.json` (plus `.tmp`, `.corrupt`)
- Models: cache dir `.../models/<model_id>/...` with atomic install staging
- Transcript history:
  - default: in-memory ring buffer (size from `history.max_entries`)
  - optional: encrypted JSONL file when `history.persistence_mode="disk"`
- Presets/manifests/contracts: embedded into the app + sidecar package
- Logs:
  - in-memory ring buffer for UI
  - optional file logs for diagnostics export (rotated)

---

## 8. Implementation Roadmap (revised)

### Phase 0 — Contracts-as-Code + Baseline Stabilization (L)
**Must land first.**
- Implement `status.get` and fix sidecar replacements bugs + tests.
- Add contract schemas + generators + CI validators.
- Standardize event naming (`state:changed` canonical + legacy alias).
- Add `session_id` + `seq` propagation and drop-stale logic.
- Implement `SidecarSupervisor` with crash-loop protection and `restart_sidecar` command.

### Phase 1 — Recording Controls + UI Coherence (M)
Depends on: Phase 0.
- Add `start_recording`/`stop_recording`/`cancel_recording` commands; hotkeys call same path.
- Tabs + status dashboard upgrades.
- History + replacements parity UI (preview must match apply).

### Phase 2 — Tray + Overlay + Cues (M/L)
Depends on: Phase 0–1.
- Dynamic tray menu includes: enable toggle, mode, language, mic device, start/stop, recent transcripts, overlay toggle.
- Overlay: minimal CPU, throttled meter, multi-monitor positioning, clear state.
- Real audio cues, gated by config and timed to avoid capture where possible.

### Phase 3 — Audio Quality + VAD Auto‑Stop (M)
Depends on: Phase 1.
- Sidecar preprocess: resample/trim/normalize.
- Optional VAD auto-stop (config-driven) and UI affordances.
- Add tests for VAD edge cases (short utterances, background noise).

### Phase 4 — Model Catalog + Optional Whisper Support (L)
Depends on: Phase 0 + schema updates.
- Add catalog + per-model manifests with checksums.
- Implement `model.install` + integrity verification.
- Whisper backend (optional capability); language dropdown + per-session language toggle.

### Phase 5 — Optional Encrypted Persistent History + Export (M)
Depends on: Phase 1.
- Disk persistence behind explicit toggle; encryption via OS keychain.
- Export to Markdown/CSV; “purge history” controls.

### Phase 6 — Onboarding + Theme + Accessibility (M)
Depends on: schema updates.
- Onboarding wizard: permissions → hotkeys → mic test → model readiness → first dictation.
- Theme override + reduce motion; keyboard nav and ARIA improvements.

### Phase 7 — CI/Packaging Hardening (M/L)
Runs continuously; release gate.
- OS matrix green; sidecar packaging includes contracts/manifests/presets.
- Deterministic build inputs (lockfiles) and security scanning.

### Parallelization (5 agents)
- Agent A: Phase 0 plumbing (contracts, supervisor, session gating)
- Agent B: Phase 1 UI (tabs/dashboard/history/replacements)
- Agent C: Tray + cues (Phase 2)
- Agent D: Overlay (Phase 2)
- Agent E: Models + Whisper + packaging (Phase 4 + CI)

---

## 9. Testing Strategy (revised)

### Contract tests (new cornerstone)
- Validate generated types match schemas.
- Golden JSONL fixtures for:
  - sidecar notifications
  - JSON-RPC requests/responses
  - Tauri events payloads

### Frontend
- Vitest + Testing Library:
  - state/recording badges, history/search/export, replacements parity, onboarding, theme/accessibility toggles, overlay throttling logic.

### Rust
- `cargo test`:
  - supervisor restart policy, stale-event dropping, tray builder snapshots, config migration defaults.

### Sidecar
- `pytest`:
  - `status.get`, preset loading, replacements preview/apply, VAD behavior (synthetic audio), model install hash verification.

### E2E
- Scripted smoke flows and failure recovery:
  - sidecar crash loop recovery
  - device removal mid-recording
  - offline install behavior (existing model usable)

---

## 10. Comparison & Trade-offs (updated)
- Contracts-as-code adds upfront work but pays off by eliminating recurring drift bugs.
- VAD improves UX for many but must be opt-in to avoid surprise cutoffs.
- Encrypted history persistence adds complexity; default remains memory-only for privacy.
- Whisper support increases package size; treated as optional capability with clear UX.

