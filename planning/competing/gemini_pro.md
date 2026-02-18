Loaded cached credentials.
Loaded cached credentials.
Server 'agent-mail' supports tool updates. Listening for changes...
Server 'agent-mail' supports resource updates. Listening for changes...
Server 'agent-mail' supports prompt updates. Listening for changes...
Hook registry initialized with 0 hook entries
Here is the detailed implementation plan for enhancing the Voice Input Tool.

---

### **Engineering Plan: Voice Input Tool - Usability & Polish Release**

### 1. Executive Summary (TL;DR)

This plan outlines the engineering effort to evolve the Voice Input Tool from a prototype into a polished, user-friendly desktop application. The work is organized into seven phases, focusing on a complete UI/UX overhaul with a new tabbed interface, the introduction of critical feedback mechanisms like a recording overlay and audio cues, and major feature enhancements including multilingual support via Whisper. The plan emphasizes maintaining architectural integrity, ensuring cross-platform compatibility (Windows & macOS) through robust CI/CD, and designing for parallel development to accelerate delivery.

### 2. Architecture Overview

The application will maintain its existing hybrid architecture:
*   **Frontend:** A React/TypeScript single-page application for the UI, managed by Vite.
*   **Host/Orchestrator:** A Tauri (Rust) application that manages windows, system tray, configuration, hotkeys, and orchestrates the backend.
*   **Sidecar Backend:** A Python process responsible for all audio processing, including device management, recording, and Automatic Speech Recognition (ASR) via an IPC bridge.

New features will integrate as follows:
*   **UI Tabs & Theming:** Implemented entirely within the React frontend, using Tailwind CSS for styling and a Zustand store for state.
*   **Recording Overlay:** A new, separate Tauri window with its own minimal React entry point, controlled by the Rust host.
*   **Audio Cues & Tray Menu:** Implemented within the Rust host, using the `rodio` crate for audio and Tauri's native `SystemTray` API.
*   **Language Support:** A full-stack feature requiring updates to the shared model manifest, the React settings UI, the Rust config and IPC layer, and a new ASR engine path in the Python sidecar.

All IPC changes will be additive to the existing V1 protocol to ensure backward compatibility.

### 3. Phased Implementation Plan

#### **Phase 1: UI/UX Overhaul — Tab Layout & Polish**
**Goal:** Reorganize the settings panel into a polished, tab-based interface.
**Parallelism:** Tasks 1.1 through 1.5 can be developed in parallel after the basic tab structure is in place.

*   **Task 1.1: Implement Core Tab Navigation**
    *   **Complexity:** M
    *   **Files:**
        *   Create `src/components/Layout/Tabs.tsx`
        *   Create `src/components/Layout/Tab.tsx`
        *   Modify `src/App.tsx` to integrate the new layout.
        *   Modify `src/store/uiStore.ts` (or create if non-existent) to manage active tab state.
    *   **Implementation:**
        *   The `Tabs` component will manage the active tab state and render `Tab` components as children.
        *   Use SVGs for icons, embedded directly in the `Tab` component.
        *   The active tab will have a distinct visual style (e.g., bottom border, background color).
        *   The last active tab will be persisted in a Zustand store.
    *   **Acceptance Criteria:**
        1.  The main view displays four tabs: Status, Settings, History, Replacements.
        2.  Clicking a tab switches the visible content panel.
        3.  The active tab is visually distinct.
        4.  On app restart, the previously selected tab is shown.

*   **Task 1.2: Build Status Dashboard Tab**
    *   **Complexity:** M
    *   **Files:**
        *   Create `src/components/Status/StatusDashboard.tsx`
        *   Enhance `src/components/StatusIndicator.tsx`.
    *   **Implementation:**
        *   The dashboard will be the content for the "Status" tab.
        *   It will subscribe to the main app state and config stores to display dynamic data.
        *   It will show the current hotkey, mode, and model status.
        *   A "Last Transcription" card will show data from the history store.
    *   **Acceptance Criteria:**
        1.  A large status indicator animates based on the current app state (e.g., pulsing when recording).
        2.  Correct hotkey and activation mode are displayed from config.
        3.  Sidecar and model status are accurately reflected.
        4.  The most recent transcription appears after being processed.

*   **Task 1.3: Reorganize Settings Tab**
    *   **Complexity:** S
    *   **Files:**
        *   Modify `src/components/Settings/SettingsPanel.tsx` (or its sub-components).
    *   **Implementation:**
        *   Group existing settings into logical, collapsible sections (e.g., `<details>`/`<summary>` elements).
        *   The activation mode selector will be a prominent radio group with descriptive labels.
    *   **Acceptance Criteria:**
        1.  Settings are grouped into "Audio", "Hotkeys", "Injection", "Model", and "UI" sections.
        2.  Sections can be individually collapsed and expanded.
        3.  Activation mode can be switched between "Push-to-Talk" and "Push-to-Start/Stop".

*   **Task 1.4: Enhance History Tab**
    *   **Complexity:** M
    *   **Files:**
        *   Refactor `src/components/Settings/HistoryPanel.tsx` to `src/components/History/HistoryPanel.tsx`.
    *   **Implementation:**
        *   Add a text input for client-side filtering of the history list.
        *   Each history item will be a card with a "Copy" button.
        *   Implement a "Clear All" button that shows a confirmation modal before dispatching the clear action.
    *   **Acceptance Criteria:**
        1.  Typing in the search bar filters the list of transcriptions in real-time.
        2.  Clicking the "Copy" button copies the transcription text to the clipboard.
        3.  The "Clear All" button prompts for confirmation before emptying the history.

---
#### **Phase 2: Recording Overlay & Tray Indicator**
**Goal:** Provide clear, system-wide visual feedback during recording.
**Dependencies:** Depends on the core app state machine.

*   **Task 2.1: Create Floating Overlay Window**
    *   **Complexity:** M
    *   **Files:**
        *   Modify `src-tauri/tauri.conf.json`: add a new window definition for `recording-overlay`.
        *   Modify `src-tauri/src/main.rs` (or a new `src-tauri/src/windows.rs` module): add logic to manage window visibility.
        *   Modify `vite.config.ts`: add a second build input for `overlay.html`.
        *   Create `overlay.html` in the project root.
    *   **Implementation:**
        *   The `recording-overlay` window will be frameless, transparent, always-on-top, and non-interactive (click-through).
        *   The Rust host will listen to `state_changed` events and call `window.show()` or `window.hide()` accordingly.
    *   **Acceptance Criteria:**
        1.  The overlay window appears only when the app state is `Recording`.
        2.  The window is positioned at the top-center of the primary display.
        3.  The window does not capture mouse clicks.

*   **Task 2.2: Design Overlay UI**
    *   **Complexity:** M
    *   **Files:**
        *   Create `src/overlay/main.tsx` (new React root).
        *   Create `src/overlay/RecordingOverlay.tsx`.
        *   Create `src/overlay/Waveform.tsx`.
    *   **Implementation:**
        *   This minimal React app renders the overlay's content.
        *   It subscribes to Tauri events for `audio:level` to draw on a `<canvas>` for the waveform and `state_changed` to manage its own animations.
        *   A timer will be managed with `setInterval`.
    *   **Acceptance Criteria:**
        1.  The overlay displays a pulsing red dot, the word "Recording", and an elapsed timer.
        2.  A small waveform visualizer reacts to incoming audio levels from the `audio:level` event.
        3.  The overlay has a smooth fade-in/out animation.

*   **Task 2.3: Implement System Tray Icon States**
    *   **Complexity:** S
    *   **Files:**
        *   Create icon assets in `src-tauri/icons/`.
        *   Modify `src-tauri/src/main.rs` or a dedicated `src-tauri/src/tray.rs`.
    *   **Implementation:**
        *   Create `idle.png`, `recording.png`, `transcribing.png`, `error.png` icons.
        *   The Rust host will listen to `state_changed` events and call `app_handle.tray_handle().set_icon(...)`.
    *   **Acceptance Criteria:**
        1.  The tray icon is the default icon when idle.
        2.  The tray icon turns to a red variant when recording.
        3.  The tray icon turns to a yellow variant when transcribing.
        4.  The tray tooltip updates to reflect the current state.

*   **Task 2.4: Build Full Tray Context Menu**
    *   **Complexity:** L
    *   **Files:**
        *   Modify `src-tauri/src/tray.rs` (or create).
    *   **Implementation:**
        *   Use `SystemTrayMenu::new()` to build the menu. Use `Submenu` for nested items.
        *   The menu will be rebuilt dynamically in response to events (e.g., `new_transcription`, `config_changed`).
        *   Menu item clicks will emit Tauri events back to the frontend or be handled directly in Rust to modify config.
    *   **Acceptance Criteria:**
        1.  Right-clicking the tray icon opens a context menu.
        2.  The menu contains enable/disable toggles, a mode selector, and the last 5 transcriptions.
        3.  Clicking a recent transcription copies it to the clipboard.
        4.  "Open Settings" brings the main app window to the foreground.

---
#### **Phase 3: Audio Feedback**
**Goal:** Add non-intrusive audio cues for key actions.
**Parallelism:** This phase is independent and can be worked on at any time.

*   **Task 3.1: Add Audio Assets & Playback Service**
    *   **Complexity:** M
    *   **Files:**
        *   Create `src-tauri/assets/sounds/` with `start.ogg`, `stop.ogg`, `cancel.ogg`, `error.ogg`.
        *   Modify `src-tauri/Cargo.toml` to add the `rodio` crate.
        *   Create `src-tauri/src/audio_cues.rs`.
        *   Modify `build.rs` to include the assets.
    *   **Implementation:**
        *   The `audio_cues` module will define an `AudioCuePlayer` that loads sound files into memory on startup.
        *   A `play(cue_name)` method will play the requested sound on a separate thread to avoid blocking.
        *   It will check the app's config before playing any sound.
    *   **Acceptance Criteria:**
        1.  Four distinct audio files are embedded in the binary.
        2.  A `play` function can be called from anywhere in the Rust host.
        3.  Playback is asynchronous.
        4.  If audio cues are disabled in settings, no sound is played.

*   **Task 3.2: Integrate Cues with State Machine**
    *   **Complexity:** S
    *   **Files:**
        *   Modify `src-tauri/src/main.rs` or modules responsible for state transitions.
    *   **Implementation:**
        *   Call `audio_cues::play("start")` just before entering the `Recording` state.
        *   Call `audio_cues::play("stop")` just before entering the `Transcribing` state.
        *   Call `audio_cues::play("error")` when a user-facing error is generated.
    *   **Acceptance Criteria:**
        1.  The "start" sound plays when recording begins.
        2.  The "stop" sound plays when recording ends and transcription starts.
        3.  The "error" sound plays when an operation fails.

---
#### **Phase 4: Language Selection & Whisper Support**
**Goal:** Enable multilingual transcription.
**Critical Path:** This is the most complex, sequential feature. Tasks must be done in order.

*   **Task 4.1: Expand Model Manifest**
    *   **Complexity:** S
    *   **Files:** Modify `shared/model/MODEL_MANIFEST.json`.
    *   **Implementation:** Add new entries for `openai/whisper-base`, `openai/whisper-small`, etc. Add a `family: "whisper"` field and a `languages: [...]` array to these entries.
    *   **Acceptance Criteria:**
        1.  The manifest contains at least one "parakeet" family model and one "whisper" family model.
        2.  Whisper models include an array of supported language codes.

*   **Task 4.2: Add Whisper Support to Sidecar**
    *   **Complexity:** L
    *   **Files:**
        *   Modify `sidecar/pyproject.toml` to add `faster-whisper`.
        *   Modify `sidecar/src/openvoicy_sidecar/asr.py` and `model_cache.py`.
    *   **Implementation:**
        *   In `asr.py`, create a new `WhisperAsrEngine` class alongside the Parakeet one.
        *   The main `initialize` function will read the `model_id` from the manifest, determine the `family`, and instantiate the correct engine.
        *   Add an optional `language: str | None` parameter to the `asr.initialize` RPC method. Pass this to the Whisper engine.
    *   **Acceptance Criteria:**
        1.  The sidecar can successfully initialize a `faster-whisper` model.
        2.  The `asr.initialize` method accepts a new optional `language` parameter.
        3.  The ASR process correctly uses the specified language for transcription.

*   **Task 4.3: Update Config Schema and Types**
    *   **Complexity:** S
    *   **Files:**
        *   Modify `shared/schema/AppConfig.schema.json`.
        *   Modify `src-tauri/src/config.rs`.
        *   Modify `src/types.ts`.
    *   **Implementation:** Add `model.language: string | null` and `model.model_id: string` to the config schema and corresponding Rust/TS types. Ensure default values are provided for backward compatibility.
    *   **Acceptance Criteria:**
        1.  The new fields are present in the JSON schema.
        2.  The Rust and TypeScript types match the schema.
        3.  Loading an old config file correctly applies default values for the new fields.

*   **Task 4.4: Implement Model & Language Selection UI**
    *   **Complexity:** M
    *   **Files:** Modify `src/components/Settings/ModelSettings.tsx`.
    *   **Implementation:**
        *   Fetch the `MODEL_MANIFEST.json` to populate the UI.
        *   Create a selector for the `model_id`.
        *   When a Whisper model is selected, show a dropdown for the `language` field, populated from the manifest.
        *   Saving the settings will update the config and may trigger a new model download via a Tauri command.
    *   **Acceptance Criteria:**
        1.  Users can select between different ASR models listed in the manifest.
        2.  If a multilingual model is chosen, a language selector appears.
        3.  The chosen model and language are saved to the application's configuration.

---
### 4. Critical Path & Dependencies

1.  **UI Foundation (Phase 1):** The tabbed layout (Task 1.1) is a prerequisite for reorganizing the UI (Tasks 1.2-1.5).
2.  **Multilingual Support (Phase 4):** This is the longest dependency chain. The sidecar must be updated before the config and UI can be changed. `4.1 -> 4.2 -> 4.3 -> 4.4`.
3.  **Feature Integration:** The Overlay (Phase 2), Audio Cues (Phase 3), and new Settings UI (Task 4.4) all depend on the core application state machine and config management provided by the Rust host.

**Parallel Workstreams:**
*   **Team 1 (Frontend):** Can work on all of Phase 1, Phase 6 (Theming), and the UI for Phase 5 (Onboarding) and Task 2.2 (Overlay UI) in parallel.
*   **Team 2 (Backend/Rust):** Can work on Phase 3 (Audio Cues), Task 2.1/2.3/2.4 (Tauri Window/Tray), and the Rust part of Phase 4 in parallel.
*   **Team 3 (Python/AI):** Can focus exclusively on Task 4.2 (Sidecar Whisper Support).
*   **Team 4 (CI/CD):** Can work on Phase 7 at any time, but it becomes critical once cross-platform features are merged.

### 5. Risk Mitigation

*   **Risk:** Cross-platform inconsistencies in UI or behavior (especially for the overlay and tray).
    *   **Mitigation:** Implement the CI matrix (Task 7.1) early. All PRs for Phase 2 must include manual test reports/screenshots from both Windows and macOS.
*   **Risk:** Breaking the IPCv1 contract between the host and sidecar.
    *   **Mitigation:** Strictly enforce an "additive changes only" policy. The `language` parameter in `asr.initialize` MUST be optional. Expand the protocol fuzzing test (`test_protocol_fuzz.py`) to cover the new parameter permutations.
*   **Risk:** Performance degradation from the overlay or audio playback.
    *   **Mitigation:** The overlay will be a minimal, separate React application. Audio playback will be fully asynchronous. CPU and memory usage will be part of the acceptance criteria for these features.
*   **Risk:** Large model downloads are slow and may fail.
    *   **Mitigation:** The download process is already handled by the sidecar. The UI (Task 4.4) must provide clear, persistent feedback on download progress and gracefully handle/display any errors reported by the sidecar. Downloads must be pausable and resumable if possible.
