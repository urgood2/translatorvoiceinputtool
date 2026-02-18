# Master Plan — Voice Input Tool (OpenVoicy): Contracts-First Dictation App + Multilingual ASR

**Plan version:** 1.2 (integrated from v0 + GPT Pro revisions; tightened for brownfield correctness; corrected command inventory + added dedupe/verification specifics; clarified IPC transcription flow + spec/implementation reconciliation)  
**Last updated:** 2026-02-14  
**Scope:** Desktop (Windows/macOS/Linux) cross-platform dictation with polished UX, resilient orchestration, and multilingual local ASR.  
**Lineage:** v0 (Opus baseline) → GPT Pro rev 2.0 → this integrated v1.

## Guardrails (non-negotiable)
- **IPC Protocol V1 is additive-only** (`shared/ipc/IPC_PROTOCOL_V1.md`). New methods/fields are optional; older sidecars remain usable.  
  - **Clarification:** “LOCKED” means **no breaking changes**; additive, opt-in extensions are allowed **only** if they are explicitly documented as optional and the host is tolerant when missing.
- **Contract drift elimination is mandatory (Phase 0)**: reconcile `IPC_PROTOCOL_V1.md`, `shared/ipc/examples/IPC_V1_EXAMPLES.jsonl`, and actual sidecar/Rust/UI behavior; after Phase 0, `shared/contracts/*` + generated types + fixtures become the enforcement mechanism, with `IPC_PROTOCOL_V1.md` kept in lockstep.
- **Config schema stays v1 and additive-only** with safe defaults (`shared/schema/AppConfig.schema.json`, `src-tauri/src/config.rs`).
- **`AppState` semantics stay intact** (`src-tauri/src/state.rs`). We can add details/metadata but do not change meaning of existing states (`Idle`, `LoadingModel`, `Recording`, `Transcribing`, `Error`).
- **Existing Rust module boundaries preserved**: `integration.rs` stays as the orchestrator; `state.rs`, `config.rs`, `history.rs`, `commands.rs`, `recording.rs`, `injection.rs`, `hotkey.rs`, `watchdog.rs`, `sidecar.rs`, `tray.rs`, `focus.rs` all keep their roles.
- **Brownfield guardrails (per `planning/BROWNFIELD_PREFLIGHT.md`)**
  - Do not propose greenfield rewrites when extension/refactor is feasible.
  - Map every planned task to existing files/modules before implementation.
  - Include migration/risk/testing steps for changed runtime behavior.
- **Brownfield mapping enforcement (to operationalize the guardrail):** every roadmap item must list its expected file touchpoints (existing modules to modify + any new files). If a task cannot be mapped to the current repo structure yet, do a small “mapping-only” change first (no behavior changes) and update this plan before feature work.
- **Compatibility window is explicit:** legacy event aliases must remain until (a) frontend listeners default to canonical events, (b) contract fixtures cover both names during the window, and (c) a deliberate follow-up change removes legacy names and fixtures together (no silent removal).

---

## 1. Executive Summary

### Problem statement
The repo has solid primitives (Tauri host, Rust state machine + integration orchestrator, Python sidecar with JSON-RPC, shared schemas), but the product has "death-by-a-thousand-cuts" risk:
- **Contract drift** between docs, Rust, UI, and sidecar creates invisible breakage (event names/payloads, command stubs, missing `status.get`).  
  - **Brownfield reality check (examples to fix in Phase 0):**
    - Rust emits `transcription:complete` but frontend listens to `transcript:complete`.
    - Rust `StateEvent` uses `detail`, while TS types currently use `error_detail`.
    - Rust emits `model:status` as `{ status: <enum> }`, while TS expects `{ model_id, status, ... }`.
    - Sidecar `system.info` shape drifts from `IPC_PROTOCOL_V1.md` (capabilities/runtime fields); contract validators must catch this.
    - Sidecar implements additional v1-compatible methods not currently documented (e.g., `asr.status`, `asr.transcribe`, `recording.status`, `audio.meter_status`, `model.download`, `replacements.preview`); Phase 0 must reconcile whether these are required/optional and document them additively.
- **Reliability gaps** (sidecar restarts, stale transcription events, device changes) can lead to wrong injections or confusing UI state.
- **User experience** is missing "app-grade" polish: strong status UX, recording controls everywhere (UI/tray/overlay), onboarding, accessibility.
- **Multilingual ASR** is planned, but model management and language UX need more rigor (integrity, install states, per-session language).

### Solution overview
Ship a polished, cross-platform dictation app by sequencing work into:
1) **Contracts & baseline stabilization** (contract alignment, sidecar spec compliance, supervisor, session gating),
2) **Recording controls + UI/UX coherence** (tabs, dashboard, history + replacements parity),
3) **System feedback** (overlay, tray, audio cues) built on the same event stream,
4) **Multilingual expansion** (model catalog, language selection, optional Whisper support),
5) **Optional power features** (VAD auto-stop, encrypted persistent history, macros), and
6) **CI/packaging hardening** for Windows/macOS/Linux.

### Key innovations
- **Contracts-as-code**: one validated contract spec generates Rust + TS types and test vectors (stops drift permanently).
- **Session + sequence correlated pipeline**: `session_id` + monotonic `seq` on all record/transcribe events; stale events are ignored by design.
- **Supervisor-driven sidecar lifecycle**: health checks, crash-loop protection, structured logs captured for diagnostics. Builds on existing `watchdog.rs`.
- **Integrity-verified model installs**: checksums, atomic installs, resumable downloads, and explicit installed/available states.
- **Parity text pipeline**: preview uses the exact same pipeline as injection; rules/presets/macros produce traceable metadata.
- **Privacy-first with opt-in power**: history stays in memory by default (existing `TranscriptHistory`); encrypted persistence is explicitly opt-in.

### Success metrics
- **Time-to-first-dictation:** < 2 minutes on a clean install (onboarding includes mic + hotkey + model readiness).
- **Crash-loop resilience:** sidecar restart loop never wedges the UI; recovery UI action is always available.
- **No "wrong session" injections:** 0 stale transcription injections due to `session_id`/`seq` gating.
- **Latency:** stop→injection median < 1.2s on a typical laptop for short utterances (after model warm).
- **CPU idle:** overlay + tray idle CPU < 1% on a typical laptop; audio meter updates throttled.

### Explicit non-goals
- Cloud/hosted ASR (everything remains offline/local).
- Always-on wake word / hotword mode (future module, not this release).
- Full-fledged voice command framework (lightweight macros only, per existing `IPC_PROTOCOL_V1.md` §Macros).

---

## 2. Core Architecture

### System diagram
```text
┌──────────────────────────────────────────────────────────────────────────┐
│                           Tauri Host (Rust)                              │
│                                                                          │
│  Contract layer (generated types + fixtures)                             │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │ shared/contracts/*  →  src-tauri/src/contracts.rs                    │  │
│  │                    →  src/types.contracts.ts (generated)             │  │
│  │                    →  src/types.ts (handwritten wrapper/exports)     │  │
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
│  │  - TranscriptHistory (history.rs) (memory; optional encrypted disk)│  │
│  │  - TrayManager (tray.rs) / OverlayManager (overlay.rs, new)        │  │
│  │  - SidecarSupervisor (supervisor.rs, new; watchdog.rs upgraded)     │  │
│  │  - RpcClient (ipc/*) to sidecar                                    │  │
│  └───────────────┬───────────────────────────────────────┬────────────┘  │
│                  │ JSON-RPC calls + captured logs          │ Tauri events  │
└──────────────────▼────────────────────────────────────────▼───────────────┘
                   │                                        │
┌──────────────────▼───────────────────┐     ┌─────────────▼──────────────┐
│          Python Sidecar              │     │   React Main + Overlay      │
│ sidecar/src/openvoicy_sidecar/      │     │ src/App.tsx + src/overlay   │
│  - audio.*, recording.*             │     │ Zustand store + hooks        │
│  - model.*, asr.*                   │     │ listens to canonical events  │
│  - replacements.*, status.get       │     │ (and legacy aliases)         │
│  - (future) VAD + preprocess        │     └─────────────────────────────┘
└──────────────────────────────────────┘
```

### Design principles
1. **Contracts are code, not prose**: schemas + generated types + fixtures are the source of truth.
2. **One source of truth per concern**: state = `AppStateManager`; config = `AppConfig`; sidecar truth = JSON-RPC + notifications.
3. **Deterministic state transitions**: state machine + session gating prevents racey "phantom" UI updates.
4. **Fail-soft behavior**: degrade gracefully (clipboard-only injection; overlay disabled; whisper unavailable).
5. **Observability by default**: every session has correlation IDs; diagnostics capture enough to debug without guesswork.
6. **Privacy-first defaults**: no transcript persistence unless explicitly enabled.
7. **Additive compatibility**: IPC v1 and config v1 only add optional params/fields with defaults.
8. **Cross-platform first**: explicit handling for Windows/macOS/Linux differences in tray, overlay, permissions.

### Contracts & versioning strategy
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
- **Brownfield integration rule:** generated files (`src/types.contracts.ts`, `src-tauri/src/contracts.rs`) are committed and treated as read-only; manual edits go into non-generated wrappers (`src/types.ts` and Rust modules).
- **Source-of-truth clarification (prevents “two specs” drift):**
  - `shared/ipc/IPC_PROTOCOL_V1.md` remains the **human-readable spec** and must continue to be updated additively when IPC changes.
  - `shared/contracts/sidecar.rpc.v1.json` is the **machine-readable mirror** used for generators/validators; any additive IPC change requires updating **both** the Markdown and the JSON in the same PR.
  - Tauri boundary (`tauri.commands.v1.json`, `tauri.events.v1.json`) is treated as authoritative for host↔UI/overlay; docs can reference it, but contract validators enforce correctness.
- **Concrete contract file structure (to increase specificity/testability):**
  - Each `shared/contracts/*.v1.json` file must include a stable top-level `version: 1` plus an explicit `items` array with stable names (commands/events/methods/notifications), so generators can be deterministic and validators can derive allowlists from a single source.
  - Payload shapes within contract items should be JSON Schema **draft-07** fragments (matches existing `shared/schema/validate.py` tooling), with `$id` values that resolve locally (no network refs).
  - Legacy aliases must be represented explicitly (e.g., `deprecated_aliases: ["state_changed"]` for `state:changed`) so fixtures/tests can cover both names during the compatibility window.
- **Brownfield contract reconciliation (explicit requirement):**
  - Phase 0 audits the sidecar’s actual handler table and the host/frontend usage, and documents any currently-implemented-but-undocumented methods (e.g., `asr.status`, `asr.transcribe`, `recording.status`, `audio.meter_status`, `model.download`, `replacements.get_rules/get_presets/get_preset_rules/preview`) additively in `IPC_PROTOCOL_V1.md`, mirroring them into `shared/contracts/sidecar.rpc.v1.json` with clear **required vs optional** semantics.
  - Avoid “two fixture corpora” drift: start by reusing `shared/ipc/examples/IPC_V1_EXAMPLES.jsonl` as the fixture corpus; if `shared/contracts/examples/*.jsonl` is introduced, it must either be generated from the existing file or become the single canonical fixture source (not a second independently-edited set).
  - **Decision for Phase 0 (clarity):** `shared/ipc/examples/IPC_V1_EXAMPLES.jsonl` remains the canonical, human-edited fixture corpus; any `shared/contracts/examples/*` introduced later must be generated from it (not edited independently).

### Session correlation & sequencing
- `session_id` is created by Rust at recording start and is used everywhere: recording, audio levels, transcription, injection, history. This aligns with the existing `session_id` param in `recording.start` (per `IPC_PROTOCOL_V1.md`).
- Every event sent to UI includes monotonic `seq` (per-app runtime) to allow deterministic ordering / dedupe.
  - **Implementation detail (host):** `seq` is generated in Rust (e.g., `AtomicU64` in the host runtime), increments for every emitted app event, and is included in both canonical and legacy-alias emissions.
  - **Clarification (testability):** `seq` is monotonic within a single app runtime and resets on app restart; frontend dedupe must not assume persistence across restarts.
- Sidecar notifications include `session_id` when relevant (already specified in `IPC_PROTOCOL_V1.md` for `event.audio_level` and `event.transcription_complete`); Rust drops any notification that does not match the current active session.  
  - **Implementation detail:** “drop-stale” means: ignore if `session_id != current_session_id`. (Sidecar notifications do not include `seq`; `seq` is used for UI event ordering/deduping and legacy alias dedupe.)

### Data flow (happy path, clarified)
1. Startup:
   - Rust loads config → starts sidecar via `SidecarSupervisor` (evolved from `watchdog.rs`)
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
   - sidecar `recording.stop(session_id)` → returns `{ audio_duration_ms }` (and may optionally return `{ audio_path? }` as an additive extension)
   - Rust transitions to `Transcribing` and waits for sidecar async notifications:
     - success: `event.transcription_complete` (required in IPC v1)
     - failure: `event.transcription_error` (required in IPC v1)
   - **Optional extension (not required for the default recording flow):** if sidecar supports `asr.transcribe`, Rust may call it for explicit file/session re-transcription flows, gated behind capability/method availability; it must not be required to complete `recording.stop`→`event.transcription_complete` behavior.
   - **Sidecar applies normalization/macros/replacements** per `IPC_PROTOCOL_V1.md` pipeline; Rust must not re-apply replacements to avoid double transforms.
   - Rust injects via `injection.rs` (with focus guard from `focus.rs`) → stores in `TranscriptHistory`
   - Rust emits `transcript:complete { entry }` and returns to `Idle`

---

## 3. Data Models

### 3.1 AppConfig (schema v1, additive-only)
**Files**
- `shared/schema/AppConfig.schema.json` (canonical JSON Schema)
- `src-tauri/src/config.rs` (Rust struct + validation)
- `src/types.ts` (TypeScript types)

**Existing fields** (preserved as-is):
- `audio.device_uid`, `audio.audio_cues_enabled`
- `hotkeys.primary`, `hotkeys.copy_last`, `hotkeys.mode`
- `injection.paste_delay_ms`, `injection.restore_clipboard`, `injection.suffix`, `injection.focus_guard_enabled`
- `model.model_id`, `model.device`
- `replacements[]`
- `ui.show_on_startup`, `ui.window_width`, `ui.window_height`
- `presets.enabled_presets`

**Additive fields (new)**

UI:
- `ui.theme: "system" | "light" | "dark"` (default `"system"`)
- `ui.onboarding_completed: boolean` (default `false` for new installs; **true on migration** when missing)
  - **Migration specificity:** treat “migration” as “an existing config file was loaded that predates this field”; when loading an existing config missing this key, set it to `true` so existing users are not forced through onboarding. On truly fresh installs (no prior config), leave as `false`.
- `ui.overlay_enabled: boolean` (default `true`)
- `ui.locale: string | null` (default `null`; future-proof for UI localization)
- `ui.reduce_motion: boolean` (default `false`)

Model / language:
- `model.language: "auto" | string | null` (default `null`)
  - `null`: no preference, sidecar decides.
  - `"auto"`: sidecar auto-detect where supported.
  - ISO 639-1 code: request that language when supported.
- `model.preferred_device: "auto" | "cpu" | "gpu"` (default `"auto"`) — NOTE: maps to existing `model.device` / sidecar's `device_pref`. Consider whether this should coexist with or replace `model.device`. **Decision: keep as alias mapped to `device_pref` at the sidecar boundary. `model.device` remains for backwards compat.**  
  - **Brownfield clarification:** existing schema enumerates `"auto"|"cpu"|"cuda"|"mps"`; implementation should store concrete device strings (`cuda`/`mps`) even if UI shows a single “GPU” option.
  - **Precedence specificity:** when both `model.device` and `model.preferred_device` are present, compute an effective `device_pref` as:
    - if `model.device` is a concrete backend (`"cuda"|"mps"`) use it;
    - else map `model.preferred_device` (`gpu→best available backend`, `cpu→cpu`, `auto→auto`).
    - This preserves backward-compat for configs that already store concrete `model.device`.

Audio:
- `audio.trim_silence: boolean` (default `true`)
- `audio.vad_enabled: boolean` (default `false`) (opt-in; reduces accidental cutoffs)
- `audio.vad_silence_ms: number` (default `1200`, clamp 400–5000)
- `audio.vad_min_speech_ms: number` (default `250`, clamp 100–2000)

History:
- `history.persistence_mode: "memory" | "disk"` (default `"memory"`)
- `history.max_entries: number` (default `100`, clamp 10–2000) — NOTE: existing `TranscriptHistory` defaults to 20; this raises it.
- `history.encrypt_at_rest: boolean` (default `true` when `disk`, ignored otherwise)

Injection:
- `injection.app_overrides?: Record<string, { paste_delay_ms?: number; use_clipboard_only?: boolean }>` (default absent)

**Validation rules**
- All new enums invalid → safe defaults.
- Numbers clamped to sane ranges with explicit logging.
- Missing nested objects are treated as defaults (no "null object" traps).
- Existing clamps remain (`paste_delay_ms` 10–500, window dims ≥ 200, non-empty hotkeys).

### 3.2 Model catalog + manifests (hardened)
**Files**
- Keep: `shared/model/MODEL_MANIFEST.json` (default model for compatibility)
- Add: `shared/model/MODEL_CATALOG.json`
- Add: `shared/model/manifests/<model_id>.json`
- (Added specificity) Add schemas for validation (draft-07, local refs only):
  - `shared/schema/ModelCatalog.schema.json`
  - `shared/schema/ModelManifest.schema.json`
  - Extend `python scripts/validate_model_manifest.py` to validate the legacy `MODEL_MANIFEST.json` plus the new catalog/manifests (including `sha256` format and `size_bytes` checks).

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
**Rust**: `src-tauri/src/history.rs` — existing `TranscriptEntry` + `TranscriptHistory` (ring buffer).

**Entry fields (extended)**
Existing fields preserved:
- `id` (Uuid)
- `text` (String)
- `timestamp` (DateTime<Utc>)
- `audio_duration_ms` (u32)
- `transcription_duration_ms` (u32)
- `injection_result` (HistoryInjectionResult)

New fields:
- `session_id: Uuid`
- `raw_text: String` (before replacements/macros)
- `final_text: String` (after replacements/macros — replaces role of `text` for display, but `text` stays for backward compat)
  - **Compatibility specificity:** during the transition, keep `text` equal to `final_text` so older UI paths that render `text` continue to show the final, post-processed content. `final_text` exists to make the “raw vs final” distinction explicit without breaking existing consumers.
- `language?: String`
- `confidence?: f32`
- `timings?: { inject_ms?: u32 }` (audio_ms and transcribe_ms already exist as top-level fields)  
  - **Brownfield clarification:** since `IPC_PROTOCOL_V1.md` currently only guarantees final `text`, `raw_text` may initially be set equal to `final_text` until the sidecar optionally emits `raw_text` (additive).

Disk persistence (opt-in, Phase 5):
- Encrypted JSONL file under app data, key stored in OS keychain where available.
- If keychain unavailable → fall back to "disk but not encrypted" **only** if user explicitly allows.

### 3.4 Replacement rules + presets + macros
- Schema: `shared/schema/ReplacementRule.schema.json` (existing)
- Presets: `shared/replacements/PRESETS.json` (existing, embedded into app + sidecar)

**Revised pipeline**
- Preview and apply must be identical (no "preview lies").
- Pipeline output includes metadata:
  - `applied_rules_count`
  - `applied_presets[]`
  - `truncated: boolean`

**Sidecar bug fixes remain required**
- Fix imports and tuple handling in `sidecar/src/openvoicy_sidecar/notifications.py`.
- Add unit tests for `process_text` and preset loading.
- **Brownfield correctness fix:** Rust `ReplacementRule` in `src-tauri/src/config.rs` must be aligned to `shared/schema/ReplacementRule.schema.json` (additive migration):
  - Add missing fields (`id`, `kind`, `word_boundary`, `case_sensitive`, `description?`, `origin?`) with defaults.
  - Migration strategy: existing saved rules without `id` get generated IDs; missing flags default to schema defaults.

### 3.5 IPC protocol entities (JSON-RPC v1, additive)
Authoritative: `shared/ipc/IPC_PROTOCOL_V1.md` (LOCKED v1.0)

**Brownfield reconciliation note (must be made concrete in Phase 0):**
- The sidecar currently implements several v1-compatible methods beyond what `IPC_PROTOCOL_V1.md` documents (e.g., `asr.status`, `asr.transcribe`, `recording.status`, `audio.meter_status`, `model.download`, and multiple `replacements.*` getters/preview). Phase 0 updates `IPC_PROTOCOL_V1.md` additively to document these and to declare which are **required** for host features vs **optional** (with host-side fallback behavior).

**Additive extensions** (all optional fields; old sidecars ignore):
- `recording.start.params.vad?: { enabled:boolean; silence_ms:number; min_speech_ms:number }`
- `recording.stop.result.audio_path?: string`
- `asr.initialize.params.language?: string | "auto" | null`
- `asr.transcribe.params.session_id?: string`
- Standard error payloads: `{ code, message, details?, recoverable? }` (consistent with existing `error.data.kind` convention from IPC_PROTOCOL_V1.md)

**New method (additive)**
- `model.install { model_id }` → `{ status:"installing" }` (download + verify + ready)  
  - **Brownfield clarification:** current sidecar already implements `model.download`; host should treat `model.install` as optional and fall back to `model.download`/`asr.initialize` until `model.install` exists everywhere.

### 3.6 Tauri events (Rust → UI/overlay)
**Canonical events (revised naming)**
- `state:changed`: `{ seq, state, enabled, detail?, timestamp }` — NOTE: rename from existing `state_changed`
- `recording:status`: `{ seq, phase:"idle"|"recording"|"transcribing", session_id?, started_at?, audio_ms? }` — NEW
- `model:status`: `{ seq, model_id, status, revision?, cache_path?, progress?, error? }`
- `model:progress`: `{ seq, model_id, current, total?, unit, stage?, current_file?, files_completed?, files_total? }`
- `audio:level`: `{ seq, source:"meter"|"recording", session_id?, rms:number, peak:number }`
- `transcript:complete`: `{ seq, entry: TranscriptEntry }`
- `transcript:error`: `{ seq, session_id?, error }` — NEW
- `app:error`: `{ seq, error }` — now includes structured error, not just message string
- `sidecar:status`: `{ seq, state:"starting"|"ready"|"failed"|"restarting"|"stopped", restart_count:number, message? }`

**Compatibility**
- For one release cycle, also emit legacy `state_changed` (no colon) with the same payload as `state:changed`.
- Frontend `useTauriEvents.ts` currently listens to `state_changed` — it must be updated to listen to `state:changed` and tolerate legacy.
- **Brownfield additions (must be handled during Phase 0):**
  - Also emit legacy transcript events while migrating:
    - Legacy: `transcription:complete` (current Rust emission) → Canonical: `transcript:complete`
    - Legacy: `transcription:error` (current Rust emission) → Canonical: `transcript:error`
  - Also bridge sidecar status events:
    - Legacy: `status:changed` (current Rust forward) → Canonical: `sidecar:status` (structured + supervisor state)
  - For `model:status`, emit both:
    - Legacy payload shape currently used in Rust (e.g. `{ status: <enum> }`) and the canonical `{ model_id, status, ... }` until frontend types/store are updated.
- **Dedupe requirement (prevents double-processing when listening to canonical + legacy):**
  - During the compatibility window, the frontend should either:
    - listen only to canonical names once they exist (preferred), OR
    - listen to both canonical and legacy aliases and dedupe by `seq` (store “last seen seq” per event stream), so the same state/transcript update is applied exactly once.
  - The host should ensure canonical payloads are **supersets** where feasible (e.g., a canonical `model:status` payload that includes `status` also satisfies any legacy consumer that only looked at `status`), minimizing the need to emit two separate `model:status` events.

---

## 4. CLI/API Surface

### 4.1 Developer CLI
Existing commands preserved:
- Beads: `bd onboard`, `bd ready`, `bd show`, `bd update`, `bd close`, `bd sync`
- Dev/build: `bun run tauri dev`, `bun run build`, `bun run test`, `bun run test:watch`, `bun run test:coverage`, `bun run lint`
- Schema/tools: `python shared/schema/validate.py --self-test`, `python scripts/validate_model_manifest.py`, `python scripts/validate_ipc_examples.py`
- Sidecar packaging: `./scripts/build-sidecar.sh`, `./scripts/bundle-sidecar.sh`
- E2E scripts: `./scripts/e2e/run-all.sh`, etc.  
  - **Brownfield note:** commands can also be run via `npm run ...` (per `package.json`), but `bun` is preferred because `bun.lock` is present.
  - (Added clarity) CI should continue to use the repo’s standard script runner; local dev can use `bun` or `npm`, but the plan’s verification commands must be runnable in CI without requiring nonstandard tooling.

New additions:
- Contracts:
  - `python scripts/validate_contracts.py`
  - `python scripts/gen_contracts_ts.py`
  - `python scripts/gen_contracts_rs.py`
- Sidecar:
  - `python -m openvoicy_sidecar.self_test` (fast sanity check; used in CI)

### 4.2 Tauri Command API (UI → Rust)
Implementation target: `src-tauri/src/commands.rs` (no TODO stubs; all delegate to IntegrationManager)  
- **Brownfield clarity:** commands will take `tauri::State<IntegrationState>` where needed; this does not change JS `invoke()` signatures.
- **Brownfield reality check (current state):** several commands in `src-tauri/src/commands.rs` are placeholders (`NotImplemented`) or return stub data; Phase 0 must convert these to real implementations by delegating through `IntegrationState` + sidecar RPC per `shared/ipc/IPC_PROTOCOL_V1.md`.
- **Inventory note (prevents confusion):** the list below is the **expected stable JS-facing surface**; some items are currently missing/stubbed and must be implemented (additively) in Phase 0 while keeping names stable.

**Existing commands** (preserved as-is):
- `get_app_state`, `get_capabilities`, `get_capability_issues`, `can_start_recording`
- `run_self_check`
- `get_config`, `update_config`, `reset_config_to_defaults`
- `list_audio_devices`, `set_audio_device`, `start_mic_test`, `stop_mic_test`
- `get_model_status`, `download_model`, `purge_model_cache`, `get_model_catalog`
- `get_transcript_history`, `copy_transcript`, `copy_last_transcript`, `clear_history`
- `get_hotkey_status`, `set_hotkey`
- `get_replacement_rules`, `set_replacement_rules`, `preview_replacement`
- `get_available_presets`, `load_preset`
- `toggle_enabled`, `is_enabled`, `set_enabled`
- `generate_diagnostics`, `get_recent_logs`  
  - **Brownfield note:** `get_model_catalog` is not currently implemented in `src-tauri/src/commands.rs`; it is treated as part of “remove TODO stubs / contract alignment” work.
  - **Inventory correction (specific):** `get_model_catalog` is not currently present as a `#[tauri::command]` function in `src-tauri/src/commands.rs`; Phase 0 should add it (additive) and keep all existing command names stable.

**New commands:**

Recording:
- `start_recording` — `invoke<void>('start_recording')` — creates session_id, delegates to RecordingController
- `stop_recording` — `invoke<void>('stop_recording')` — stops current session
- `cancel_recording` — `invoke<void>('cancel_recording')` — cancels without transcription

Sidecar lifecycle:
- `restart_sidecar` — `invoke<void>('restart_sidecar')` — forces sidecar restart

Model (extended signatures):
- `get_model_status` gains optional `{ modelId?: string }` param
- `download_model` gains optional `{ modelId?: string, force?: boolean }` params

Replacements (parity fix):
- `preview_replacement` updated to call sidecar `replacements.preview` for pipeline parity
- Output: `{ result, truncated, applied_rules_count }`

History (extended):
- `export_history` — `invoke<string>('export_history', { format:"md"|"csv" })` — returns file path

_All existing command signatures remain; any changes are additive-only (optional params)._

### 4.3 Tauri events
See §3.6. UI must subscribe to canonical names, but tolerate legacy aliases during migration.

### 4.4 Sidecar JSON-RPC (Rust → Sidecar) + formats
Transport: NDJSON over stdio. All methods per `shared/ipc/IPC_PROTOCOL_V1.md`.

**Clarification (brownfield correctness):**
- Some methods listed below are already implemented in the sidecar but not fully documented in `IPC_PROTOCOL_V1.md` today; Phase 0 updates the spec additively and mirrors into `shared/contracts/sidecar.rpc.v1.json`. Until that lands, the host must treat any “extra” methods as **optional** (gate behind capability/method availability and/or tolerate `E_METHOD_NOT_FOUND`).

**System**
- `system.ping`, `system.info`, `system.shutdown`

**Status**
- `status.get` → `{ state, detail?, model? }` (**must exist** — currently missing in sidecar, fix required)

**Audio**
- `audio.list_devices`, `audio.set_device`, `audio.meter_start`, `audio.meter_stop`, `audio.meter_status`

**Recording**
- `recording.start { session_id, device_uid?, vad? }` (vad is additive)
- `recording.stop { session_id }` → `{ audio_duration_ms, audio_path? }` (audio_path is additive)  
  - **IPC v1 behavior requirement:** `recording.stop` begins transcription asynchronously and the final result is delivered via `event.transcription_complete` / `event.transcription_error`. Any additional method (e.g., `asr.transcribe`) must not replace or break this core behavior.
- `recording.cancel { session_id }`
- `recording.status`

**Model**
- `model.get_status { model_id? }`
- `model.install { model_id }` (NEW — download + verify + ready)
- `model.purge_cache { model_id? }`  
  - **Brownfield note:** current sidecar also supports `model.download`; host should be tolerant and use whichever is available.

**ASR**
- `asr.initialize { model_id, device_pref, language? }` (language is additive)
- `asr.status`
- `asr.transcribe { audio_path, session_id?, language? }` (session_id, language are additive)  
  - **Clarification:** this is an optional extension for explicit transcription flows; the default dictation path remains `recording.stop` → notifications.

**Replacements**
- `replacements.get_rules`, `replacements.set_rules`, `replacements.get_presets`, `replacements.get_preset_rules`, `replacements.preview`

### 4.5 Sidecar notifications (Sidecar → Rust)
Per `IPC_PROTOCOL_V1.md`:
- `event.status_changed`
- `event.audio_level` (includes `session_id` when `source=recording`)
- `event.transcription_complete` (**must include `session_id`**, `text`, `confidence?`, `duration_ms`)
- `event.transcription_error` (**must include `session_id`**, `kind`, `message`)

Additive (future):
- `event.model_progress` (optional; for long downloads)

---

## 5. Error Handling

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
NOTE: This is the **Tauri event / UI-facing shape**. The existing `CommandError` enum in `commands.rs` and the `error.data.kind` convention in `IPC_PROTOCOL_V1.md` continue to serve their respective transport layers. The `AppError` shape is used for the `app:error` event and UI error display.  
- **Compatibility requirement:** keep emitting the legacy `{ message, recoverable }` payload shape (or tolerate it in frontend) for one release cycle while migrating to `{ seq, error: AppError }`.
  - **Specific compatibility strategy:** emit `app:error` with a payload that includes `message`/`recoverable` **and** `{ seq, error }` so both old and new UI consumers can read what they need without duplicate events.

### Recovery strategies
- **Sidecar spawn/IPC failure**: Supervisor emits `sidecar:status=failed`; UI shows banner + "Restart sidecar" button; tray mirrors it. Crash-loop protection uses exponential backoff + circuit breaker (requires manual restart after N rapid failures). Builds on existing `watchdog.rs` (`WatchdogConfig` with `max_restart_count`, `backoff_factor`).  
  - **Brownfield clarification:** current `WatchdogConfig` does not yet include `max_restart_count`/`backoff_factor`; add them (or introduce a `SidecarSupervisorConfig`) as part of Phase 0.5 while keeping existing fields (`check_interval`, `ping_timeout`, `hang_threshold`, `auto_restart_on_hang`) intact.
- **Mic permission denied** (`E_MIC_PERMISSION`): actionable OS-specific steps + "recheck" flow. Already defined in `IPC_PROTOCOL_V1.md`.
- **Device hot-swap**: on device removal, immediately stop recording (clipboard preserves transcript if already done); fall back to default device and emit `app:error` with guidance.
- **Model install issues**:
  - `E_DISK_FULL`: show required/available space; offer purge.
  - `E_CACHE_CORRUPT`: suggest purge + reinstall (hash mismatch triggers this).
  - `E_NETWORK`: retry with backoff; allow "offline mode" (keep existing installs usable).
- **Injection failures**: never lose transcript; store `ClipboardOnly` with reason (existing `HistoryInjectionResult` handles this); include "copy again" actions.
- **Overlay issues**: auto-disable overlay (set `ui.overlay_enabled=false`) only after repeated failures; always allow re-enable.
- **IPC drift** (older sidecar): when `language` rejected, retry initialize without it; surface "Whisper not supported in this build".

---

## 6. Integration Points

### Dependencies
- Rust: `tauri`, `global_hotkey`, platform-specific injection backends in `injection.rs`, optional `rodio` for audio cues (existing `sounds/*.wav`).
- Python: `sounddevice`, `numpy`; optional `faster-whisper` (+ `ctranslate2`) for Whisper; optional VAD deps (kept lightweight).
- Contracts tooling: python scripts + CI validators.

### Security / privacy
- Never store tokens in config; redact in logs/diagnostics.
- Never log full transcripts by default (only lengths/hashes unless user enables debug).
- Model/license attribution stays in `docs/THIRD_PARTY_NOTICES.md` (existing).
- Default mirrors `auth_required=false`. Optional HF token via env var `HF_TOKEN` (never stored in config).

---

## 7. Storage & Persistence
- Config: platform dir `OpenVoicy/config.json` (plus `.tmp`, `.corrupt`) — existing in `config.rs`.
- Models: cache dir (managed by sidecar, e.g. `~/.cache/openvoicy/models/<model_id>/...`) with atomic install staging.
- Transcript history:
  - default: in-memory ring buffer (`history.rs`, size from `history.max_entries`)
  - optional: encrypted JSONL file when `history.persistence_mode="disk"` (Phase 5)
- Presets/manifests/contracts: embedded into the app + sidecar package.
- Logs: in-memory ring buffer (`log_buffer.rs`); optional file logs for diagnostics export (rotated).
- Frontend build output: `dist/`; overlay adds an additional built HTML entry.

---

## 8. Implementation Roadmap

Phased delivery with dependencies and complexity (S/M/L). Designed for 3–5 parallel agents.

### Phase 0 — Contracts-as-Code + Baseline Stabilization (L)
**Must land first.**

- **P0.1 Sidecar spec compliance + blocking bug fixes (M)**
  - Implement `status.get` in `sidecar/src/openvoicy_sidecar/server.py`.
  - Fix replacements integration in `sidecar/src/openvoicy_sidecar/notifications.py` (missing `get_current_rules`, tuple return).
  - Load presets on startup from `shared/replacements/PRESETS.json` (packaged resource path in release).
  - **Spec drift fix (must be explicit):** align `system.info` response to the required `IPC_PROTOCOL_V1.md` fields (`capabilities: string[]`, `runtime.python_version/platform/cuda_available`) while allowing additive extra detail (e.g., optional `capabilities_detail` object) so hosts can rely on a stable baseline.
  - (Added specificity) Add `sidecar/src/openvoicy_sidecar/self_test.py` (or equivalent module) so `python -m openvoicy_sidecar.self_test` runs a fast, deterministic sanity check of required IPC (at least `system.ping`, `system.info`, `status.get`, and one replacements path) without needing large model downloads.
  - Add regression tests under `sidecar/tests/` for the above.  
  - **Acceptance criteria**
    - `status.get` matches `IPC_PROTOCOL_V1.md` shape and is included in the handler dispatch table.
    - `event.transcription_complete.params.text` is a string (not a tuple), and replacements/macros are applied exactly once.
    - Presets load in dev and in packaged builds (resource path resolved).
    - `system.info` includes the required baseline fields (per spec) and does not regress existing consumers.
    - `python -m openvoicy_sidecar.self_test` exits 0 in dev and in packaged builds, and fails nonzero with a clear message when required IPC handlers are missing.
  - **Verification**
    - `pytest sidecar/tests`
    - `python scripts/validate_ipc_examples.py`
    - `python -m openvoicy_sidecar.self_test`

- **P0.2 Rust↔UI contracts (L)**
  - Make `src-tauri/src/commands.rs` delegate to `IntegrationState` + sidecar RPC (remove TODO `NotImplemented` paths for devices/model/meter/presets).
  - Standardize Tauri event names: emit `state:changed` canonical + `state_changed` legacy alias.
  - Fix sidecar notification parsing in `integration.rs` to match `IPC_PROTOCOL_V1.md`.
  - Add `session_id` + `seq` propagation and drop-stale logic in `integration.rs`.  
  - (Added completeness) Extend `src-tauri/src/history.rs` `TranscriptEntry` to include `session_id` (and optional fields from §3.3 as available), keep backward-compat by continuing to populate `text` (and set `raw_text==final_text` until sidecar emits raw) and update:
    - emitted `transcript:complete` payload schema/types,
    - `src/types.ts` `TranscriptEntry` (additive fields only),
    - any store reducers/selectors that assume only the old fields.
  - (Added specificity) Implement `recording:status` emission at least for `idle→recording→transcribing→idle` transitions, with `session_id` where applicable, even if the UI initially uses only a subset.
  - (Added specificity) Ensure `app:error` emission follows §5 compatibility strategy (payload includes both legacy `message/recoverable` and `{ seq, error }`) so frontend changes can be incremental.
  - **Brownfield must-fix drift included in this item**
    - Emit `transcript:complete` while preserving legacy `transcription:complete` for one release cycle.
    - Emit `transcript:error` while preserving legacy `transcription:error` for one release cycle.
    - Ensure UI actually receives state change events (not just tray updates).
    - Align TS types in `src/types.ts` with Rust payload keys (`detail` vs `error_detail`, model event shapes).
  - **File touchpoints (brownfield mapping)**
    - Rust: `src-tauri/src/integration.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/history.rs`, `src-tauri/src/state.rs`, `src-tauri/src/ipc/types.rs` (as needed for payload structs)
    - Frontend: `src/hooks/useTauriEvents.ts`, `src/store/*` (where event payloads are applied), `src/types.ts`, `src/hooks/useTauriEvents.test.ts`
  - **Acceptance criteria**
    - Frontend can run without console spam/errors from missing events; store updates on state/model/transcript events.
    - All existing `#[tauri::command]` endpoints return non-placeholder data where defined in plan (no `NotImplemented` for core flows).
    - Frontend dedupes canonical+legacy aliases (if listening to both) using `seq`, so history/state are not double-applied.
    - Transcript history entries include `session_id` (and do not break existing UI that expects `text`).
  - **Verification**
    - `bun run test`
    - `cargo test` (in `src-tauri`)
    - `python scripts/validate_contracts.py` (once P0.4 lands)

- **P0.3 Schema/type alignment (M)**
  - Add config fields: `ui.theme`, `ui.onboarding_completed`, `ui.overlay_enabled`, `model.language` across schema, Rust, TS.
  - (Added completeness) Add the rest of the additive config fields introduced in §3.1 across schema, Rust, TS (with safe defaults and clamping):
    - UI: `ui.locale`, `ui.reduce_motion`
    - Model/language: `model.preferred_device` (alias behavior per §3.1)
    - Audio: `audio.trim_silence`, `audio.vad_enabled`, `audio.vad_silence_ms`, `audio.vad_min_speech_ms`
    - History: `history.persistence_mode`, `history.max_entries`, `history.encrypt_at_rest`
    - Injection: `injection.app_overrides`
  - Update `shared/schema/validate.py` self-tests/examples accordingly.  
  - **Brownfield addition:** align `ReplacementRule` Rust/TS/schema shapes as described in §3.4 with a migration step.
  - **File touchpoints (brownfield mapping)**
    - `shared/schema/AppConfig.schema.json`
    - `src-tauri/src/config.rs`
    - `src/types.ts`
    - `shared/schema/validate.py`
  - **Acceptance criteria**
    - `python shared/schema/validate.py --self-test` passes.
    - Existing config files load and round-trip with defaults applied; no runtime panics on missing new fields.

- **P0.4 Contract schemas + generators (M)**
  - Add `shared/contracts/` with command, event, and RPC schemas.
  - Add generators (`gen_contracts_ts.py`, `gen_contracts_rs.py`).
  - Add CI validators.
  - **Spec/impl reconciliation (explicit):**
    - Audit sidecar handler inventory vs `IPC_PROTOCOL_V1.md` and document missing-but-implemented methods additively (keeping IPC v1 compatibility and marking optional where needed).
    - Mirror those decisions into `shared/contracts/sidecar.rpc.v1.json` so generators/validators reflect reality.
    - Reduce future drift by updating `scripts/validate_ipc_examples.py` to be contract-driven (derive the allowed method/notification sets from `shared/contracts/sidecar.rpc.v1.json`, or from a single shared list generated from it).
  - (Added specificity) Ensure contract coverage includes the migration window:
    - canonical + legacy event names for `state:changed`/`state_changed`, `transcript:complete`/`transcription:complete`, `transcript:error`/`transcription:error`
    - payload schema for `TranscriptEntry` including additive fields (`session_id`, etc.)
    - `recording:status` and `sidecar:status` payload schemas
  - **File touchpoints (brownfield mapping)**
    - `shared/contracts/*`
    - `scripts/gen_contracts_ts.py`, `scripts/gen_contracts_rs.py`, `scripts/validate_contracts.py`
    - `scripts/validate_ipc_examples.py`
  - **Acceptance criteria**
    - Generators are deterministic (no local absolute paths/timestamps in output).
    - CI fails if contracts and generated types are out of date.

- **P0.5 Sidecar supervisor (M)**
  - Evolve existing `watchdog.rs` into `SidecarSupervisor` with crash-loop protection and `restart_sidecar` command.
  - Emit structured `sidecar:status` events.  
  - **Brownfield clarification:** keep `watchdog.rs` as health monitoring + resume handling; add lifecycle/restart policy in a supervisor layer (new `supervisor.rs` or equivalent), potentially extending `WatchdogConfig` or adding `SidecarSupervisorConfig` (`max_restart_count`, `backoff_factor`, circuit breaker thresholds).
  - **File touchpoints (brownfield mapping)**
    - `src-tauri/src/watchdog.rs` (health)
    - `src-tauri/src/sidecar.rs` (spawn/stdio)
    - `src-tauri/src/integration.rs` (wiring + event emission)
    - `src-tauri/src/commands.rs` (`restart_sidecar`)
    - New: `src-tauri/src/supervisor.rs` (lifecycle policy)
  - **Acceptance criteria**
    - Repeated sidecar crashes do not wedge UI; `restart_sidecar` reliably recovers in normal cases.
    - `sidecar:status` includes `restart_count` and a stable `state` enum.
  - **Verification**
    - `cargo test` (in `src-tauri`)
    - `scripts/e2e/test-error-recovery.sh` (targeted)

### Phase 1 — Recording Controls + UI Coherence (M)
Depends on: Phase 0.

- P1.1 Add `start_recording`/`stop_recording`/`cancel_recording` Tauri commands; hotkeys, UI, and (later) tray all call same path.
  - (Added specificity) Ensure these commands also drive `recording:status` transitions (`idle→recording→transcribing→idle`) and preserve “cancel without transcription” semantics.
  - **File touchpoints (brownfield mapping)**
    - `src-tauri/src/commands.rs`, `src-tauri/src/recording.rs`, `src-tauri/src/integration.rs`, `src-tauri/src/state.rs`
    - Frontend invocation sites: `src/App.tsx`, existing settings components (as needed)
- P1.2 Top-level tabs: `src/components/Layout/TabBar.tsx`, `TabPanel.tsx`; wire in `src/App.tsx`.
  - **File touchpoints (brownfield mapping)**
    - `src/App.tsx`, new `src/components/Layout/*`, existing `src/components/Settings/SettingsPanel.tsx` (as source for refactors)
- P1.3 Status dashboard: `src/components/Status/StatusDashboard.tsx` (state, hotkey/mode, last transcript, model + sidecar badges).
  - **File touchpoints (brownfield mapping)**
    - new `src/components/Status/*`, existing `src/components/StatusIndicator.tsx` (reuse/refactor), `src/store/*`, `src/hooks/useTauriEvents.ts`
- P1.4 History panel: move to `src/components/History/HistoryPanel.tsx`; add search + clear-all confirm.
  - **File touchpoints (brownfield mapping)**
    - existing `src/components/Settings/HistoryPanel.tsx` (refactor source), new `src/components/History/*`, `src/store/*`, `src/types.ts`
- P1.5 Replacements tab: integrate existing `ReplacementList` + `PresetsPanel`; add tab badge counts. Preview must match apply (parity fix).  
  - **File touchpoints (brownfield mapping)**
    - `src/components/Replacements/*`, `src/store/*`, `src/types.ts`, `src/hooks/useTauriEvents.ts` (if any event-driven updates)
- **Acceptance criteria**
  - Recording can be started/stopped from UI without relying on hotkeys.
  - Replacements preview uses sidecar pipeline; UI preview matches injected output for same input.
- **Verification**
  - `bun run test`
  - `bun run lint`
  - Manual smoke: start/stop/cancel from UI; confirm `recording:status` + `transcript:complete` arrive and history updates exactly once (no dedupe failures).

### Phase 2 — Tray + Overlay + Audio Cues (M/L)
Depends on: Phase 0–1.

- **P2.1 Tray enhancements (M)**
  - Dynamic tray menu builder (`src-tauri/src/tray.rs` or new `tray_menu.rs`) with: enable toggle, mode, language, mic device, start/stop, recent transcripts, overlay toggle.
  - Rebuild triggers on config/history/device changes; add Rust unit tests.
  - **File touchpoints (brownfield mapping)**
    - `src-tauri/src/tray.rs`, `src-tauri/src/integration.rs`, `src-tauri/src/config.rs`, `src-tauri/src/history.rs`
    - New (optional): `src-tauri/src/tray_menu.rs`
    - Tests: add/extend `#[cfg(test)]` modules in tray code (or `tests/` if used)

- **P2.2 Audio cues (M)**
  - Implement real audio playback (`src-tauri/src/audio_cue.rs`, `rodio`), use existing `src-tauri/sounds/*.wav` and add cancel cue.
  - Wire into start/stop/cancel/error with timing to reduce beep capture; respects `audio.audio_cues_enabled`.
  - **File touchpoints (brownfield mapping)**
    - New: `src-tauri/src/audio_cue.rs`
    - Wiring: `src-tauri/src/integration.rs`, `src-tauri/src/config.rs`, `src-tauri/src/recording.rs`
    - Assets: `src-tauri/sounds/*`

- **P2.3 Overlay (M/L)**
  - Add overlay window config to `src-tauri/tauri.conf.json`; implement `src-tauri/src/overlay.rs` show/hide/position/click-through gated by `ui.overlay_enabled`.
  - Add Vite multi-page build (`vite.config.ts`) + `overlay.html` + `src/overlay/*` UI (pill, timer, waveform).
  - Minimal CPU: throttled meter, multi-monitor positioning, clear state.  
  - (Added specificity) Define throttle targets for testability:
    - overlay meter updates ≤ 15 Hz when visible, 0 Hz when hidden/disabled
    - overlay timer updates ≤ 2 Hz (text) unless actively recording
  - **File touchpoints (brownfield mapping)**
    - `src-tauri/tauri.conf.json`, new `src-tauri/src/overlay.rs`, `src-tauri/src/integration.rs`
    - `vite.config.ts`, new `overlay.html`, new `src/overlay/*`
- **Acceptance criteria**
  - Tray reflects current enabled/recording/model state within 250ms of changes.
  - Overlay can be disabled safely via config; when disabled, it has zero impact on idle CPU.
- **Verification**
  - `cargo test` (tray menu builder tests)
  - `bun run build` (overlay multi-page build included)
  - Manual smoke: tray start/stop mirrors UI; overlay shows correct session timer and stops updating when idle.

### Phase 3 — Audio Quality + VAD Auto-Stop (M)
Depends on: Phase 1.

- Sidecar preprocess: resample/trim/normalize (trim gated by `audio.trim_silence`).
  - **File touchpoints (brownfield mapping)**
    - `sidecar/src/openvoicy_sidecar/preprocess.py`
    - `sidecar/src/openvoicy_sidecar/recording.py`
- Optional VAD auto-stop (config-driven: `audio.vad_enabled`, `audio.vad_silence_ms`, `audio.vad_min_speech_ms`) and UI affordances.
  - **File touchpoints (brownfield mapping)**
    - `sidecar/src/openvoicy_sidecar/recording.py` (session loop / stop triggers)
    - new (optional): `sidecar/src/openvoicy_sidecar/vad.py` (if kept separate)
    - `shared/schema/AppConfig.schema.json`, `src-tauri/src/config.rs`, `src/types.ts` (already landed in P0.3)
    - Frontend settings surface: extend `src/components/Settings/SettingsPanel.tsx` (or introduce `src/components/Settings/AudioSettings.tsx`)
- Add tests for VAD edge cases (short utterances, background noise).  
  - **File touchpoints (brownfield mapping)**
    - `sidecar/tests/*` (synthetic audio fixtures or generated arrays; no large committed audio files)
- **Acceptance criteria**
  - With VAD disabled, recording behavior matches current baseline.
  - With VAD enabled, auto-stop triggers only after configured silence window and does not cut typical short utterances.
- **Verification**
  - `pytest sidecar/tests` (VAD-focused tests included)
  - `scripts/e2e/test-full-flow.sh` (sanity on end-to-end behavior)

### Phase 4 — Model Catalog + Optional Whisper Support (L)
Depends on: Phase 0 + schema updates.

- P4.1 Add `shared/model/MODEL_CATALOG.json` + per-model manifests with checksums.
  - (Added specificity) Add/extend schema validation and ensure catalog/manifests are packaged with both host and sidecar resources.
  - **File touchpoints (brownfield mapping)**
    - `shared/model/MODEL_CATALOG.json`, `shared/model/manifests/*`
    - `shared/schema/ModelCatalog.schema.json`, `shared/schema/ModelManifest.schema.json`
    - `python scripts/validate_model_manifest.py`
- P4.2 Implement `model.install` in sidecar + integrity verification (sha256 hash check).
  - **File touchpoints (brownfield mapping)**
    - `sidecar/src/openvoicy_sidecar/server.py` (RPC method exposure)
    - `sidecar/src/openvoicy_sidecar/model_cache.py` (download/verify/atomic install)
    - `sidecar/tests/*` (hash mismatch, partial download, cancel/retry)
- P4.3 Sidecar ASR backend dispatch by `family` (Parakeet vs Whisper); implement Whisper backend (`faster-whisper`) with optional `language` param.
  - **File touchpoints (brownfield mapping)**
    - `sidecar/src/openvoicy_sidecar/asr/base.py`
    - `sidecar/src/openvoicy_sidecar/asr/parakeet.py`
    - new: `sidecar/src/openvoicy_sidecar/asr/whisper.py` (or similar)
    - `sidecar/src/openvoicy_sidecar/asr/__init__.py`
- P4.4 IPC additive: `asr.initialize.language?`; host retries without language if unsupported.
  - **File touchpoints (brownfield mapping)**
    - `shared/ipc/IPC_PROTOCOL_V1.md` (additive doc)
    - `shared/contracts/sidecar.rpc.v1.json` (mirror)
    - `src-tauri/src/integration.rs`, `src-tauri/src/ipc/types.rs`
- P4.5 Update UI `src/components/Settings/ModelSettings.tsx` for model selection + language dropdown (Whisper only).  
  - **File touchpoints (brownfield mapping)**
    - `src/components/Settings/ModelSettings.tsx`, `src/store/*`, `src/types.ts`
- (Added completeness) Ensure progress observability matches the plan:
  - implement `event.model_progress` notification (optional per IPC) and forward to canonical `model:progress`, **or** ensure `model:status.progress` updates frequently enough during installs that the UI can show meaningful progress without polling.
  - **File touchpoints (brownfield mapping)**
    - `sidecar/src/openvoicy_sidecar/server.py`, `sidecar/src/openvoicy_sidecar/notifications.py` (if progress notifications are emitted)
    - `src-tauri/src/integration.rs` (forwarding)
    - `shared/contracts/*` and fixtures (progress payload schema)
- **Acceptance criteria**
  - Model install/update is atomic; corrupt partial downloads never produce “ready” state.
  - Host tolerates missing `language` support and downgrades gracefully.
- **Verification**
  - `python scripts/validate_model_manifest.py`
  - `pytest sidecar/tests` (hash verification + install state tests)
  - Manual smoke: select model, install, cancel/retry; verify `model:progress` and final `model:status`.

### Phase 5 — Optional Encrypted Persistent History + Export (M)
Depends on: Phase 1.

- Disk persistence behind explicit toggle (`history.persistence_mode`); encryption via OS keychain.
- Export to Markdown/CSV via `export_history` command; "purge history" controls.  
- **File touchpoints (brownfield mapping)**
  - Rust: `src-tauri/src/history.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/config.rs`
  - New (optional): `src-tauri/src/history_persistence.rs` (or similar) to isolate disk/encryption logic from the ring buffer
  - Frontend: `src/components/Settings/HistoryPanel.tsx` (or new history UI from Phase 1), `src/store/*`, `src/types.ts`
- **Acceptance criteria**
  - Default remains memory-only with no disk writes.
  - If enabled, export produces deterministic output and handles empty history.
- **Verification**
  - `cargo test` (history persistence/export unit tests)
  - `bun run test` (UI export flow tests)

### Phase 6 — Onboarding + Theme + Accessibility (M)
Depends on: schema updates in Phase 0.

- Onboarding wizard (`src/components/Onboarding/*`) gated by `ui.onboarding_completed` with migration-safe defaulting.
- Theme override (`ui.theme`) with Tailwind `darkMode:'class'`, `src/hooks/useTheme.ts`, and Settings toggle.
- Reduce motion (`ui.reduce_motion`); keyboard nav and ARIA improvements.  
- **File touchpoints (brownfield mapping)**
  - `src/components/Onboarding/*`, `src/App.tsx`, `src/components/Settings/SettingsPanel.tsx`
  - `tailwind.config.js`, `src/index.css`, new `src/hooks/useTheme.ts`
  - `src/types.ts`, `src/store/*`
- **Acceptance criteria**
  - First-run onboarding does not block power users (skip available).
  - Accessibility checks: tab order sane, key controls reachable, reduced motion respected.
- **Verification**
  - `bun run test`
  - `bun run lint`
  - Manual: fresh install path shows onboarding; migrated config path does not.

### Phase 7 — CI/Packaging Hardening (M/L)
Runs continuously; release gate.

- OS matrix green; sidecar packaging includes contracts/manifests/presets.
- Ensure overlay build works in CI builds.
- Deterministic build inputs (lockfiles) and security scanning.
- Stabilize tests across OS matrix in `.github/workflows/test.yml` and `.github/workflows/build.yml`.  
- **File touchpoints (brownfield mapping)**
  - `.github/workflows/test.yml`, `.github/workflows/build.yml`
  - `scripts/build-sidecar.sh`, `scripts/bundle-sidecar.sh`
  - `src-tauri/tauri.conf.json` (resources), `shared/contracts/*`, `shared/model/*`, `shared/replacements/*`
- **Acceptance criteria**
  - CI runs the same contract validators + unit tests as local (no “works locally only” gaps).
  - Packaged app can locate sidecar resources (presets/manifests/contracts) without dev-only paths.
- **Verification**
  - `bun run build && bun run test && bun run lint`
  - `cargo test` (in `src-tauri`)
  - Run the existing CI workflows locally where feasible (or validate by pushing to CI).

### Parallelization (5 agents)
- Agent A: Phase 0 plumbing (contracts, supervisor, session gating, P0.1–P0.5)
- Agent B: Phase 1 UI (tabs/dashboard/history/replacements) + Phase 6 UI
- Agent C: Tray + cues (Phase 2.1 + 2.2)
- Agent D: Overlay (Phase 2.3)
- Agent E: Models + Whisper + packaging (Phase 4 + Phase 7)  
- **Brownfield parallelization guard:** minimize merge conflicts by owning file surfaces:
  - `src-tauri/src/integration.rs` + IPC bridging: single owner at a time.
  - Frontend event hooks/types (`src/hooks/useTauriEvents.ts`, `src/types.ts`): single owner at a time.
  - (Added specificity) `src-tauri/src/commands.rs` and `shared/ipc/IPC_PROTOCOL_V1.md` / `shared/contracts/*` should also have a single owner per PR to avoid contract drift during Phase 0.
  - (Added specificity) Sidecar RPC surface in `sidecar/src/openvoicy_sidecar/server.py` should have a single owner per PR while Phase 0 is landing.

---

## 9. Testing Strategy

### Contract tests (new cornerstone)
- Validate generated types match schemas.
- Golden JSONL fixtures for:
  - sidecar notifications
  - JSON-RPC requests/responses
  - Tauri event payloads
- Extends existing `scripts/validate_ipc_examples.py`.
- **Brownfield requirement:** include fixtures for both canonical and legacy event aliases during the compatibility window.
- **Regression focus (explicit):** include fixtures that cover the known drift points called out in §1 (transcript event names, `StateEvent.detail` vs TS, `model:status` payload shape).
- **Drift-prevention requirement (tightened):** validators should avoid hard-coded allowlists that reintroduce drift; prefer deriving allowed method/event names from the contract JSON (or from a single generated list) so adding a method requires updating contracts + fixtures in one place.
- (Added specificity) Contract validation must fail the build if:
  - frontend listens to an event name not present in `shared/contracts/tauri.events.v1.json` (canonical or declared alias),
  - Rust emits an event payload that does not validate against the corresponding schema,
  - sidecar handlers are missing required methods (`status.get` at minimum) or fixture examples include unknown method names.

### Frontend
- Vitest + Testing Library (existing framework in `vitest.config.ts`):
  - State/recording badges, history/search/export, replacements parity, onboarding, theme/accessibility toggles, overlay throttling logic.
  - Extends existing tests in `src/tests/`.
  - Extend `src/hooks/useTauriEvents.test.ts` to assert canonical+legacy subscription behavior and `seq`-based dedupe.

### Rust
- `cargo test` in `src-tauri`:
  - Supervisor restart policy, stale-event dropping, tray builder snapshots, config migration defaults.
  - Extends existing tests in `state.rs`, `history.rs`, `config.rs`.

### Sidecar
- `pytest sidecar/tests`:
  - `status.get`, preset loading, replacements preview/apply, VAD behavior (synthetic audio), model install hash verification.
  - Regression tests per `IPC_PROTOCOL_V1.md` §Test Requirements.

### E2E
- `scripts/e2e/run-all.sh` plus targeted OS smoke checks:
  - Sidecar crash loop recovery
  - Device removal mid-recording
  - Offline install behavior (existing model usable)

### Test data
- No large audio/model artifacts committed.
- Use generated audio for meter tests and mock transcription for unit tests.
- Optional local-only whisper smoke fixture.

---

## 10. Comparison & Trade-offs

### Why this approach
- Contracts-as-code adds upfront work but pays off by eliminating recurring drift bugs.
- Additive-only protocol/schema evolution preserves upgrade safety.
- Event-driven updates reduce polling complexity and keep UI responsive.
- Catalog + per-model manifests scale to future models without breaking the default manifest tooling.

### Trade-offs
- Contracts-as-code adds upfront work; worth it for eliminating drift.
- VAD improves UX for many but must be opt-in to avoid surprise cutoffs.
- Encrypted history persistence adds complexity; default remains memory-only for privacy.
- Whisper support increases package size; treated as optional capability with clear UX.
- Overlay click-through/always-on-top is inherently OS-fragile; mitigated via `ui.overlay_enabled` and graceful fallback.
- Audio cues may still be picked up acoustically by microphones; delaying start reduces risk but cannot eliminate it.
- In-memory history resets on restart (privacy-first); persistence is explicit opt-in.

---

## Appendix A: Brownfield Compatibility Notes

This plan was developed with full awareness of the existing codebase:

| Existing Module | Plan Impact | Notes |
|---|---|---|
| `src-tauri/src/state.rs` | No semantic changes | `AppState` enum untouched; may add metadata to `StateEvent` |
| `src-tauri/src/config.rs` | Additive fields only | New optional fields with defaults; `validate_and_clamp` extended |
| `src-tauri/src/history.rs` | Extended `TranscriptEntry` | New optional fields; ring buffer max_size becomes configurable |
| `src-tauri/src/integration.rs` | Orchestrator role preserved | Session gating + supervisor wiring added |
| `src-tauri/src/commands.rs` | Remove TODOs, add new commands | Existing signatures stable; new commands additive |
| `src-tauri/src/watchdog.rs` | Evolved into supervisor | Same crate; enhanced with circuit breaker |
| `src-tauri/src/injection.rs` | Minor: app_overrides support | Existing flow preserved |
| `src-tauri/src/tray.rs` | Dynamic menu builder | Extends existing tray |
| `src/hooks/useTauriEvents.ts` | Listen to `state:changed` + legacy | Current `state_changed` preserved as alias |
| `src/types.ts` | Extended with new types | Existing types stable |
| `shared/ipc/IPC_PROTOCOL_V1.md` | Additive only | LOCKED v1.0; new optional params |
| `shared/schema/AppConfig.schema.json` | Additive fields only | `additionalProperties: false` requires explicit additions |
| `sidecar/` | Bug fixes + new methods | `status.get` impl; `model.install` new |
| (New) `src-tauri/src/supervisor.rs` | New module only | Supervisor layer; no rewrite of `integration.rs`/`watchdog.rs` semantics |
| (New) `src-tauri/src/overlay.rs` | New module only | Overlay window management; gated by config; can be disabled |
| (New) `src-tauri/src/audio_cue.rs` | New module only | Audio cues; respects existing `audio.audio_cues_enabled` |