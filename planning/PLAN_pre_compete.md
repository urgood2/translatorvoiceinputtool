# PLAN v0 — Voice Input Tool: Usability & Polish Release

**Version:** 0 (initial draft from interview)
**Date:** 2026-02-13
**Base commit:** 6a260e5 (main)
**Brownfield baseline:** planning/BROWNFIELD_PREFLIGHT.md
**Interview transcript:** planning/INTERVIEW_TRANSCRIPT.md

---

## Goal

Transform the Voice Input Tool from a functional-but-bare-bones prototype into a polished, cross-platform desktop application that feels professional on both Windows and macOS. Add industry-standard features (audio cues, multilingual support, recording overlay) and ensure robust CI coverage.

---

## Architecture Constraints (Brownfield Guardrails)

1. **IPC Protocol V1 is LOCKED** — no breaking changes to `shared/ipc/IPC_PROTOCOL_V1.md`. All sidecar protocol changes must be additive (new methods, new optional params).
2. **Existing state machine** — `AppState` enum (Idle/LoadingModel/Recording/Transcribing/Error) must not break. New states require migration.
3. **Config schema** — `schema_version: 1` must remain backward-compatible. New fields get defaults. Existing fields keep semantics.
4. **Sidecar boundary** — The Python sidecar owns audio capture + ASR. The Rust host owns orchestration + injection + UI. This boundary stays.
5. **Session ID correlation** — UUID v4 session IDs flow through the entire pipeline. New features must respect this pattern.

---

## Phases

### Phase 1: UI/UX Overhaul — Tab Layout & Polish
**Goal:** Reorganize the monolithic settings panel into a navigable, polished interface.
**Risk:** Low — purely frontend, no backend changes.

#### Task 1.1: Tab Navigation Component
- **Files:** New `src/components/Layout/TabBar.tsx`, `src/components/Layout/TabPanel.tsx`
- **Modify:** `src/App.tsx` (swap single panel for tab layout)
- **Details:**
  - Horizontal tab bar: **Status** | **Settings** | **History** | **Replacements**
  - SVG icons for each tab (inline, no icon library dependency)
  - Active tab indicator with subtle animation
  - Persist last-active tab in Zustand store (not config — ephemeral)
- **Tests:** Tab switching, keyboard navigation (arrow keys), active state rendering

#### Task 1.2: Status Dashboard Tab
- **Files:** New `src/components/Status/StatusDashboard.tsx`
- **Modify:** `src/components/StatusIndicator.tsx` (enhance, don't replace)
- **Details:**
  - Central status display: large animated state indicator
  - Current mode badge (Push-to-Talk / Push-to-Start/Stop)
  - Hotkey hint display ("Hold Ctrl+Shift+Space to record")
  - Last transcription preview card (text + timestamp + confidence)
  - Quick stats: total transcriptions today, total audio time
  - Model status badge (ready / downloading / not downloaded)
  - Sidecar health indicator
- **Tests:** Renders correctly for each app state, responsive layout

#### Task 1.3: Settings Tab Reorganization
- **Files:** Modify `src/components/Settings/SettingsPanel.tsx`
- **Details:**
  - Group into collapsible sections: Audio, Hotkeys, Injection, Model, UI
  - Add section headers with icons
  - Activation mode selector: prominent radio group with descriptions
    - "Push-to-Talk (Hold)" — Hold the hotkey while speaking. Release to stop.
    - "Push-to-Start/Stop (Toggle)" — Press once to start. Press again to stop.
  - Better spacing, labels, help text for each setting
- **Tests:** Section expand/collapse, mode switching

#### Task 1.4: History Tab Enhancement
- **Files:** Modify `src/components/Settings/HistoryPanel.tsx` → move to `src/components/History/HistoryPanel.tsx`
- **Details:**
  - Search bar (client-side filter on transcription text)
  - Entry cards: text, timestamp, duration, confidence badge, copy button
  - Clear all with confirmation dialog
  - Empty state illustration/message
- **Tests:** Search filtering, copy action, clear confirmation

#### Task 1.5: Replacements Tab Polish
- **Files:** Modify existing `src/components/Replacements/` components
- **Details:**
  - Better visual separation between user rules and presets
  - Inline validation feedback for regex patterns
  - Preset cards with description and "Apply" button
  - Rule count badge on tab
- **Tests:** Rule CRUD, preset loading, validation feedback

---

### Phase 2: Recording Overlay & Tray Indicator
**Goal:** Provide clear visual feedback when recording is active.
**Risk:** Medium — requires new Tauri window, platform-specific always-on-top behavior.

#### Task 2.1: Floating Overlay Window (Tauri Config)
- **Files:** Modify `src-tauri/tauri.conf.json` (add second window config), `src-tauri/src/lib.rs` (window creation)
- **Details:**
  - New Tauri window: `recording-overlay`
    - Transparent background, no decorations, always-on-top
    - Small size (~300x60px), positioned at top-center of screen
    - Click-through on Windows/macOS (mouse events pass through)
    - Hidden by default, shown/hidden on state change
  - Window management in Rust: show on `Recording` state, hide on others
- **Migration risk:** Tauri 2 multi-window is well-supported. Test on both platforms.

#### Task 2.2: Overlay React UI
- **Files:** New `src/overlay/main.tsx`, `src/overlay/RecordingOverlay.tsx`, `src/overlay/Waveform.tsx`
- **Modify:** `vite.config.ts` (add overlay entry point), `index-overlay.html`
- **Details:**
  - Separate React root for overlay window (minimal bundle)
  - Pill design: `[🔴 Recording  00:03  ~~~waveform~~~]`
  - Pulsing red dot (CSS animation)
  - Elapsed timer (updates every 100ms)
  - Mini waveform: Canvas-based, ~100px wide, renders last ~50 audio level samples
  - Subscribes to `audio:level` Tauri events for waveform data
  - Subscribes to `state_changed` for show/hide
  - Dark semi-transparent background with rounded corners
  - Smooth fade in/out transitions
- **Tests:** Waveform rendering with mock data, timer accuracy, state-driven visibility

#### Task 2.3: System Tray State Indicator
- **Files:** Modify `src-tauri/src/tray.rs`
- **Details:**
  - Icon variants: `idle` (default), `recording` (red), `transcribing` (yellow), `disabled` (gray)
  - Generate icon variants at build time (or embed multiple PNGs)
  - Swap icon on state change via `AppStateManager` listener
  - Tray tooltip updates: "Voice Input - Idle", "Voice Input - Recording...", etc.
- **Tests:** Icon swap on state transition (unit test the mapping logic)

#### Task 2.4: Full Tray Context Menu
- **Files:** Modify `src-tauri/src/tray.rs`, possibly new `src-tauri/src/tray_menu.rs`
- **Details:**
  - Dynamic menu items:
    - **Enable/Disable** toggle (checkmark when enabled)
    - **Mode** submenu: Push-to-Talk / Push-to-Start/Stop (radio items)
    - Separator
    - **Recent Transcriptions** (last 5, truncated to ~50 chars, click → copy to clipboard)
    - Separator
    - **Microphone** submenu (list devices, checkmark on active)
    - Separator
    - **Open Settings** (brings main window to front)
    - **About Voice Input Tool** (version info dialog)
    - **Quit**
  - Menu rebuilds on: new transcription, device list change, mode change, enable/disable toggle
- **Platform notes:** Tauri 2 `SystemTray` menu API works cross-platform. Test item limits on macOS (has max menu items).
- **Tests:** Menu item generation, event handling for menu clicks

---

### Phase 3: Audio Feedback
**Goal:** Provide auditory confirmation for recording start/stop/cancel.
**Risk:** Low — straightforward audio playback.

#### Task 3.1: Audio Assets
- **Files:** New `src-tauri/assets/sounds/` directory with embedded audio files
- **Details:**
  - Record/source 3-4 short audio cues (< 100KB each):
    - `start.ogg` — soft "boop" or click (recording begins)
    - `stop.ogg` — slightly different tone (recording ends, transcribing)
    - `cancel.ogg` — descending tone (recording cancelled)
    - `error.ogg` — gentle alert (error occurred)
  - OGG Vorbis format for small size + cross-platform support
  - Embed as Rust `include_bytes!()` or bundle as resources

#### Task 3.2: Audio Playback in Rust
- **Files:** New `src-tauri/src/audio_cue.rs`, modify `src-tauri/Cargo.toml` (add `rodio` dependency)
- **Details:**
  - Use `rodio` crate for audio playback (cross-platform, lightweight)
  - `AudioCuePlayer` struct: preloads cue buffers on init
  - `play(cue: Cue)` — fire-and-forget async playback on system default output
  - Respects `config.audio.audio_cues_enabled` setting
  - Must NOT play through the recording input device
  - Must NOT block the main thread
- **Tests:** Cue selection logic, config respect (enabled/disabled)

#### Task 3.3: Integration with State Machine
- **Files:** Modify `src-tauri/src/integration.rs`, `src-tauri/src/recording.rs`
- **Details:**
  - Play `start` cue when `RecordingController` transitions to Recording
  - Play `stop` cue when `RecordingController` transitions to Transcribing
  - Play `cancel` cue on double-tap cancel
  - Play `error` cue on `UserError` emission
  - Timing: play cue *before* starting recording (so the beep isn't captured)
- **Tests:** Integration test: state transition triggers correct cue

---

### Phase 4: Language Selection & Whisper Support
**Goal:** Allow users to transcribe in languages beyond English.
**Risk:** High — touches sidecar ASR engine, model manifest, config schema, and UI.

#### Task 4.1: Expand Model Manifest
- **Files:** Modify `shared/model/MODEL_MANIFEST.json`
- **Details:**
  - Add model entries for Whisper variants:
    ```json
    {
      "id": "openai/whisper-base",
      "family": "whisper",
      "name": "Whisper Base (Multilingual)",
      "languages": ["auto", "en", "es", "fr", "de", "it", "pt", "zh", "ja", "ko", ...],
      "size_mb": 290,
      "speed": "fast",
      "quality": "good",
      "description": "Good balance of speed and accuracy for 99 languages"
    }
    ```
  - Keep existing Parakeet entry with `"family": "parakeet"`, `"languages": ["en"]`
  - Add Whisper small, medium variants with size/quality tradeoffs
  - Schema: `id`, `family`, `name`, `languages[]`, `size_mb`, `speed`, `quality`, `description`, `hf_repo`, `hf_filename`

#### Task 4.2: Sidecar Whisper Support
- **Files:** Modify `sidecar/asr/`, `sidecar/model_cache.py`, `sidecar/pyproject.toml`
- **Details:**
  - Add `openai-whisper` or `faster-whisper` as optional dependency
  - `AsrEngine` needs model-family dispatch: Parakeet pipeline vs Whisper pipeline
  - `asr.initialize` already accepts `model_id` — use to select model
  - Add optional `language` param to `asr.initialize` (additive protocol change)
  - Model cache handles different HuggingFace repos for each model family
- **Protocol addendum:** New optional param `language` on `asr.initialize`. Backward compatible (defaults to `null` = auto-detect or English for Parakeet).

#### Task 4.3: Config Schema Update
- **Files:** Modify `shared/schema/AppConfig.schema.json`, Rust `src-tauri/src/config.rs`, TS `src/types.ts`
- **Details:**
  - Add `config.model.language: string | null` (default: `null` for auto)
  - Add `config.model.family: "parakeet" | "whisper"` (default: `"parakeet"`)
  - Schema version stays 1 (additive — new fields with defaults)
  - Rust config migration: add defaults for new fields when loading old configs

#### Task 4.4: Model Settings UI
- **Files:** Modify `src/components/Settings/ModelSettings.tsx`
- **Details:**
  - Two-step selector:
    1. Model family cards (Parakeet English / Whisper Multilingual) with size + quality info
    2. Language dropdown (only shown for Whisper family)
  - Download/cache status per model
  - "Currently loaded" indicator
  - Size estimate and download time hint
- **Tests:** Model family selection, language dropdown visibility logic, download trigger

---

### Phase 5: First-Run Onboarding
**Goal:** Guide new users through initial setup so they can start transcribing in under 2 minutes.
**Risk:** Low — new component, no backend changes except config flag.

#### Task 5.1: Onboarding Wizard Component
- **Files:** New `src/components/Onboarding/OnboardingWizard.tsx`, step components
- **Details:**
  - 4-step wizard:
    1. **Welcome** — What this app does, privacy note (all processing is local)
    2. **Microphone** — Select mic, test it (reuse MicrophoneTest component)
    3. **Model** — Pick model + language, trigger download (show progress)
    4. **Hotkey** — Show current hotkey, let user customize, explain modes
  - Final screen: "You're ready! Press [hotkey] to start recording."
  - Skip button on each step (for power users)
  - Progress dots at bottom
  - Config flag: `config.ui.onboarding_completed: boolean`

#### Task 5.2: Onboarding Trigger
- **Files:** Modify `src/App.tsx`
- **Details:**
  - On app launch, check `config.ui.onboarding_completed`
  - If false, render OnboardingWizard instead of main UI
  - On completion, set flag and transition to main UI
  - "Reset onboarding" option in Settings for re-running

---

### Phase 6: Dark/Light Theme
**Goal:** Respect system theme preference and allow manual override.
**Risk:** Low — Tailwind dark mode is straightforward.

#### Task 6.1: Theme Infrastructure
- **Files:** Modify `tailwind.config.js` (add `darkMode: 'class'`), `src/index.css`, `src/store/appStore.ts`
- **Details:**
  - Detect system preference via `window.matchMedia('(prefers-color-scheme: dark)')`
  - Config field: `config.ui.theme: "system" | "light" | "dark"` (default: `"system"`)
  - Apply `dark` class to `<html>` element based on resolved theme
  - Store tracks resolved theme for components

#### Task 6.2: Component Dark Mode Styles
- **Files:** Modify all component files with Tailwind `dark:` variants
- **Details:**
  - Background: `bg-white dark:bg-gray-900`
  - Text: `text-gray-900 dark:text-gray-100`
  - Borders, inputs, buttons, cards — all get dark variants
  - Overlay window: already dark-themed (transparent), minimal changes
  - Test on both light and dark system themes

#### Task 6.3: Theme Toggle in Settings
- **Files:** Modify Settings UI
- **Details:**
  - Three-way toggle: System / Light / Dark
  - Preview updates instantly
  - Persists to config

---

### Phase 7: CI/CD for Windows & macOS
**Goal:** Ensure the app builds, tests pass, and core features work on both platforms.
**Risk:** Medium — CI configuration, platform-specific test failures.

#### Task 7.1: Expand CI Matrix
- **Files:** Modify `.github/workflows/test.yml`, `.github/workflows/build.yml`
- **Details:**
  - Test workflow: matrix with `[ubuntu-latest, macos-latest, windows-latest]`
  - Build workflow: matrix with `[macos-latest, windows-latest]` (release builds)
  - Cache Cargo registry + target dir per platform
  - Cache node_modules per platform
  - Install platform-specific dependencies in CI (WebKit on Linux, Xcode on Mac, etc.)

#### Task 7.2: Platform-Specific Test Fixes
- **Files:** Various — depends on failures found
- **Details:**
  - Run existing test suite on both platforms, fix any failures
  - Common issues: path separators, file permissions, audio API stubs
  - Add `#[cfg(target_os = "...")]` for platform-specific test code
  - Ensure sidecar tests work cross-platform (Python path handling)

#### Task 7.3: Manual QA Checklist
- **Files:** New `docs/QA_CHECKLIST.md`
- **Details:**
  - Platform-specific QA items:
    - [ ] Hotkey registration works
    - [ ] Recording captures audio
    - [ ] Text injection works into Notepad/TextEdit/browser
    - [ ] Tray icon appears and menu works
    - [ ] Overlay window appears and is always-on-top
    - [ ] Audio cues play through speakers (not mic)
    - [ ] Model download completes successfully
    - [ ] App launches on system startup (if implemented)
    - [ ] Focus guard prevents wrong-window injection
    - [ ] Config persists across app restarts

---

## Dependency Graph

```
Phase 1 (UI Layout) ──────────────────────────────────────┐
  ├── 1.1 Tab Navigation                                   │
  ├── 1.2 Status Dashboard (depends on 1.1)                │
  ├── 1.3 Settings Reorg (depends on 1.1)                  │
  ├── 1.4 History Enhancement (depends on 1.1)             │
  └── 1.5 Replacements Polish (depends on 1.1)             │
                                                           │
Phase 2 (Overlay + Tray) ─── can start parallel with 1.x  │
  ├── 2.1 Overlay Window Config                            │
  ├── 2.2 Overlay React UI (depends on 2.1)                │
  ├── 2.3 Tray State Indicator (independent)               │
  └── 2.4 Full Tray Menu (depends on 2.3)                  │
                                                           │
Phase 3 (Audio Feedback) ─── can start parallel            │
  ├── 3.1 Audio Assets                                     │
  ├── 3.2 Audio Playback (depends on 3.1)                  │
  └── 3.3 State Integration (depends on 3.2)               │
                                                           │
Phase 4 (Languages) ─── depends on Phase 1.3 for UI       │
  ├── 4.1 Model Manifest                                   │
  ├── 4.2 Sidecar Whisper (depends on 4.1)                 │
  ├── 4.3 Config Schema (depends on 4.1)                   │
  └── 4.4 Model Settings UI (depends on 4.1, 4.3, 1.3)    │
                                                           │
Phase 5 (Onboarding) ─── depends on Phase 1, 4            │
  ├── 5.1 Wizard Component                                 │
  └── 5.2 Trigger Logic (depends on 5.1)                   │
                                                           │
Phase 6 (Theme) ─── can start after Phase 1               │
  ├── 6.1 Theme Infrastructure                             │
  ├── 6.2 Component Dark Styles (depends on 6.1)           │
  └── 6.3 Theme Toggle (depends on 6.1)                    │
                                                           │
Phase 7 (CI/CD) ─── can start anytime, ideally early      │
  ├── 7.1 CI Matrix Expansion                              │
  ├── 7.2 Platform Test Fixes (depends on 7.1)             │
  └── 7.3 QA Checklist                                     │
```

## Recommended Execution Order

1. **Phase 7.1** (CI matrix) — Start first so all subsequent work is tested cross-platform
2. **Phase 1** (UI layout) — Foundation for all UI features
3. **Phase 2** (Overlay + tray) and **Phase 3** (audio cues) — Parallel
4. **Phase 4** (Languages) — After UI is in place
5. **Phase 6** (Theme) — After components are finalized
6. **Phase 5** (Onboarding) — Last, since it references all other features
7. **Phase 7.2-7.3** (QA) — Final pass after all features land

---

## Verification Commands

```bash
# Frontend tests
npm run test

# Frontend lint
npm run lint

# Frontend build (type check + vite)
npm run build

# Rust tests
cd src-tauri && cargo test

# Sidecar tests
cd sidecar && python -m pytest

# Full app build
npm run tauri build
```

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Overlay window not always-on-top on Wayland | High | Medium | Detect Wayland, use layer-shell protocol or fall back to tray-only indicator |
| Whisper model too large for casual users | Medium | High | Default to whisper-base (290MB), show size before download, allow whisper-tiny (150MB) |
| Audio cue playback conflicts with recording | Medium | High | Play cue *before* starting recording, add small delay buffer |
| CI runners lack audio hardware for sidecar tests | High | Medium | Mock audio APIs in CI, run hardware tests only in manual QA |
| Tray menu flicker on rapid state changes | Low | Low | Debounce menu rebuilds (200ms) |
| Config migration breaks existing users | Low | High | All new fields have defaults, schema_version stays 1, add migration tests |

---

## Out of Scope (for this iteration)

- Auto-start on system boot
- Cloud sync of settings/history
- Multiple simultaneous model loading
- Plugin/extension system
- Mobile support
- Real-time streaming transcription (currently batch after recording stops)
- Custom audio cue sounds (user-provided)
- Accessibility audit (deferred to follow-up iteration)
