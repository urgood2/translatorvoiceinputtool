

# Implementation Plan — Voice Input Tool: Usability & Polish Release

## Executive Summary (TL;DR)

Transform the Voice Input Tool from a functional prototype into a polished, cross-platform desktop app across 7 phases. The plan is ordered for **incremental delivery**: each phase produces a shippable, testable increment. Phases 1 (UI tabs), 3 (audio cues), 5 (onboarding), and 6 (theme) are low-risk and parallelizable. Phase 2 (overlay) and 4 (multilingual) carry medium-to-high risk and are sequenced carefully. Phase 7 (CI matrix) runs continuously in the background.

**Critical path:** Phase 1 → Phase 5 → Phase 2 → Phase 4. Phases 3, 6, and 7 are independent side-tracks.

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│                     Tauri Host (Rust)                      │
│  ┌─────────┐  ┌──────────┐  ┌───────────┐  ┌──────────┐ │
│  │ AppState │  │ Recording│  │ AudioCue  │  │   Tray   │ │
│  │ Manager  │  │Controller│  │  Player   │  │  Manager │ │
│  └────┬─────┘  └────┬─────┘  └─────┬─────┘  └────┬─────┘ │
│       │              │              │              │       │
│       └──────────────┴──────────────┴──────────────┘       │
│                          │ events                          │
│  ┌───────────────────────┼───────────────────────────────┐ │
│  │              Tauri WebView Windows                     │ │
│  │  ┌─────────────┐  ┌──────────────────┐               │ │
│  │  │ Main Window  │  │ Overlay Window   │               │ │
│  │  │ (Tab Layout) │  │ (Recording Pill) │               │ │
│  │  └─────────────┘  └──────────────────┘               │ │
│  └───────────────────────────────────────────────────────┘ │
│                          │ IPC V1                          │
│  ┌───────────────────────┴───────────────────────────────┐ │
│  │               Python Sidecar                           │ │
│  │  ┌──────────┐  ┌──────────────┐  ┌────────────────┐  │ │
│  │  │ Parakeet │  │ Whisper      │  │ Model Cache    │  │ │
│  │  │ Pipeline │  │ Pipeline     │  │ (HF download)  │  │ │
│  │  └──────────┘  └──────────────┘  └────────────────┘  │ │
│  └───────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
```

**Key constraint:** IPC Protocol V1 is locked. All sidecar changes are additive (new optional params). The `AppState` enum is extended only if absolutely necessary (it isn't for this plan — overlay observes existing states).

---

## Phase Breakdown

---

### Phase 1: UI/UX Overhaul — Tab Layout & Polish

**Goal:** Replace the monolithic panel with a tabbed interface. This is the foundation for Phases 5 and 6.
**Parallelism:** Tasks 1.2–1.5 can run in parallel once 1.1 is merged.
**Risk:** Low.

---

#### Task 1.1: Tab Navigation Component
**Complexity:** S
**Files to create:**
- `src/components/Layout/TabBar.tsx`
- `src/components/Layout/TabPanel.tsx`

**Files to modify:**
- `src/App.tsx` — replace current single-panel render with `<TabBar>` + `<TabPanel>`
- `src/store/appStore.ts` — add `activeTab: "status" | "settings" | "history" | "replacements"` (ephemeral, not persisted to config)

**Implementation details:**
1. `TabBar` renders a horizontal `<nav>` with `<button>` elements for each tab.
2. Each button has an inline SVG icon (no icon library). Icons: dashboard gauge, gear, clock, swap-arrows.
3. Active tab gets a bottom border indicator with a CSS `transition: left, width 200ms ease`.
4. Keyboard: `ArrowLeft`/`ArrowRight` cycle tabs when the tab bar is focused. `Home`/`End` jump to first/last.
5. `TabPanel` is a simple wrapper that conditionally renders the active panel's children.
6. Zustand slice:
   ```ts
   interface UISlice {
     activeTab: TabId;
     setActiveTab: (tab: TabId) => void;
   }
   ```
   No persistence — resets to `"status"` on app launch.

**Acceptance criteria:**
- [ ] Four tabs render with icons and labels
- [ ] Clicking a tab switches visible content
- [ ] Arrow key navigation works with proper ARIA roles (`role="tablist"`, `role="tab"`, `role="tabpanel"`, `aria-selected`)
- [ ] Active indicator animates smoothly
- [ ] Unit tests: tab switching, keyboard nav, active state

**Tests to write:** `src/__tests__/components/Layout/TabBar.test.tsx`

---

#### Task 1.2: Status Dashboard Tab
**Complexity:** M
**Depends on:** Task 1.1
**Files to create:**
- `src/components/Status/StatusDashboard.tsx`

**Files to modify:**
- `src/components/StatusIndicator.tsx` — extract reusable `StatusBadge` sub-component; do NOT delete existing exports (other consumers may exist)

**Implementation details:**
1. Layout: CSS Grid, 2-column on wider views, single-column on narrow.
2. Top section: large animated state indicator (reuse/enhance `StatusIndicator`).
   - Idle: green dot, static
   - Recording: red dot, pulsing CSS animation (`@keyframes pulse`)
   - Transcribing: yellow dot, spinning
   - Error: red triangle, shake animation
3. Mode badge: reads `config.activation.mode` from store → "Push-to-Talk" or "Push-to-Start/Stop".
4. Hotkey hint: reads `config.activation.shortcut` → formats human-readable string.
5. Last transcription card: reads from `transcriptionHistory[0]` in store. Shows text (truncated 200 chars), timestamp (relative: "2 min ago"), confidence percentage.
6. Quick stats: computed from `transcriptionHistory` — count for today, sum of durations. Memoize with `useMemo`.
7. Model status badge: reads `modelState` from store (existing field).
8. Sidecar health: reads `sidecarStatus` from store (existing field).

**Acceptance criteria:**
- [ ] Renders correctly for each `AppState` variant (Idle, LoadingModel, Recording, Transcribing, Error)
- [ ] Stats compute correctly from history data
- [ ] Responsive: stacks to single column below 500px
- [ ] Handles empty history gracefully (empty state message)

**Tests to write:** `src/__tests__/components/Status/StatusDashboard.test.tsx` — render tests for each state, stats computation, empty state.

---

#### Task 1.3: Settings Tab Reorganization
**Complexity:** M
**Depends on:** Task 1.1
**Files to modify:**
- `src/components/Settings/SettingsPanel.tsx`

**Implementation details:**
1. Wrap each logical group in a `<CollapsibleSection title="..." icon={...} defaultOpen={true}>` component.
   - Create `src/components/Layout/CollapsibleSection.tsx` (small utility — `<details>`/`<summary>` with Tailwind styling).
2. Sections: **Audio** (mic select, input level, cues toggle), **Hotkeys** (shortcut config, mode select), **Injection** (method, delay), **Model** (model select, download), **UI** (theme — placeholder for Phase 6, onboarding reset — placeholder for Phase 5).
3. Activation mode selector: prominent `<fieldset>` with two `<label>` cards:
   - Each card: radio input + title + 1-line description
   - Cards use `ring-2 ring-blue-500` when selected
4. Help text: small gray text below relevant inputs explaining the setting.

**Acceptance criteria:**
- [ ] Sections expand/collapse independently
- [ ] Mode selector switches between PTT and PTS/S
- [ ] All existing settings remain functional (no regression)
- [ ] Help text appears below relevant controls

**Tests to write:** `src/__tests__/components/Settings/SettingsPanel.test.tsx` — collapse/expand, mode switching, no regression on config writes.

---

#### Task 1.4: History Tab Enhancement
**Complexity:** M
**Depends on:** Task 1.1
**Files to modify:**
- Move `src/components/Settings/HistoryPanel.tsx` → `src/components/History/HistoryPanel.tsx`
- Update import in `src/App.tsx` (or wherever the tab panel wires it)

**Implementation details:**
1. Search bar: `<input>` at top, debounced (300ms) client-side filter on `entry.text.toLowerCase().includes(query)`.
2. Entry cards: `<div>` per entry with:
   - Transcription text (full, wrapping)
   - Timestamp: `new Date(entry.timestamp).toLocaleString()`
   - Duration: formatted `mm:ss`
   - Confidence: color-coded badge (green >90%, yellow 70-90%, red <70%)
   - Copy button: `navigator.clipboard.writeText(entry.text)` with brief "Copied!" tooltip
3. Clear all: button at bottom → confirmation dialog (native `window.confirm` is fine for MVP, custom modal is stretch).
4. Empty state: centered text + subtle icon when no entries.
5. Virtualize list only if performance is an issue (defer — YAGNI for now; typical history is <1000 entries).

**Acceptance criteria:**
- [ ] Search filters entries in real-time
- [ ] Copy button copies text to clipboard
- [ ] Clear all prompts for confirmation before deleting
- [ ] Empty state renders when no history
- [ ] Confidence badges show correct colors

**Tests to write:** `src/__tests__/components/History/HistoryPanel.test.tsx` — search filter, copy mock, clear confirmation, empty state.

---

#### Task 1.5: Replacements Tab Polish
**Complexity:** S
**Depends on:** Task 1.1
**Files to modify:**
- `src/components/Replacements/ReplacementsList.tsx` (or equivalent existing file)
- `src/components/Replacements/PresetSelector.tsx` (or equivalent)

**Implementation details:**
1. Visual separation: `<hr>` or distinct background between "Your Rules" and "Presets" sections.
2. Inline regex validation: on blur of pattern field, try `new RegExp(value)` — if throws, show red border + error message below input.
3. Preset cards: each preset gets a card with name, description, rule count, "Apply" button.
4. Tab badge: `TabBar` supports an optional `badge?: number` prop on each tab — show rule count on Replacements tab.

**Acceptance criteria:**
- [ ] User rules and presets are visually distinct
- [ ] Invalid regex shows inline error
- [ ] Preset "Apply" adds rules correctly
- [ ] Tab shows rule count badge

**Tests to write:** `src/__tests__/components/Replacements/ReplacementsList.test.tsx` — validation feedback, preset apply, badge count.

---

### Phase 2: Recording Overlay & Tray Indicator

**Goal:** Visual feedback when recording. Requires Tauri multi-window and platform-specific behavior.
**Parallelism:** Tasks 2.1+2.2 form one vertical slice. Tasks 2.3+2.4 form another. Both slices can run in parallel.
**Risk:** Medium — multi-window Tauri, platform differences.

---

#### Task 2.1: Floating Overlay Window (Tauri Config + Rust)
**Complexity:** M
**Depends on:** None (can start immediately)
**Files to modify:**
- `src-tauri/tauri.conf.json` — add window entry:
  ```json
  {
    "label": "recording-overlay",
    "url": "/overlay.html",
    "width": 300,
    "height": 60,
    "decorations": false,
    "transparent": true,
    "alwaysOnTop": true,
    "visible": false,
    "resizable": false,
    "skipTaskbar": true
  }
  ```
- `src-tauri/src/lib.rs` — add window show/hide logic tied to `AppState` changes:
  ```rust
  fn on_state_change(state: &AppState, app: &AppHandle) {
      let overlay = app.get_webview_window("recording-overlay");
      match state {
          AppState::Recording => { overlay.show(); overlay.center(); /* move to top-center */ }
          _ => { overlay.hide(); }
      }
  }
  ```

**Files to create:**
- `index-overlay.html` — minimal HTML entry for overlay window

**Platform considerations:**
- **Windows:** `transparent: true` requires `WebView2` — already a Tauri 2 dependency. Click-through: set `WS_EX_TRANSPARENT` via Tauri's window builder or `set_ignore_cursor_events(true)`.
- **macOS:** `NSWindow.ignoresMouseEvents = true` via Tauri API.

**Acceptance criteria:**
- [ ] Overlay window defined in tauri.conf.json
- [ ] Window shows when app state transitions to Recording
- [ ] Window hides when state transitions away from Recording
- [ ] Window is always-on-top, transparent background, no decorations
- [ ] Mouse clicks pass through the overlay on both Windows and macOS

**Tests to write:** `src-tauri/src/tests/overlay_test.rs` — unit test the state→show/hide mapping logic (mock window handle).

---

#### Task 2.2: Overlay React UI
**Complexity:** M
**Depends on:** Task 2.1
**Files to create:**
- `src/overlay/main.tsx` — separate React entry point
- `src/overlay/RecordingOverlay.tsx` — main overlay component
- `src/overlay/Waveform.tsx` — canvas-based mini waveform
- `index-overlay.html` — (may already exist from 2.1)

**Files to modify:**
- `vite.config.ts` — add rollup input for overlay:
  ```ts
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        overlay: resolve(__dirname, 'index-overlay.html'),
      }
    }
  }
  ```

**Implementation details:**
1. `RecordingOverlay` layout: horizontal flex pill.
   - Left: pulsing red `<div>` (CSS `@keyframes` — `opacity 0.5 ↔ 1` at 1s period).
   - Center: "Recording" label + elapsed timer (`useEffect` with `setInterval(100ms)`).
   - Right: `<Waveform>` canvas.
2. `Waveform`: `<canvas>` 100×40px. Subscribes to `audio:level` Tauri event. Maintains a ring buffer of last 50 samples. Renders vertical bars, latest on the right.
   ```tsx
   listen<number>('audio:level', (event) => {
     samplesRef.current.push(event.payload);
     if (samplesRef.current.length > 50) samplesRef.current.shift();
     drawWaveform();
   });
   ```
3. Fade transitions: wrapper `<div>` with CSS `transition: opacity 200ms`. Opacity controlled by `visible` state from `state_changed` event.
4. Styling: `bg-black/70 rounded-full px-4 py-2 text-white font-mono`.
5. Bundle size target: overlay JS < 50KB gzipped (no heavy deps).

**Acceptance criteria:**
- [ ] Overlay renders the pill design with pulsing dot, timer, waveform
- [ ] Timer increments correctly (±100ms tolerance)
- [ ] Waveform renders from `audio:level` events
- [ ] Smooth fade in on show, fade out on hide
- [ ] Separate Vite entry produces a small bundle

**Tests to write:** `src/__tests__/overlay/RecordingOverlay.test.tsx` — timer increment, waveform canvas mock, fade visibility. `src/__tests__/overlay/Waveform.test.tsx` — ring buffer logic, draw calls.

---

#### Task 2.3: System Tray State Indicator
**Complexity:** S
**Depends on:** None (can start immediately)
**Files to modify:**
- `src-tauri/src/tray.rs`

**Files to create (assets):**
- `src-tauri/icons/tray-idle.png` (16×16, 32×32 variants)
- `src-tauri/icons/tray-recording.png` (red variant)
- `src-tauri/icons/tray-transcribing.png` (yellow variant)
- `src-tauri/icons/tray-disabled.png` (gray variant)

**Implementation details:**
1. Define icon mapping:
   ```rust
   fn tray_icon_for_state(state: &AppState, enabled: bool) -> &'static [u8] {
       if !enabled { return include_bytes!("../icons/tray-disabled.png"); }
       match state {
           AppState::Idle | AppState::LoadingModel => include_bytes!("../icons/tray-idle.png"),
           AppState::Recording => include_bytes!("../icons/tray-recording.png"),
           AppState::Transcribing => include_bytes!("../icons/tray-transcribing.png"),
           AppState::Error(_) => include_bytes!("../icons/tray-recording.png"), // red for error too
       }
   }
   ```
2. Subscribe to state changes in the `AppStateManager` listener. On change, call `tray.set_icon(Icon::Raw(bytes))`.
3. Tooltip: `tray.set_tooltip(&format!("Voice Input - {}", state.display_name()))`.

**Acceptance criteria:**
- [ ] Tray icon changes on each state transition
- [ ] Tooltip text updates to reflect current state
- [ ] Disabled state shows gray icon regardless of app state
- [ ] Icons are crisp at 1x and 2x DPI

**Tests to write:** `src-tauri/src/tests/tray_test.rs` — `tray_icon_for_state` returns correct bytes for each state/enabled combo.

---

#### Task 2.4: Full Tray Context Menu
**Complexity:** M
**Depends on:** Task 2.3
**Files to modify:**
- `src-tauri/src/tray.rs`

**Files to create (optional refactor):**
- `src-tauri/src/tray_menu.rs` — if `tray.rs` gets too large

**Implementation details:**
1. `build_tray_menu(state: &TrayMenuState) -> Menu` function that constructs the full menu tree:
   ```rust
   struct TrayMenuState {
       enabled: bool,
       mode: ActivationMode,
       recent_transcriptions: Vec<String>, // last 5, truncated
       audio_devices: Vec<AudioDevice>,
       active_device_id: Option<String>,
       app_version: String,
   }
   ```
2. Menu structure (as listed in the plan):
   - Enable/Disable (check item)
   - Mode submenu (radio items)
   - Separator
   - Recent transcriptions (5 items, click → clipboard)
   - Separator
   - Microphone submenu (radio items per device)
   - Separator
   - Open Settings
   - About
   - Quit
3. Event handling: each menu item has a unique ID. Match on ID in the `on_menu_event` handler:
   ```rust
   "toggle-enable" => toggle_enabled(),
   "mode-ptt" => set_mode(PushToTalk),
   "mode-pts" => set_mode(PushToStartStop),
   id if id.starts_with("recent-") => copy_transcription(index),
   id if id.starts_with("mic-") => set_microphone(device_id),
   "open-settings" => show_main_window(),
   "about" => show_about_dialog(),
   "quit" => app.exit(0),
   ```
4. Rebuild trigger: call `rebuild_tray_menu()` whenever any of the menu state inputs change.

**Acceptance criteria:**
- [ ] All menu items render correctly
- [ ] Enable/Disable toggles and persists
- [ ] Mode selection changes activation mode
- [ ] Recent transcriptions copy to clipboard on click
- [ ] Microphone selection changes active device
- [ ] Open Settings brings main window to front
- [ ] Quit exits the app
- [ ] Menu rebuilds dynamically on state changes

**Tests to write:** `src-tauri/src/tests/tray_menu_test.rs` — `build_tray_menu` produces correct item count and types for various states.

---

### Phase 3: Audio Feedback

**Goal:** Auditory cues on state transitions.
**Parallelism:** Entirely independent of Phases 1 and 2. Can run in parallel.
**Risk:** Low.

---

#### Task 3.1: Audio Assets
**Complexity:** S
**Depends on:** None
**Files to create:**
- `src-tauri/assets/sounds/start.ogg`
- `src-tauri/assets/sounds/stop.ogg`
- `src-tauri/assets/sounds/cancel.ogg`
- `src-tauri/assets/sounds/error.ogg`

**Implementation details:**
1. Source royalty-free audio cues or generate with a tone generator.
2. Requirements: each file < 100KB, duration < 500ms, OGG Vorbis format.
3. Embed via Tauri's `resources` in `tauri.conf.json`:
   ```json
   "bundle": {
     "resources": ["assets/sounds/*"]
   }
   ```
   OR use `include_bytes!()` for zero-runtime-IO.

**Acceptance criteria:**
- [ ] Four OGG files exist, each < 100KB
- [ ] Files play correctly in a media player (manual verification)
- [ ] Files are bundled in the app (either via resources or include_bytes)

---

#### Task 3.2: Audio Playback in Rust
**Complexity:** M
**Depends on:** Task 3.1
**Files to create:**
- `src-tauri/src/audio_cue.rs`

**Files to modify:**
- `src-tauri/Cargo.toml` — add `rodio = "0.19"` (or latest)
- `src-tauri/src/lib.rs` — register `AudioCuePlayer` in Tauri managed state

**Implementation details:**
1. `AudioCuePlayer`:
   ```rust
   pub struct AudioCuePlayer {
       output_stream: OutputStream,
       sink_handle: OutputStreamHandle,
       cues: HashMap<Cue, Vec<u8>>,
       enabled: AtomicBool,
   }

   pub enum Cue { Start, Stop, Cancel, Error }

   impl AudioCuePlayer {
       pub fn new() -> Result<Self> { /* init rodio, load bytes */ }
       pub fn play(&self, cue: Cue) {
           if !self.enabled.load(Ordering::Relaxed) { return; }
           let data = Cursor::new(self.cues[&cue].clone());
           let source = Decoder::new(data).unwrap();
           let sink = Sink::try_new(&self.sink_handle).unwrap();
           sink.append(source);
           sink.detach(); // fire-and-forget
       }
       pub fn set_enabled(&self, enabled: bool) {
           self.enabled.store(enabled, Ordering::Relaxed);
       }
   }
   ```
2. Plays on the **default output device** — NOT the recording input.
3. `play()` is non-blocking (sink is detached).
4. Config integration: read `config.audio.audio_cues_enabled` on startup and on config change.

**Acceptance criteria:**
- [ ] `AudioCuePlayer::play(Cue::Start)` plays the start sound on the default output
- [ ] When `enabled` is false, `play()` does nothing
- [ ] Playback does not block the calling thread
- [ ] No panic on missing cue data (graceful error)

**Tests to write:** `src-tauri/src/tests/audio_cue_test.rs` — enabled/disabled logic, cue selection. Integration test with actual playback is manual.

---

#### Task 3.3: Integration with State Machine
**Complexity:** S
**Depends on:** Task 3.2
**Files to modify:**
- `src-tauri/src/integration.rs` — (or the file containing the state transition orchestrator)
- `src-tauri/src/recording.rs` — (where recording start/stop lives)

**Implementation details:**
1. In `RecordingController::start_recording()`:
   ```rust
   audio_cue_player.play(Cue::Start);
   tokio::time::sleep(Duration::from_millis(150)).await; // let cue finish before mic opens
   self.begin_capture().await?;
   ```
2. In `RecordingController::stop_recording()`:
   ```rust
   self.end_capture().await?;
   audio_cue_player.play(Cue::Stop);
   ```
3. In cancel handler: `audio_cue_player.play(Cue::Cancel);`
4. In error emission: `audio_cue_player.play(Cue::Error);`

**Critical timing detail:** The 150ms delay before mic open ensures the "start" beep is NOT captured in the recording. The "stop" beep plays AFTER capture ends, so it's also not captured.

**Acceptance criteria:**
- [ ] Start beep plays before recording begins (not captured in audio)
- [ ] Stop beep plays after recording ends
- [ ] Cancel beep plays on double-tap cancel
- [ ] Error beep plays on UserError
- [ ] All cues respect enabled/disabled config

**Tests to write:** Integration test in `src-tauri/src/tests/recording_integration_test.rs` — mock `AudioCuePlayer`, verify `play()` called with correct `Cue` variant at correct point in state transition sequence.

---

### Phase 4: Language Selection & Whisper Support

**Goal:** Multilingual transcription.
**Risk:** High — touches sidecar, config schema, model manifest, and UI.
**Parallelism:** Task 4.1 (manifest) and 4.3 (config schema) can run in parallel. Task 4.2 (sidecar) depends on 4.1. Task 4.4 (UI) depends on 4.1 + 4.3.

---

#### Task 4.1: Expand Model Manifest
**Complexity:** S
**Depends on:** None
**Files to modify:**
- `shared/model/MODEL_MANIFEST.json`

**Implementation details:**
1. Add entries for Whisper models. Each entry:
   ```json
   {
     "id": "openai/whisper-base",
     "family": "whisper",
     "name": "Whisper Base (Multilingual)",
     "languages": ["auto", "en", "es", "fr", "de", "it", "pt", "zh", "ja", "ko", "ar", "hi", "ru", "nl", "pl", "sv", "tr", "uk", "vi", "th"],
     "size_mb": 290,
     "speed": "fast",
     "quality": "good",
     "description": "Good balance of speed and accuracy for 99 languages",
     "hf_repo": "openai/whisper-base",
     "hf_filename": "model.bin"
   }
   ```
2. Add `openai/whisper-small` (966MB, "moderate" speed, "very good" quality) and `openai/whisper-medium` (3.1GB, "slow" speed, "excellent" quality).
3. Existing Parakeet entry: add `"family": "parakeet"`, `"languages": ["en"]`, `"hf_repo"`, `"hf_filename"` if not already present.
4. Validate JSON schema consistency — all entries must have all fields.

**Acceptance criteria:**
- [ ] Manifest contains at least 4 model entries (1 Parakeet + 3 Whisper)
- [ ] Each entry has all required fields
- [ ] `languages` array is correct per model
- [ ] Existing code that reads the manifest doesn't break (add fields, don't change existing ones)

**Tests to write:** Schema validation test in `shared/model/test_manifest.py` or equivalent.

---

#### Task 4.2: Sidecar Whisper Support
**Complexity:** L
**Depends on:** Task 4.1
**Files to modify:**
- `sidecar/asr/engine.py` (or equivalent ASR engine file)
- `sidecar/model_cache.py`
- `sidecar/pyproject.toml` — add `faster-whisper` as optional dependency

**Implementation details:**
1. `pyproject.toml`: add `faster-whisper >= 1.0.0` to optional deps group `[whisper]`.
2. `model_cache.py`: update `download_model()` to handle Whisper HF repos (different directory structure than Parakeet).
3. `engine.py`: factory pattern:
   ```python
   class AsrEngine:
       @staticmethod
       def create(model_id: str, language: str | None = None) -> AsrPipeline:
           manifest = load_manifest()
           model_info = manifest[model_id]
           if model_info["family"] == "parakeet":
               return ParakeetPipeline(model_id)
           elif model_info["family"] == "whisper":
               return WhisperPipeline(model_id, language=language)
           else:
               raise ValueError(f"Unknown model family: {model_info['family']}")
   ```
4. `WhisperPipeline`:
   ```python
   class WhisperPipeline(AsrPipeline):
       def __init__(self, model_id: str, language: str | None = None):
           from faster_whisper import WhisperModel
           model_path = model_cache.get_path(model_id)
           self.model = WhisperModel(model_path, device="cpu", compute_type="int8")
           self.language = language  # None = auto-detect

       def transcribe(self, audio_path: str) -> TranscriptionResult:
           segments, info = self.model.transcribe(
               audio_path,
               language=self.language,
               beam_size=5
           )
           text = " ".join(s.text for s in segments)
           return TranscriptionResult(text=text, language=info.language, confidence=...)
   ```
5. IPC addendum: `asr.initialize` gains optional `language` param. If absent, behavior is unchanged (Parakeet uses English, Whisper auto-detects).

**Acceptance criteria:**
- [ ] `asr.initialize(model_id="openai/whisper-base", language="es")` loads Whisper in Spanish mode
- [ ] `asr.initialize(model_id="..parakeet..")` still works exactly as before (backward compat)
- [ ] Auto-detect (`language=null`) works for Whisper
- [ ] `faster-whisper` is optional — app still works without it if only Parakeet is used
- [ ] Model download works for Whisper models via `model_cache`

**Tests to write:**
- `sidecar/tests/test_asr_engine.py` — factory dispatch, Parakeet fallback, Whisper initialization (mocked model)
- `sidecar/tests/test_whisper_pipeline.py` — transcription with mock model, language param handling

---

#### Task 4.3: Config Schema Update
**Complexity:** S
**Depends on:** None (can run in parallel with 4.1)
**Files to modify:**
- `shared/schema/AppConfig.schema.json` — add `model.language` and `model.family`
- `src-tauri/src/config.rs` — add fields with defaults, migration logic
- `src/types.ts` — add TypeScript types

**Implementation details:**
1. JSON Schema additions:
   ```json
   "model": {
     "properties": {
       "id": { "type": "string", "default": "nvidia/parakeet-tdt-0.6b-v2" },
       "family": { "type": "string", "enum": ["parakeet", "whisper"], "default": "parakeet" },
       "language": { "type": ["string", "null"], "default": null }
     }
   }
   ```
2. Rust `Config` struct:
   ```rust
   #[derive(Deserialize, Serialize)]
   pub struct ModelConfig {
       pub id: String,
       #[serde(default = "default_family")]
       pub family: ModelFamily,
       #[serde(default)]
       pub language: Option<String>,
   }
   fn default_family() -> ModelFamily { ModelFamily::Parakeet }
   ```
3. Migration: when loading a config file that lacks `model.family` or `model.language`, `serde` defaults fill them. Schema version stays 1.
4. TypeScript:
   ```ts
   interface ModelConfig {
     id: string;
     family: "parakeet" | "whisper";
     language: string | null;
   }
   ```

**Acceptance criteria:**
- [ ] Old config files (without new fields) load without error
- [ ] New fields have correct defaults
- [ ] Schema version remains 1
- [ ] TypeScript types match Rust types match JSON Schema

**Tests to write:**
- `src-tauri/src/tests/config_test.rs` — load old config, verify defaults
- `src/__tests__/types.test.ts` — type check (compile-time)

---

#### Task 4.4: Model Settings UI
**Complexity:** M
**Depends on:** Tasks 4.1, 4.3
**Files to modify:**
- `src/components/Settings/ModelSettings.tsx` (or create if not exists)

**Implementation details:**
1. Load model manifest from `shared/model/MODEL_MANIFEST.json` (bundled as static asset or fetched via Tauri command).
2. Two-step selection:
   - **Step 1:** Model family cards. Two cards side-by-side:
     - "Parakeet (English Only)" — size, speed, quality badges
     - "Whisper (Multilingual)" — size, speed, quality badges
   - Selecting Whisper reveals Step 2.
   - **Step 2:** Language dropdown (searchable `<select>` with type-ahead). Populated from `manifest.languages` for the selected Whisper variant.
3. Within Whisper family, allow selecting variant (base/small/medium) via radio buttons with size/quality tradeoff info.
4. Download status: show "Downloaded" / "Not Downloaded (290 MB)" / "Downloading 45%..." per model.
5. "Currently loaded" badge on the active model.

**Acceptance criteria:**
- [ ] Family selection shows/hides language dropdown
- [ ] Language dropdown populated correctly from manifest
- [ ] Model download triggers correctly
- [ ] Download progress is shown
- [ ] Currently loaded model is indicated
- [ ] Switching model triggers `asr.initialize` with correct params

**Tests to write:** `src/__tests__/components/Settings/ModelSettings.test.tsx` — family toggle, language visibility, download trigger mock.

---

### Phase 5: First-Run Onboarding

**Goal:** Guide new users to first transcription in < 2 minutes.
**Risk:** Low.
**Depends on:** Phase 1 (tab layout) for consistent UI, and Phase 4 (model selection) for the model step to be meaningful.

---

#### Task 5.1: Onboarding Wizard Component
**Complexity:** M
**Depends on:** Phase 1 (Task 1.1), Phase 4 (Tasks 4.3, 4.4) for model step
**Files to create:**
- `src/components/Onboarding/OnboardingWizard.tsx`
- `src/components/Onboarding/steps/WelcomeStep.tsx`
- `src/components/Onboarding/steps/MicrophoneStep.tsx`
- `src/components/Onboarding/steps/ModelStep.tsx`
- `src/components/Onboarding/steps/HotkeyStep.tsx`

**Implementation details:**
1. `OnboardingWizard`: manages step index, renders current step, provides next/back/skip.
   ```tsx
   const steps = [WelcomeStep, MicrophoneStep, ModelStep, HotkeyStep];
   const [currentStep, setCurrentStep] = useState(0);
   ```
2. **WelcomeStep:** App icon, "Voice Input Tool" title, 3 bullet points (local processing, configurable hotkey, text replacement). "Get Started" button.
3. **MicrophoneStep:** Reuse `MicrophoneTest` component. Device selector dropdown + live level meter. "My mic works" button to proceed.
4. **ModelStep:** Reuse `ModelSettings` component (from Task 4.4). Shows family + variant selection. Triggers download. Progress bar. "Download" or "Continue" (if already downloaded).
5. **HotkeyStep:** Show current hotkey in large text. "Customize" button opens hotkey recorder (reuse existing). Mode explanation (PTT vs PTS/S). "Finish Setup" button.
6. Final: brief "You're all set!" with hotkey reminder, auto-transitions to main UI after 2 seconds.
7. Progress dots: `<div>` with 4 dots, filled up to current step.
8. Config: `config.ui.onboarding_completed: boolean` (default: `false`).

**Acceptance criteria:**
- [ ] Wizard renders 4 steps with forward/back navigation
- [ ] Skip button on each step advances without requiring completion
- [ ] Mic test works within wizard context
- [ ] Model download can be initiated from wizard
- [ ] On completion, `onboarding_completed` is set to `true`
- [ ] Wizard does not appear on subsequent launches

**Tests to write:** `src/__tests__/components/Onboarding/OnboardingWizard.test.tsx` — step navigation, skip, completion flag.

---

#### Task 5.2: Onboarding Trigger
**Complexity:** S
**Depends on:** Task 5.1
**Files to modify:**
- `src/App.tsx`
- `src/store/appStore.ts` (if needed for onboarding state)
- Config schema (already added `onboarding_completed` in Task 4.3 or here)

**Implementation details:**
1. In `App.tsx`:
   ```tsx
   const onboardingCompleted = useAppStore(s => s.config.ui.onboarding_completed);
   if (!onboardingCompleted) return <OnboardingWizard onComplete={markOnboardingComplete} />;
   return <MainApp />;
   ```
2. Add "Reset Onboarding" button in Settings → UI section:
   ```tsx
   <button onClick={() => updateConfig({ ui: { onboarding_completed: false } })}>
     Reset Onboarding
   </button>
   ```

**Acceptance criteria:**
- [ ] Fresh install shows onboarding
- [ ] After completion, main UI shows
- [ ] Reset button re-triggers onboarding on next launch (or immediately)

**Tests to write:** `src/__tests__/App.test.tsx` — conditional rendering based on `onboarding_completed`.

---

### Phase 6: Dark/Light Theme

**Goal:** System theme detection + manual override.
**Risk:** Low.
**Parallelism:** Tasks 6.1 and 6.3 can run together. Task 6.2 is the bulk work (all components).

---

#### Task 6.1: Theme Infrastructure
**Complexity:** S
**Depends on:** None
**Files to modify:**
- `tailwind.config.js` — add `darkMode: 'class'`
- `src/index.css` — add CSS custom properties for theme colors (optional, Tailwind dark: variants may suffice)
- `src/store/appStore.ts` — add theme slice
- Config schema — add `config.ui.theme: "system" | "light" | "dark"` (default: `"system"`)

**Implementation details:**
1. Theme resolution hook:
   ```tsx
   function useTheme() {
     const themeSetting = useAppStore(s => s.config.ui.theme);
     const systemDark = useMediaQuery('(prefers-color-scheme: dark)');
     const isDark = themeSetting === 'dark' || (themeSetting === 'system' && systemDark);
     useEffect(() => {
       document.documentElement.classList.toggle('dark', isDark);
     }, [isDark]);
     return { isDark, themeSetting };
   }
   ```
2. Call `useTheme()` in `App.tsx` (or a top-level provider).
3. `useMediaQuery` is a simple custom hook wrapping `window.matchMedia`.

**Acceptance criteria:**
- [ ] `dark` class toggles on `<html>` based on config + system preference
- [ ] System theme changes are detected in real-time
- [ ] Config field persists across restarts

**Tests to write:** `src/__tests__/hooks/useTheme.test.ts` — system detection mock, config override.

---

#### Task 6.2: Component Dark Mode Styles
**Complexity:** L (many files, low per-file complexity)
**Depends on:** Task 6.1
**Files to modify:**
- All component files in `src/components/`

**Implementation details:**
1. Systematic pass through every component:
   - Backgrounds: `bg-white` → `bg-white dark:bg-gray-900`
   - Text: `text-gray-900` → `text-gray-900 dark:text-gray-100`
   - Borders: `border-gray-200` → `border-gray-200 dark:border-gray-700`
   - Inputs: `bg-gray-50` → `bg-gray-50 dark:bg-gray-800`
   - Buttons: add dark hover/active states
   - Cards/panels: `bg-gray-100` → `bg-gray-100 dark:bg-gray-800`
2. Overlay window: already has dark transparent background — minimal changes.
3. Test visually on both themes.

**Acceptance criteria:**
- [ ] All components are readable and aesthetically consistent in dark mode
- [ ] No white-on-white or black-on-black text
- [ ] Interactive elements (buttons, inputs, links) have visible focus/hover states in both themes
- [ ] Overlay window looks correct in both themes

**Tests to write:** Snapshot tests for key components in both themes (optional — visual regression testing is ideal but heavyweight). At minimum, verify no Tailwind classes are missing `dark:` counterparts via grep/lint.

---

#### Task 6.3: Theme Toggle in Settings
**Complexity:** S
**Depends on:** Task 6.1
**Files to modify:**
- Settings UI (within the UI section added in Task 1.3)

**Implementation details:**
1. Three-option segmented control or radio group:
   - System (default) — follows OS
   - Light — always light
   - Dark — always dark
2. Selection writes to `config.ui.theme` and triggers immediate visual update.

**Acceptance criteria:**
- [ ] Toggle renders three options with current selection highlighted
- [ ] Changing selection immediately updates the theme
- [ ] Selection persists to config

**Tests to write:** `src/__tests__/components/Settings/ThemeToggle.test.tsx` — selection change fires config update.

---

### Phase 7: CI/CD for Windows & macOS

**Goal:** Cross-platform CI. Runs continuously alongside other phases.
**Risk:** Medium (platform-specific failures).
**Parallelism:** Can start from day 1 and run throughout.

---

#### Task 7.1: Expand CI Matrix
**Complexity:** M
**Depends on:** None
**Files to modify:**
- `.github/workflows/test.yml`
- `.github/workflows/build.yml`

**Implementation details:**
1. Test workflow:
   ```yaml
   strategy:
     matrix:
       os: [ubuntu-latest, macos-latest, windows-latest]
   steps:
     - uses: actions/checkout@v4
     - uses: actions/setup-node@v4
     - uses: dtolnay/rust-toolchain@stable
     - name: Install Linux deps
       if: runner.os == 'Linux'
       run: sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev
     - name: Cache Cargo
       uses: actions/cache@v4
       with:
         path: |
           ~/.cargo/registry
           src-tauri/target
         key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
     - name: Cache node_modules
       uses: actions/cache@v4
       with:
         path: node_modules
         key: ${{ runner.os }}-node-${{ hashFiles('package-lock.json') }}
     - run: npm ci
     - run: npm test
     - run: cd src-tauri && cargo test
   ```
2. Build workflow: similar matrix but only `macos-latest` and `windows-latest` for release artifacts.

**Acceptance criteria:**
- [ ] Tests run on all three platforms
- [ ] Builds succeed on macOS and Windows
- [ ] Caching reduces CI time by >50% on cache hits
- [ ] CI failure on any platform blocks merges

**Tests to write:** N/A — the CI config IS the test.

---

#### Task 7.2: Platform-Specific Test Fixes
**Complexity:** M (unpredictable scope)
**Depends on:** Task 7.1
**Files to modify:** Various — depends on failures discovered

**Implementation details:**
1. Run full test suite on CI matrix and collect failures.
2. Common fix patterns:
   - Path separators: use `std::path::MAIN_SEPARATOR` or `Path::new()` instead of hardcoded `/`
   - File permissions: Windows doesn't support Unix permissions — gate tests with `#[cfg(unix)]`
   - Audio API: mock `rodio` on CI (no audio device available)
   - Temp dirs: use `tempfile::TempDir` instead of hardcoded paths
3. Add conditional compilation for truly platform-specific code:
   ```rust
   #[cfg(target_os = "windows")]
   fn platform_specific_thing() { /* Windows impl */ }

   #[cfg(target_os = "macos")]
   fn platform_specific_thing() { /* macOS impl */ }
   ```

**Acceptance criteria:**
- [ ] All tests pass on all three platforms
- [ ] Platform-specific tests are properly gated
- [ ] No test is `#[ignore]`d without a tracking issue

---

## Critical Path & Dependencies

```
Phase 1.1 (Tab Bar)  ─────────┬──→  Phase 1.2 (Status Dashboard)
                               ├──→  Phase 1.3 (Settings Reorg)
                               ├──→  Phase 1.4 (History)
                               └──→  Phase 1.5 (Replacements)
                                        │
Phase 4.1 (Manifest)  ──┬──→  Phase 4.2 (Sidecar Whisper)
                         │
Phase 4.3 (Config)  ────┴──→  Phase 4.4 (Model UI)
                                        │
Phase 1.3 + 4.3 + 4.4  ────→  Phase 5.1 (Onboarding Wizard)
                                        │
Phase 5.1  ─────────────────→  Phase 5.2 (Onboarding Trigger)

Phase 2.1 (Overlay Tauri)  ──→  Phase 2.2 (Overlay UI)
Phase 2.3 (Tray Icon)  ──────→  Phase 2.4 (Tray Menu)

Phase 3.1 (Audio Assets)  ───→  Phase 3.2 (Audio Playback) ──→  Phase 3.3 (State Integration)

Phase 6.1 (Theme Infra)  ──┬──→  Phase 6.2 (Dark Mode Styles)
                            └──→  Phase 6.3 (Theme Toggle)

Phase 7.1 (CI Matrix)  ──────→  Phase 7.2 (Platform Fixes)
```

**Minimum viable demo order:**
1. Phase 1.1 → 1.2, 1.3 (tabbed UI with status + settings)
2. Phase 3 (audio cues — quick win, feels polished)
3. Phase 2.1 → 2.2 (recording overlay — visual wow factor)
4. Phase 6 (dark mode — polish)
5. Phase 4 (multilingual — major feature)
6. Phase 5 (onboarding — polish for new users)
7. Phase 7 (CI — ongoing)

---

## Parallel Execution Plan for Multiple Agents

| Agent | Track | Tasks |
|-------|-------|-------|
| Agent A | UI/Frontend | 1.1 → 1.2 → 1.4 → 5.1 → 5.2 |
| Agent B | UI/Frontend | 1.3 → 1.5 → 6.1 → 6.2 → 6.3 |
| Agent C | Rust/Backend | 2.1 → 2.3 → 2.4 → 3.2 → 3.3 |
| Agent D | Rust/Backend | 3.1 → 4.3 → (config done) → 2.2 (overlay UI) |
| Agent E | Sidecar/Python | 4.1 → 4.2 |
| Agent F | UI/Frontend | 4.4 (after 4.1+4.3 done) |
| Agent G | CI/DevOps | 7.1 → 7.2 (ongoing) |

**Conflict zones (file reservations needed):**
- `src/App.tsx` — touched by Tasks 1.1, 5.2. Agent A owns it.
- `src-tauri/tauri.conf.json` — touched by Tasks 2.1, 3.1. Agent C owns it.
- `src-tauri/src/lib.rs` — touched by Tasks 2.1, 3.2. Agent C owns it.
- `shared/schema/AppConfig.schema.json` — touched by Tasks 4.3, 5.1, 6.1. Agent D owns it, others wait.
- `src/store/appStore.ts` — touched by Tasks 1.1, 6.1. Agent A merges first.

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Tauri multi-window transparency fails on Windows | Medium | High (Phase 2 blocked) | Early spike in Task 2.1. Fallback: non-transparent overlay with solid dark background. |
| `rodio` doesn't work on CI (no audio device) | High | Low (tests affected) | Mock audio output in tests. Only integration-test audio on real machines. |
| `faster-whisper` binary wheel unavailable for target platform | Medium | High (Phase 4 blocked) | Fall back to `openai-whisper` (pure Python, slower). Or use ONNX Whisper. |
| Config migration breaks on edge cases | Low | Medium | Comprehensive `serde(default)` on all new fields. Test with configs from every prior version. |
| Dark mode misses components, looks broken | Low | Low | Lint for Tailwind classes missing `dark:` counterparts. Manual visual QA checklist. |
| CI matrix doubles CI costs/time | Medium | Low | Aggressive caching. Run expensive builds only on PR merge, not every push. |
| Onboarding wizard blocks experienced users | Low | Low | Skip button on every step. "Don't show again" persisted in config. |

---

## Config Schema Additions Summary

All new fields are additive with defaults. `schema_version` remains `1`.

```json
{
  "model": {
    "id": "nvidia/parakeet-tdt-0.6b-v2",
    "family": "parakeet",
    "language": null
  },
  "audio": {
    "audio_cues_enabled": true
  },
  "ui": {
    "theme": "system",
    "onboarding_completed": false
  }
}
```

---

## IPC Protocol Addendum

**No breaking changes.** One additive change:

| Method | Change | Backward Compatible |
|--------|--------|---------------------|
| `asr.initialize` | New optional param `language: string \| null` | Yes — omission defaults to `null` (auto/English) |

---

## Testing Strategy Summary

| Layer | Tool | What's tested |
|-------|------|---------------|
| Rust unit | `cargo test` | State logic, config migration, tray icon mapping, audio cue selection, menu building |
| Frontend unit | `vitest` | Component rendering, store logic, tab navigation, search filtering, theme hook |
| Frontend integration | `vitest` + Testing Library | Onboarding flow, settings interactions, model selection flow |
| Sidecar unit | `pytest` | ASR engine factory, Whisper pipeline (mocked), model cache |
| E2E (manual) | Checklist | Full recording flow with audio cues, overlay, tray, cross-platform |
| CI | GitHub Actions | All above, on 3 platforms |
