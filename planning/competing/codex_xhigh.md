**Executive Summary (TL;DR)**  
Deliver a polished cross‑platform Voice Input Tool by restructuring the UI into tabs, adding a recording overlay and richer tray behavior, introducing audio cues, and enabling multilingual Whisper support with robust config migration. The plan preserves IPC Protocol V1 (additive only), keeps the existing `AppState` enum intact, and adds only backward‑compatible schema fields. Work is split into parallel front‑end, Rust host, sidecar, and CI streams with explicit dependencies and acceptance criteria. New features include onboarding, theme support, and expanded CI matrices for Windows/macOS. Regression tests are required for any bug fixes.

**Architecture Overview**  
The system is a Tauri desktop app with a Rust host for orchestration/UI windows/tray and a Python sidecar that owns audio capture + ASR. UI is React + Tailwind; shared config/schema and model manifest live under `shared/`. IPC Protocol V1 remains stable, only additive changes (new optional params or new messages) are allowed in `shared/ipc/IPC_PROTOCOL_V1.md`. The `AppState` enum (Idle/LoadingModel/Recording/Transcribing/Error) remains unchanged, with session IDs (UUID v4) flowing end‑to‑end. New config fields are added with defaults while keeping `schema_version: 1` semantics.

**Phase 1 — UI/UX Overhaul (Tabs & Polish)**  
1.1 Tab Navigation Component (Complexity: M).  
Files: `src/components/Layout/TabBar.tsx`, `src/components/Layout/TabPanel.tsx`, `src/App.tsx`, `src/store/appStore.ts`.  
Data structures: `appStore.ui.activeTab: 'status' | 'settings' | 'history' | 'replacements'`, `setActiveTab(tab)` action.  
Depends on: none.  
Implementation: Create `TabBar` with inline SVG icons and keyboard navigation; `TabPanel` for content switching; persist active tab in Zustand store only.  
Acceptance: default tab is Status; clicking and arrow keys switch tabs; active indicator animates; tab state persists during session; unit tests added in `src/components/Layout/__tests__/TabBar.test.tsx` or the repo’s existing test location.  
Edge cases: invalid stored tab value falls back to Status; keyboard navigation wraps; icons render without external libs.

1.2 Status Dashboard Tab (Complexity: M).  
Files: `src/components/Status/StatusDashboard.tsx`, `src/components/StatusIndicator.tsx`, `src/store/appStore.ts` selectors.  
Data structures: `lastTranscription`, `todayStats`, `modelStatus`, `sidecarHealth` selectors.  
Depends on: 1.1.  
Implementation: Central animated status, mode badge, hotkey hint, last transcription card, quick stats, model and sidecar health badges.  
Acceptance: renders for each `AppState`; empty/unknown values show placeholders; responsive layout works down to 360px width; tests in `src/components/Status/__tests__/StatusDashboard.test.tsx`.  
Edge cases: no history yet; model not downloaded; sidecar disconnected; `AppState::Error` state shows clear message.

1.3 Settings Tab Reorganization (Complexity: M).  
Files: `src/components/Settings/SettingsPanel.tsx`, `src/components/Settings/ActivationMode.tsx` (new if needed).  
Data structures: `config.recording.activation_mode: 'hold' | 'toggle'`.  
Depends on: 1.1.  
Implementation: Collapsible sections (Audio, Hotkeys, Injection, Model, UI) with icons and help text; prominent activation mode radio group.  
Acceptance: sections expand/collapse; mode selection updates config; keyboard accessible; tests for expand/collapse and mode switching.  
Edge cases: all sections collapsed; persisted collapse state is optional and should not enter config.

1.4 History Tab Enhancement (Complexity: M).  
Files: move `src/components/Settings/HistoryPanel.tsx` to `src/components/History/HistoryPanel.tsx`, update imports in `src/App.tsx`.  
Data structures: `history.items[]` with `text`, `timestamp`, `durationMs`, `confidence`.  
Depends on: 1.1.  
Implementation: search bar, entry cards, copy button, clear‑all with confirm dialog, empty state illustration.  
Acceptance: search filters client‑side case‑insensitively; copy uses clipboard and reports success/failure; confirm required for clear‑all; tests in `src/components/History/__tests__/HistoryPanel.test.tsx`.  
Edge cases: large history list performance; clipboard permission denied; no results after filtering.

1.5 Replacements Tab Polish (Complexity: M).  
Files: `src/components/Replacements/*`, `src/components/Layout/TabBar.tsx` (rule count badge).  
Data structures: `replacementRules[]` with `pattern`, `replacement`, `enabled`, `isRegex`.  
Depends on: 1.1.  
Implementation: visual separation between user rules and presets, inline regex validation, preset cards with Apply, tab badge with rule count.  
Acceptance: invalid regex shows inline error; preset applies and updates list; CRUD works as before; tests updated or added near existing Replacements tests.  
Edge cases: regex compilation failure; duplicate rules; rule count reflects enabled rules only.

**Phase 2 — Recording Overlay & Tray Indicator**  
2.1 Floating Overlay Window (Tauri Config) (Complexity: M).  
Files: `src-tauri/tauri.conf.json`, `src-tauri/src/lib.rs`.  
Functions: `create_recording_overlay_window()`, `show_recording_overlay()`, `hide_recording_overlay()`.  
Depends on: none.  
Implementation: Add `recording-overlay` window config; transparent, always‑on‑top, no decorations, skip taskbar; show/hide based on `AppState` transitions.  
Acceptance: overlay window created at startup, hidden by default; shows only during Recording; does not steal focus; cross‑platform behavior validated.  
Edge cases: multi‑monitor placement; DPI scaling; click‑through not supported on some OS versions (fallback to normal input).

2.2 Overlay React UI (Complexity: M).  
Files: `src/overlay/main.tsx`, `src/overlay/RecordingOverlay.tsx`, `src/overlay/Waveform.tsx`, `index-overlay.html`, `vite.config.ts`.  
Functions: `useAudioLevels()`, `useRecordingTimer()`, `renderWaveform(ctx, samples)`.  
Depends on: 2.1 and existing event emissions for `audio:level` and `state_changed`.  
Implementation: Minimal React root; pill UI with pulsing red dot, elapsed timer, waveform canvas; subscribe to Tauri events; fade in/out.  
Acceptance: overlay renders correctly; timer updates every 100ms; waveform renders with mock data; hides on non‑Recording state; tests in `src/overlay/__tests__/RecordingOverlay.test.tsx`.  
Edge cases: no audio level events; event handler cleanup on unmount; timer drift.

2.3 System Tray State Indicator (Complexity: S).  
Files: `src-tauri/src/tray.rs`, `src-tauri/icons/` or `src-tauri/assets/`.  
Functions: `icon_for_state(state: AppState)`, `tooltip_for_state(state: AppState)`.  
Depends on: AppState manager existing hooks.  
Implementation: Add icon variants and swap on state change; update tooltip.  
Acceptance: icon and tooltip change across all states; unit test mapping in `src-tauri/src/tray.rs` or `src-tauri/src/tray_tests.rs`.  
Edge cases: missing icon assets; state mismatch.

2.4 Full Tray Context Menu (Complexity: L).  
Files: `src-tauri/src/tray.rs`, `src-tauri/src/tray_menu.rs` (new), `src-tauri/src/clipboard.rs` if needed.  
Functions: `build_tray_menu(state, recent, devices, mode, enabled)`, `handle_tray_action(action_id)`.  
Depends on: 2.3 and history/device availability.  
Implementation: Enable/Disable toggle, Mode submenu, Recent Transcriptions (last 5), Microphone submenu, Open Settings, About, Quit; rebuild on state changes.  
Acceptance: menu reflects current state; actions fire correctly; recent transcription click copies text; tests validate menu item generation and action routing.  
Edge cases: macOS menu item limits; device list empty; clipboard failure; long transcription truncated safely.

**Phase 3 — Audio Feedback**  
3.1 Audio Assets (Complexity: S).  
Files: `src-tauri/assets/sounds/start.ogg`, `stop.ogg`, `cancel.ogg`, `error.ogg`, `src-tauri/tauri.conf.json` resources.  
Depends on: none.  
Implementation: add short OGG files (<100KB) with appropriate licensing metadata.  
Acceptance: assets included in bundle; filenames referenced by code; verified size constraints.  
Edge cases: missing resources in release build.

3.2 Audio Playback in Rust (Complexity: M).  
Files: `src-tauri/src/audio_cue.rs`, `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs` initialization.  
Data structures: `enum Cue { Start, Stop, Cancel, Error }`, `struct AudioCuePlayer { stream, handle, buffers }`.  
Depends on: 3.1.  
Implementation: use `rodio` to preload cues; `AudioCuePlayer::play(cue)` is non‑blocking; respect `config.audio.audio_cues_enabled`.  
Acceptance: cues play when enabled; no blocking on main thread; graceful no‑output‑device handling; tests validate cue selection and config gating.  
Edge cases: missing audio device; rodio init failure; rapid repeated cues.

3.3 State Machine Integration (Complexity: M).  
Files: `src-tauri/src/recording.rs`, `src-tauri/src/integration.rs` or `src-tauri/src/state_machine.rs`.  
Functions: hook `AudioCuePlayer::play` into transitions for Recording, Transcribing, Cancel, Error.  
Depends on: 3.2.  
Implementation: play start cue before recording begins; stop cue on transition to Transcribing; cancel on explicit cancel; error on `UserError`.  
Acceptance: correct cue per transition; cue not captured in recording; integration tests in `src-tauri/tests/recording_cues.rs`.  
Edge cases: double‑tap cancel; repeated error emissions; cue disabled in config.

**Phase 4 — Language Selection & Whisper Support**  
4.1 Expand Model Manifest (Complexity: M).  
Files: `shared/model/MODEL_MANIFEST.json`.  
Data structures: add `family`, `languages[]`, `hf_repo`, `hf_filename`, `size_mb`, `speed`, `quality`.  
Depends on: none.  
Implementation: add Whisper base/small/medium entries with multilingual language codes; keep Parakeet entry as English only.  
Acceptance: JSON validates; manifest consumed by UI; new models visible.  
Edge cases: large language list size; missing `hf_repo` for existing models.

4.2 Sidecar Whisper Support (Complexity: L).  
Files: `sidecar/asr/engine.py`, `sidecar/asr/whisper_engine.py` (new), `sidecar/model_cache.py`, `sidecar/pyproject.toml`, `shared/ipc/IPC_PROTOCOL_V1.md`.  
Functions: `AsrEngine.initialize(model_id, language=None)`, `WhisperEngine.transcribe(audio, language)`.  
Depends on: 4.1 and additive IPC update.  
Implementation: add Whisper engine using `faster-whisper` (preferred) or `openai-whisper`; dispatch by model family; add optional `language` param in IPC; keep Parakeet path unchanged.  
Acceptance: Whisper models load and transcribe with chosen language; Parakeet behavior unchanged; IPC remains backward compatible; unit tests in `sidecar/tests/test_asr_whisper.py`.  
Edge cases: missing dependency, GPU unavailable, unsupported language codes, model download failure; emit `UserError` with actionable message.

4.3 Config Schema Update (Complexity: M).  
Files: `shared/schema/AppConfig.schema.json`, `src-tauri/src/config.rs`, `src/types.ts`.  
Data structures: `config.model.family`, `config.model.language`, `config.ui.onboarding_completed`, `config.ui.theme`.  
Depends on: none.  
Implementation: add optional fields with defaults; migration in Rust config loader fills missing values; keep `schema_version: 1`.  
Acceptance: old configs load without changes; defaults applied; schema validation passes.  
Edge cases: invalid enum values; null language vs `'auto'`.

4.4 Model Settings UI (Complexity: M).  
Files: `src/components/Settings/ModelSettings.tsx`, `src/store/appStore.ts` selectors.  
Functions: `onModelFamilyChange`, `onLanguageChange`, `resolveModelDownloadStatus`.  
Depends on: 4.1 and 4.3.  
Implementation: model family cards and language dropdown (Whisper only); show download status and “currently loaded”.  
Acceptance: switching family updates UI and config; language dropdown shown only for Whisper; downloads trigger; tests in `src/components/Settings/__tests__/ModelSettings.test.tsx`.  
Edge cases: selected language not supported by model; missing model manifest entry.

**Phase 5 — First‑Run Onboarding**  
5.1 Onboarding Wizard Component (Complexity: M).  
Files: `src/components/Onboarding/OnboardingWizard.tsx`, `src/components/Onboarding/steps/*`.  
Data structures: `wizardStep: 0..3`, `config.ui.onboarding_completed`.  
Depends on: 4.3.  
Implementation: 4 steps (Welcome, Microphone, Model, Hotkey) with skip buttons and progress dots; reuses existing microphone test if available.  
Acceptance: step navigation works; skip does not set completion; completion sets config flag; tests in `src/components/Onboarding/__tests__/OnboardingWizard.test.tsx`.  
Edge cases: no microphone devices; model download fails; hotkey conflict.

5.2 Onboarding Trigger (Complexity: S).  
Files: `src/App.tsx`, `src/components/Settings/UISettings.tsx` (add “Reset onboarding”).  
Depends on: 5.1 and 4.3.  
Implementation: render wizard when `onboarding_completed` is false; add reset toggle.  
Acceptance: first run shows wizard; completion returns to main UI; reset re‑opens wizard.  
Edge cases: config not yet loaded at startup; asynchronous config updates.

**Phase 6 — Dark/Light Theme**  
6.1 Theme Infrastructure (Complexity: M).  
Files: `tailwind.config.js`, `src/index.css`, `src/store/appStore.ts`, `src/main.tsx`.  
Data structures: `config.ui.theme: 'system' | 'light' | 'dark'`, `ui.resolvedTheme`.  
Functions: `resolveTheme(configTheme, systemPref)`, `applyThemeClass(theme)`.  
Depends on: 4.3.  
Implementation: Tailwind `darkMode: 'class'`, apply `dark` class to `<html>`; listen to `matchMedia` changes.  
Acceptance: system theme respected when set to system; manual overrides work; tests for `resolveTheme` logic.  
Edge cases: `matchMedia` unavailable in tests; theme change while app open.

6.2 Component Dark Mode Styles (Complexity: L).  
Files: all UI components, including `src/App.tsx`, `src/components/**`, `src/overlay/RecordingOverlay.tsx`.  
Depends on: 6.1.  
Implementation: add `dark:` variants for backgrounds, text, borders, inputs, buttons, cards; ensure contrast.  
Acceptance: full UI readable in both themes; no color regressions; manual spot‑checks on main screens.  
Edge cases: SVG icon color inheritance; translucent overlay visibility.

6.3 Theme Toggle in Settings (Complexity: S).  
Files: `src/components/Settings/UISettings.tsx`.  
Depends on: 6.1.  
Implementation: three‑way toggle (System/Light/Dark) that updates config and re‑applies class.  
Acceptance: toggle updates instantly and persists; tests cover toggle logic.  
Edge cases: rapid toggling; system change while in manual mode.

**Phase 7 — CI/CD for Windows & macOS**  
7.1 Expand CI Matrix (Complexity: M).  
Files: `.github/workflows/test.yml`, `.github/workflows/build.yml`.  
Depends on: none.  
Implementation: test matrix `[ubuntu-latest, macos-latest, windows-latest]`; build matrix `[macos-latest, windows-latest]`; cache Cargo and node_modules; install OS‑specific deps.  
Acceptance: CI runs on all OS; caches hit on subsequent runs; builds succeed on macOS and Windows.  
Edge cases: CI timeouts; cache invalidation.

7.2 Platform‑Specific Test Fixes (Complexity: L).  
Files: varies; likely `src-tauri/src/*`, `sidecar/tests/*`, `src/components/**`.  
Depends on: 7.1, plus any new tests from earlier phases.  
Implementation: fix path separator issues, permissions, audio stubs; add `#[cfg(target_os = "...")]` guards as needed; add regression tests for each fix.  
Acceptance: `npm run test` and Rust tests pass on all OS; no skipped tests without justification.  
Edge cases: flaky timing tests; OS‑specific clipboard behavior.

**Critical Path and Dependencies**  
1. Config/schema additions in 4.3 must land before any UI that reads new fields (4.4, 5.1, 5.2, 6.1, 6.3).  
2. Whisper engine (4.2) depends on manifest (4.1) and IPC additive update.  
3. Overlay UI (2.2) depends on overlay window creation (2.1) and event availability.  
4. Tray menu (2.4) depends on tray indicator (2.3) and history/mode/device data sources.  
5. CI fixes (7.2) depend on expanded CI (7.1) and new tests across phases.

**Parallel Execution Plan**  
1. Front‑end stream: Phase 1 tasks (1.1–1.5), Phase 5, Phase 6, and Phase 4.4 can proceed in parallel with Rust/sidecar work. Coordinate on `src/App.tsx` and `src/store/appStore.ts` to avoid conflicts.  
2. Rust host stream: Phase 2 tasks (2.1–2.4) and Phase 3 tasks (3.1–3.3) can proceed in parallel; coordinate on `src-tauri/src/lib.rs` and shared AppState hooks.  
3. Sidecar stream: Phase 4.2 can proceed in parallel with front‑end work after manifest/schema decisions are finalized; coordinate on IPC and model manifest updates.  
4. CI stream: Phase 7.1 can start early; Phase 7.2 should run after new tests are added.

**Risk Mitigation**  
1. Overlay click‑through differences across OS. Mitigation: implement OS‑specific fallbacks and manual test checklist for Windows/macOS; fail safe to normal input if unsupported.  
2. Audio cue capture in recordings. Mitigation: play cue before starting capture with minimal delay and document behavior; add test to ensure cue call happens before `start_capture`.  
3. Whisper dependency and performance. Mitigation: guard initialization with clear error messages, optional dependency handling, and CPU fallback; add UI hint for model size and speed.  
4. Config migration errors. Mitigation: default missing fields in Rust config loader; add schema validation tests and config load tests.  
5. CI flakiness on Windows/macOS. Mitigation: stabilize timing‑sensitive tests, use OS‑specific guards, and add regression tests for each fix.