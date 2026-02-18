# Voice Input Tool — Usability & Polish Release (PLAN v1)
_Base commit: `6a260e5` (main) • Date: 2026-02-13 • Guardrails: IPC v1 additive-only, `AppState` non-breaking, `schema_version: 1` additive, Rust host/ Python sidecar boundary, UUID v4 session correlation._

---

## 1) Executive Summary (TL;DR)
- Deliver a polished, professional cross-platform desktop UX (Windows + macOS) via a tabbed UI, richer status dashboard, improved history/replacements UX, and first-run onboarding.
- Add high-signal feedback loops: recording overlay window + tray state icon/menu + audio cues.
- Introduce multilingual transcription via Whisper (sidecar), while keeping IPC protocol V1 backward-compatible (additive optional params only).
- Add theme support (system/light/dark) and expand CI to build/test on Windows and macOS.
- Organize work into parallelizable streams with a single “contracts” stream owning shared schema/types/events to minimize merge conflicts.

---

## 2) Architecture Overview
### Runtime components
- **Rust host (Tauri)**: orchestration, UI windows, tray, injection, config persistence, IPC client to sidecar, emits frontend events.
  - Key state machine: `AppState` enum in Rust (must keep existing variants: `Idle | LoadingModel | Recording | Transcribing | Error`).
- **Frontend (React + Zustand)**: settings/UI, state display, history/replacements, onboarding, theme, overlay UI.
- **Python sidecar**: audio capture + ASR engine + model cache/download; owns ASR implementation boundary.

### Contracts (must remain compatible)
- **IPC**: `shared/ipc/IPC_PROTOCOL_V1.md` is locked; changes must be additive (new methods or new optional params).
- **Config**: `shared/schema/AppConfig.schema.json` with `schema_version: 1` must remain backward-compatible; new fields require defaults; existing semantics unchanged.
- **Session IDs**: UUID v4 session IDs flow end-to-end and must be preserved/propagated into history, tray “recent”, status preview, overlay correlation.

### Proposed internal “event contract” (Tauri → frontend)
- Standardize event payloads so multiple UI surfaces can subscribe consistently:
  - `app:state_changed` → `{ state: AppState, session_id?: string, at_ms: number }`
  - `transcription:created` → `{ session_id: string, text: string, confidence?: number, duration_ms?: number, started_at_ms?: number, ended_at_ms?: number }`
  - `audio:level` → `{ session_id: string, rms: number, peak: number, at_ms: number }` (only when recording)
  - `sidecar:health` → `{ ok: boolean, last_seen_ms: number, details?: string }`
  - (All are additive; do not remove/rename existing events—only introduce aliases if needed.)

---

## 3) Phase Breakdown (numbered tasks, dependencies, acceptance criteria)

### Phase 0 — Shared Contracts & Scaffolding (do first to enable parallel work)
**Goal:** Prevent merge conflicts by centralizing edits to config schema/types/events/IPC addenda.  
**Owner stream:** “Contracts” agent (single owner).  
**Risk:** Medium (touches shared surfaces).  

#### 0.1 Beads issue breakdown + workstream ownership (Complexity: S)
- **Files:** `.beads/*` (issues only), no product code changes.
- **Implementation:**
  - Create beads issues per phase + per major task cluster (UI, overlay/tray, audio cues, Whisper, onboarding/theme, CI).
  - For each issue, define “file ownership boundaries” (below).
- **Acceptance criteria:**
  - Each major task has a `bd-###` with clear scope, dependencies, and a designated owner stream.
  - No two issues claim overlapping “owned” files without an explicit coordination note.

#### 0.2 Config schema additions (aggregate all new config fields once) (Complexity: M)
- **Files:**
  - `shared/schema/AppConfig.schema.json`
  - `src-tauri/src/config.rs` (load/migrate defaults)
  - `src/types.ts` (or wherever `AppConfig` TS type lives)
- **New fields (additive, defaults required):**
  - `config.model.family: "parakeet" | "whisper"` (default `"parakeet"`)
  - `config.model.language: string | null` (default `null`)
  - `config.ui.theme: "system" | "light" | "dark"` (default `"system"`)
  - `config.ui.onboarding_completed: boolean` (default **`true` for migrated existing configs**, **`false` for brand-new config creation**; implement via “is_new_config” detection in Rust)
  - `config.audio.audio_cues_enabled: boolean` (default `true`)
- **Edge cases / failure modes:**
  - Old configs missing nested objects (e.g., `ui` absent) must load with defaults without panic.
  - Invalid enum values should fall back safely (log warning + default).
- **Acceptance criteria:**
  - Loading an old config (schema v1) produces a fully-populated config struct with defaults and no behavior regressions.
  - Schema validation (where used) passes with new fields optional/defaulted.

#### 0.3 IPC protocol additive extension spec (language param) (Complexity: S)
- **Files:** `shared/ipc/IPC_PROTOCOL_V1.md`
- **Implementation:**
  - Add optional param to `asr.initialize`: `language?: string | null` (default behavior unchanged if omitted).
  - Document semantics:
    - `null`/omitted = auto for Whisper, English/default for Parakeet.
    - If `family=parakeet`, ignore non-null `language` (or return soft warning in response payload if protocol supports).
- **Acceptance criteria:**
  - No breaking changes: existing clients sending the old shape remain valid.
  - Sidecar can accept old initialize payload unchanged.

#### 0.4 Standardize frontend-facing events + shared types (Complexity: M)
- **Files:**
  - `src-tauri/src/lib.rs` (or central emitter module)
  - New `src-tauri/src/events.rs` (recommended) with helper fns:
    - `emit_state_changed(app: &AppHandle, snapshot: AppStateSnapshot)`
    - `emit_transcription_created(app: &AppHandle, entry: TranscriptionEvent)`
    - `emit_audio_level(app: &AppHandle, level: AudioLevelEvent)`
  - `src/types.ts` (or `src/types/events.ts`) for event payload TS types.
- **Acceptance criteria:**
  - Main window and overlay can both subscribe to the same stable event names.
  - Payloads include `session_id` wherever relevant.

**Parallelization note:** Phases 1/2/3/5/6 can proceed in parallel after Phase 0.2 and 0.4 land (or at least after their interfaces are agreed).

---

### Phase 1 — UI/UX Overhaul: Tab Layout & Polish
**Owner stream:** “Frontend-UI” agent.  
**Dependencies:** Phase 0.4 (event types), Phase 0.2 (config types for theme/onboarding later).  
**Risk:** Low.  

#### 1.1 Tab navigation component + routing (Complexity: M)
- **Files (new):**
  - `src/components/Layout/TabBar.tsx`
  - `src/components/Layout/TabPanel.tsx`
- **Files (modify):**
  - `src/App.tsx` (replace monolithic panel with tab layout)
  - `src/store/appStore.ts` (add ephemeral UI slice: `activeTab`)
- **Data structures:**
  - `type AppTab = "status" | "settings" | "history" | "replacements"`
  - Zustand slice: `ui: { activeTab: AppTab }` + `setActiveTab(tab: AppTab)`
- **UX requirements:**
  - Horizontal tab bar with inline SVG icons, active indicator animation.
  - Keyboard nav: Left/Right arrows switch tabs; Enter activates focused tab.
  - Persist active tab in Zustand only (not config).
- **Acceptance criteria:**
  - Tabs render and switch reliably via click and keyboard.
  - Active tab persists during runtime navigation but resets on app restart.

#### 1.2 Status dashboard tab (Complexity: M)
- **Files (new):** `src/components/Status/StatusDashboard.tsx`
- **Files (modify):** `src/components/StatusIndicator.tsx`
- **Implementation details:**
  - Subscribe to `app:state_changed`, `transcription:created`, `sidecar:health`, model download/status events (existing or add minimal new).
  - Display:
    - Prominent animated state indicator for each `AppState`.
    - Mode badge (Push-to-Talk vs Toggle) derived from config.
    - Hotkey hint string (format consistently across OS).
    - Last transcription card: text, timestamp, confidence (show “—” if absent).
    - Quick stats computed client-side from history: `count_today`, `audio_time_today_ms`.
    - Model status badge (ready/downloading/not-downloaded) from existing model manager events/state.
- **Edge cases:**
  - No history → empty state (no “NaN”/undefined).
  - Confidence missing → display neutral badge.
  - Sidecar unhealthy → actionable text (“restart sidecar”, “open diagnostics” if exists).
- **Acceptance criteria:**
  - For each `AppState`, dashboard shows correct indicator and does not throw.
  - Stats match history entries filtered by local day boundary.

#### 1.3 Settings tab reorganization (Complexity: M)
- **Files (modify):** `src/components/Settings/SettingsPanel.tsx`
- **Implementation:**
  - Collapsible sections: Audio, Hotkeys, Injection, Model, UI (with icons).
  - Prominent activation mode radio group with descriptions.
- **Acceptance criteria:**
  - Sections expand/collapse; state persists for session (optional) without breaking layout.
  - Mode switch updates config/store and reflects in Status tab immediately.

#### 1.4 History tab enhancement + file move (Complexity: M)
- **Files:**
  - Move `src/components/Settings/HistoryPanel.tsx` → `src/components/History/HistoryPanel.tsx`
  - Update imports in `src/App.tsx` and any settings references.
- **Features:**
  - Search filter (client-side) on text.
  - Entry cards: text, timestamp, duration, confidence badge, copy button.
  - Clear-all with confirmation dialog.
  - Empty state illustration/message.
- **Acceptance criteria:**
  - Search is case-insensitive and stable with large history (no O(n²) rendering).
  - Copy uses Tauri clipboard API reliably; shows transient “Copied” state.
  - Clear-all requires explicit confirm; cancel leaves history unchanged.

#### 1.5 Replacements tab polish + regex validation feedback (Complexity: S)
- **Files (modify):** existing `src/components/Replacements/*`
- **Implementation:**
  - Inline validation for regex patterns with `try { new RegExp(pattern) } catch`.
  - Preset cards with description and “Apply”.
  - Tab badge with rule count.
- **Acceptance criteria:**
  - Invalid regex cannot be saved (or saves but clearly disabled—pick one consistent behavior).
  - Existing replacements tests remain passing; add regression tests for validation.

**Tests (Phase 1)**
- **Files (new/modify):**
  - `src/components/Layout/TabBar.test.tsx`
  - `src/components/History/HistoryPanel.test.tsx`
  - Extend existing replacements tests under `src/components/Replacements/*.test.tsx`
- **Acceptance criteria:**
  - `npm run test` passes; tests cover tab keyboard nav, history search/copy/clear, replacements regex validation.

---

### Phase 2 — Recording Overlay & Tray Indicator
**Owner streams:** “Tauri-Windows” agent (Rust window/tray), “Frontend-Overlay” agent (overlay React).  
**Dependencies:** Phase 0.4 (events), some existing recording state transitions.  
**Risk:** Medium (platform quirks).  

#### 2.1 Tauri overlay window creation + state-driven show/hide (Complexity: L)
- **Files (modify):**
  - `src-tauri/tauri.conf.json` (add window config for `recording-overlay`)
  - `src-tauri/src/lib.rs` (create/manage window at startup or lazy-create)
- **Implementation details:**
  - Window label: `"recording-overlay"`.
  - Properties: transparent, undecorated, always-on-top, hidden by default, not focusable.
  - Positioning:
    - Add helper `fn position_overlay(window: &Window, target_monitor: MonitorHandle)` (top-center, respects scale factor).
    - Choose monitor: primary monitor or monitor of main window.
  - Show/hide:
    - Subscribe to app state changes (`AppStateManager` listener).
    - Show only in `Recording`; hide in all others.
- **Platform edge cases / failure modes:**
  - macOS: always-on-top + transparent window may require special level; ensure it doesn’t steal focus.
  - Windows: click-through requires `set_ignore_cursor_events(true)` (or platform-specific extension); verify it doesn’t break drag/resize (overlay should not be draggable).
  - Multi-monitor DPI scaling; negative coordinates; fullscreen apps.
- **Acceptance criteria:**
  - Overlay appears within 200ms of entering `Recording`, disappears on exit.
  - Overlay never receives focus; does not interfere with typing/clicking other apps.

#### 2.2 Overlay React entrypoint + UI (Complexity: M)
- **Files (new):**
  - `src/overlay/main.tsx`
  - `src/overlay/RecordingOverlay.tsx`
  - `src/overlay/Waveform.tsx`
  - `index-overlay.html` (or `overlay.html`, consistent with repo conventions)
- **Files (modify):** `vite.config.ts` (add overlay entry build)
- **Implementation details:**
  - UI pill: recording dot + “Recording” + elapsed timer + waveform canvas.
  - Timer: derived from `app:state_changed` `at_ms` and updated via `requestAnimationFrame` or 100ms interval (avoid drift).
  - Waveform: ring buffer of last ~50 `audio:level` samples; render RMS/peak.
- **Edge cases:**
  - No `audio:level` events (permissions denied) → waveform shows idle baseline instead of crashing.
  - If overlay loads late, it should request current state snapshot (optional) or wait for next event.
- **Acceptance criteria:**
  - Overlay bundle builds independently (no imports from main-only modules that increase size).
  - Waveform renders smoothly without >5% CPU usage on idle.

#### 2.3 Tray state icon variants + mapping logic (Complexity: M)
- **Files (modify):** `src-tauri/src/tray.rs`
- **Implementation:**
  - Add icon variants for `idle`, `recording`, `transcribing`, `disabled`.
  - Add pure mapping fn (unit-testable):
    - `fn tray_variant(state: AppState, enabled: bool) -> TrayIconVariant`
  - Update tooltip: `"Voice Input — Idle"`, `"Voice Input — Recording…"`, etc.
- **Acceptance criteria:**
  - Tray icon + tooltip update on every state change and enable/disable toggle.
  - Unit tests cover mapping for each `AppState` + enabled flag.

#### 2.4 Tray context menu: dynamic items + actions (Complexity: L)
- **Files (modify/new):**
  - `src-tauri/src/tray.rs`
  - Optional new `src-tauri/src/tray_menu.rs` to isolate menu building:
    - `fn build_tray_menu(ctx: &TrayMenuContext) -> Menu`
  - `src-tauri/src/config.rs` (if tray actions mutate config)
- **Data structure:**
  - `struct TrayMenuContext { enabled: bool, mode: ActivationMode, recent: Vec<RecentItem>, devices: Vec<DeviceInfo>, active_device_id: Option<String> }`
- **Menu requirements:**
  - Enable/Disable toggle (checkmark).
  - Mode submenu (radio items).
  - Recent transcriptions (last 5, truncated; click copies to clipboard).
  - Microphone submenu (device list; select active).
  - Open Settings, About (version dialog), Quit.
- **Rebuild triggers:**
  - New transcription, device list changes, mode change, enabled toggle.
  - Throttle rebuild (e.g., debounce 100ms) to avoid churn.
- **Edge cases:**
  - macOS menu item limits: cap recent items, cap devices, add “More…” if needed.
  - Clipboard failures: show notification/toast if available, or log.
- **Acceptance criteria:**
  - All menu actions work without opening the main window (except Open Settings).
  - Recent transcription copy works for Unicode/large text (truncate display only; copy full text).

**Tests (Phase 2)**
- Rust unit tests for `tray_variant()` and `build_tray_menu()` structure (IDs, checked/radio correctness).
- Optional “smoke” e2e script under `artifacts/e2e/` (if existing harness) to assert overlay window exists and is hidden by default.

---

### Phase 3 — Audio Feedback (start/stop/cancel/error cues)
**Owner stream:** “Tauri-Audio” agent.  
**Dependencies:** Phase 0.2 (config flag), existing recording/controller transitions.  
**Risk:** Low.  

#### 3.1 Add audio cue assets (Complexity: S)
- **Files (new):** `src-tauri/assets/sounds/{start,stop,cancel,error}.ogg`
- **Acceptance criteria:**
  - Each file < 100KB, short (<300ms), non-jarring.
  - Licensing/source documented in `docs/` or `THIRD_PARTY_NOTICES.md` addendum if required by repo policy.

#### 3.2 Implement `AudioCuePlayer` (Complexity: M)
- **Files (new):**
  - `src-tauri/src/audio_cue.rs`
- **Files (modify):**
  - `src-tauri/Cargo.toml` (add `rodio`)
  - `src-tauri/src/lib.rs` (initialize singleton, store in app state)
- **Implementation details:**
  - `enum AudioCue { Start, Stop, Cancel, Error }`
  - `struct AudioCuePlayer { tx: Sender<AudioCue> }` spawning a background thread holding `rodio::OutputStream` and playing decoded buffers.
  - `fn play(&self, cue: AudioCue)` is non-blocking.
  - Respect `config.audio.audio_cues_enabled`.
- **Failure modes:**
  - No default output device → no panic; log and continue silently.
  - Decoder error → log and continue.
- **Acceptance criteria:**
  - Playing cues never blocks recording/transcribing pipeline.
  - Disabling audio cues prevents any playback.

#### 3.3 Wire cues into state transitions (Complexity: M)
- **Files (modify):**
  - `src-tauri/src/recording.rs`
  - `src-tauri/src/integration.rs` (or whichever layer orchestrates transitions/errors)
- **Rules:**
  - Play `Start` immediately **before** recording begins.
  - Play `Stop` when recording ends and transcribing begins.
  - Play `Cancel` on explicit cancel action.
  - Play `Error` on user-visible error emission.
- **Acceptance criteria:**
  - Integration test or unit test verifies “transition → cue” mapping.
  - Cue playback does not change `AppState` semantics.

---

### Phase 4 — Language Selection & Whisper Support (Multilingual)
**Owner streams:** “Sidecar-ASR” agent (Python), “Frontend-ModelUI” agent (React), “Contracts” agent (already owns schema/IPC).  
**Dependencies:** Phase 0.2 + 0.3 (config + IPC), model manifest updates, sidecar model cache.  
**Risk:** High (packaging, performance, model downloads).  

#### 4.1 Expand model manifest schema + entries (Complexity: M)
- **Files (modify):** `shared/model/MODEL_MANIFEST.json`
- **Add fields (additive):**
  - `family`, `languages`, `size_mb`, `speed`, `quality`, `description`, `hf_repo`, `hf_revision?`, `hf_filename?`
- **Add models:**
  - `openai/whisper-base` (or map to a faster-whisper repo under the hood)
  - `openai/whisper-small`, `openai/whisper-medium` (optional gated by download size)
  - Keep existing Parakeet entry with `family: "parakeet", languages: ["en"]`
- **Acceptance criteria:**
  - Existing manifest consumers do not break (unknown fields ignored).
  - UI can render cards using new fields without hardcoding.

#### 4.2 Sidecar: engine dispatch + whisper implementation (Complexity: L)
- **Files (modify/add):**
  - `sidecar/asr/` (add `whisper_engine.py`, `factory.py`)
  - `sidecar/model_cache.py` (support multiple repos/families)
  - `sidecar/pyproject.toml` (add dependency: prefer `faster-whisper` + required libs)
- **Implementation details:**
  - Add engine selection based on `model_id` → manifest lookup yields `family`.
  - New optional initialize param: `language`:
    - Whisper: `None`/`"auto"` = auto-detect; else force language code.
    - Parakeet: ignore unless `"en"`/None; return structured warning if protocol supports.
  - Maintain session correlation: all results carry `session_id`.
- **Packaging considerations / failure modes:**
  - Dependency installation failures on Windows/macOS: provide clear error surfaced to host (e.g., “Whisper not available in this build”).
  - Model download failures: resumable or at minimum clear error + retry.
  - CPU-only performance: default to Whisper Base; warn for Medium if too slow.
- **Acceptance criteria:**
  - Sidecar can transcribe at least one non-English sample (fixture) with Whisper when configured.
  - Parakeet path remains unchanged and still works for English.

#### 4.3 Host ↔ sidecar IPC wiring (initialize with language) (Complexity: M)
- **Files (modify):**
  - Rust IPC client module (where `asr.initialize` is invoked; likely under `src-tauri/src/integration.rs`)
- **Implementation:**
  - When loading model, send:
    - `model_id` (existing)
    - `language` (new optional) from config only if family is whisper; otherwise omit.
  - Backward compatibility: if sidecar reports “unknown field” (older sidecar), retry without `language`.
- **Acceptance criteria:**
  - Host works with old sidecar binaries (language ignored) without crashing.
  - New sidecar uses `language` successfully.

#### 4.4 Frontend: model family + language UI (Complexity: M)
- **Files (modify):** `src/components/Settings/ModelSettings.tsx`
- **Implementation:**
  - Family cards (Parakeet/Whisper) with size/quality info from manifest.
  - Language dropdown visible only for Whisper; includes `"Auto"` plus supported codes.
  - Download + “currently loaded” status indicators.
- **Edge cases:**
  - Language selected but Whisper not installed/available → show actionable error.
  - Switching family while recording → either block with message or defer change until idle.
- **Acceptance criteria:**
  - Selecting Whisper + language triggers reinitialize flow and updates Status tab.
  - Language dropdown state persists via config.

**Tests (Phase 4)**
- Sidecar: `sidecar/tests/test_asr_initialize_language.py` (or equivalent) verifying initialize accepts/ignores language correctly.
- Frontend: test dropdown visibility + config update logic.
- Host: unit/integration test for “retry without language on older sidecar” (mock IPC).

---

### Phase 5 — First-Run Onboarding
**Owner stream:** “Frontend-Onboarding” agent.  
**Dependencies:** Phase 0.2 (onboarding flag), Phase 4 download/status events if onboarding triggers model download.  
**Risk:** Low.  

#### 5.1 Onboarding wizard UI (Complexity: M)
- **Files (new):**
  - `src/components/Onboarding/OnboardingWizard.tsx`
  - `src/components/Onboarding/steps/WelcomeStep.tsx`
  - `src/components/Onboarding/steps/MicrophoneStep.tsx`
  - `src/components/Onboarding/steps/ModelStep.tsx`
  - `src/components/Onboarding/steps/HotkeyStep.tsx`
- **Implementation:**
  - 4-step wizard with Skip, progress dots, and final “ready” screen.
  - Microphone step reuses existing mic test component (do not duplicate audio logic).
  - Model step triggers download and shows progress (subscribe to model events).
  - Completion sets `config.ui.onboarding_completed = true`.
- **Edge cases:**
  - User skips model download → warn that recording won’t work until model is installed.
  - Permissions denied for mic test → provide guidance and allow continue/skip.
- **Acceptance criteria:**
  - Brand-new config shows onboarding; existing users do not get forced into onboarding after upgrade.
  - Completing onboarding routes to main tabs.

#### 5.2 App launch trigger + reset option (Complexity: S)
- **Files (modify):**
  - `src/App.tsx` (gate main UI on onboarding flag)
  - `src/components/Settings/SettingsPanel.tsx` (add “Reset onboarding” button)
- **Acceptance criteria:**
  - Reset sets flag to false and immediately shows wizard on next render/restart (choose one, document behavior).

**Tests (Phase 5)**
- Wizard step navigation; completion flag set; reset works.

---

### Phase 6 — Dark/Light Theme
**Owner stream:** “Frontend-Theme” agent.  
**Dependencies:** Phase 0.2 (theme config).  
**Risk:** Low.  

#### 6.1 Theme infrastructure + resolved theme state (Complexity: M)
- **Files (modify):**
  - `tailwind.config.js` (`darkMode: "class"`)
  - `src/index.css` (base background/text vars if needed)
  - `src/store/appStore.ts` (resolved theme + setter)
- **Implementation:**
  - Persist preference in config: `config.ui.theme`.
  - Resolve to `light|dark` using `matchMedia` when `system`.
  - Apply `dark` class to `<html>` on change.
- **Acceptance criteria:**
  - Theme updates live without reload and persists across restart.
  - System theme changes propagate when preference is `system`.

#### 6.2 Apply dark variants across components (Complexity: M)
- **Files (modify):** components touched in Phase 1 + common UI primitives.
- **Acceptance criteria:**
  - No illegible text in dark mode; focus/hover states visible.
  - Overlay remains readable and consistent.

#### 6.3 Settings toggle UI (Complexity: S)
- **Files (modify):** settings UI (where UI section lives)
- **Acceptance criteria:**
  - 3-way toggle (System/Light/Dark) writes config and updates immediately.

**Tests (Phase 6)**
- Unit test for theme resolver logic (system vs override); minimal snapshot/DOM tests for class toggling.

---

### Phase 7 — CI/CD for Windows & macOS
**Owner stream:** “CI” agent.  
**Dependencies:** Stabilized tests/build commands; sidecar dependencies strategy.  
**Risk:** Medium.  

#### 7.1 Expand CI matrix for tests + builds (Complexity: M)
- **Files (modify):**
  - `.github/workflows/test.yml`
  - `.github/workflows/build.yml`
- **Implementation:**
  - Test matrix: `ubuntu-latest`, `macos-latest`, `windows-latest` for `npm test` + Rust tests.
  - Build matrix: `macos-latest`, `windows-latest` for Tauri build artifacts.
  - Caching: Cargo + node modules keyed by lockfiles.
- **Edge cases:**
  - Windows path length issues; ensure checkout uses short paths if needed.
  - macOS signing not required for CI build (only compile).
- **Acceptance criteria:**
  - CI completes successfully on all OS targets with deterministic caching.

#### 7.2 Platform-specific test fixes + guards (Complexity: L)
- **Files:** varies based on failures; likely Rust path handling and tests.
- **Implementation:**
  - Fix separators/permissions; use `std::path::Path` robustly.
  - Add `#[cfg(target_os = "...")]` only when unavoidable; prefer portable code.
  - Where sidecar tests are too heavy on Windows/macOS, split:
    - Always-run unit tests (pure Python) across platforms.
    - Optional Whisper integration tests on `ubuntu-latest` only (documented).
- **Acceptance criteria:**
  - No skipped tests without justification and documentation in workflow comments.

---

## 4) Critical Path & Dependencies
### Critical path (minimum sequence to ship “polish release”)
1. **Phase 0.2 + 0.4** (config + event contracts) → unblocks most UI surfaces and overlay wiring.
2. **Phase 2.1 (overlay window)** + **Phase 2.3 (tray icon mapping)** → core “recording feedback” loop.
3. **Phase 3.2/3.3 (audio cues + wiring)** → completes multi-modal feedback.
4. **Phase 7.1/7.2 (CI matrix + fixes)** → required for cross-platform confidence.
5. **Phase 4 (Whisper)** is critical *if multilingual is in scope for this release*; otherwise treat as a feature flag / separate milestone.

### Dependency map (high level)
- Phase 1 depends on Phase 0 events/types (for Status dashboard).
- Phase 2 depends on Phase 0 events + stable state transitions in Rust.
- Phase 3 depends on config flag + recording transition hooks.
- Phase 4 depends on config + IPC additive spec + model manifest.
- Phase 5 depends on onboarding flag + model download/progress surfaces.
- Phase 6 depends on theme config field.
- Phase 7 depends on tests/build scripts stabilized by all phases.

### Parallel execution design (suggested workstreams)
- **Contracts stream:** Phase 0 (owns `shared/schema/*`, `shared/ipc/*`, shared TS types, event names)
- **Frontend-UI stream:** Phase 1 (owns `src/components/Layout/*`, `src/components/Status/*`, history/replacements UI)
- **Tauri-Windows/Tray stream:** Phase 2.1/2.3/2.4 (owns `src-tauri/src/tray*.rs`, overlay window creation in `src-tauri/src/lib.rs`)
- **Frontend-Overlay stream:** Phase 2.2 (owns `src/overlay/*`, `vite.config.ts` overlay entry)
- **Tauri-Audio stream:** Phase 3 (owns `src-tauri/src/audio_cue.rs`, integration hooks)
- **Sidecar-ASR stream:** Phase 4.2 (owns `sidecar/**` ASR changes)
- **Frontend-ModelUI stream:** Phase 4.4 (owns `src/components/Settings/ModelSettings.tsx`)
- **Frontend-Onboarding/Theme streams:** Phases 5 & 6
- **CI stream:** Phase 7 (owns `.github/workflows/*`)

---

## 5) Risk Mitigation
- **IPC compatibility:** implement “send language if supported, retry without” in host; keep sidecar permissive with unknown fields; update `IPC_PROTOCOL_V1.md` additively only.
- **Config migration safety:** treat missing fields as defaults; distinguish “existing user” vs “new install” so onboarding doesn’t surprise current users.
- **Overlay platform quirks:** isolate platform-specific window calls behind `cfg` blocks in a dedicated module (e.g., `src-tauri/src/overlay_window.rs`) and add a manual test checklist item per OS (multi-monitor, fullscreen, focus).
- **Audio cue leakage into mic:** play cues before recording start; add setting to disable cues; document limitation (speakers can still be picked up by mic).
- **Whisper dependency weight:** prefer CPU-friendly, wheel-available dependency (e.g., faster-whisper); gate Medium model behind “advanced” UI; provide clear error when dependencies unavailable in packaged sidecar.
- **CI flakiness:** avoid timing-dependent UI tests; use deterministic event mocks; for platform-specific tests, isolate and document rationale rather than broad skipping.
- **Regression testing culture:** any bug found during integration (tray/menu rebuild, overlay visibility, config load) must ship with a targeted regression test in the closest existing test suite (Vitest for frontend, `cargo test` for Rust, pytest for sidecar).