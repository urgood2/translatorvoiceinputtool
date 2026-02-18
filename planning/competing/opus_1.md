

# Implementation Plan: Voice Input Tool — Usability & Polish Release

## Executive Summary (TL;DR)

Transform the Voice Input Tool from a functional prototype into a polished cross-platform desktop app across 7 phases. The work decomposes into **~25 discrete tasks** spanning UI reorganization, recording overlay, audio feedback, multilingual Whisper support, onboarding, theming, and CI hardening. Phases 1–3 and 5–6 are low-risk frontend/Rust work; Phase 4 (Whisper) is the highest-risk item touching the sidecar ASR boundary; Phase 7 is CI plumbing. Most phases are parallelizable across agents, with Phase 4 as the critical path due to cross-boundary protocol changes.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│  Tauri Host (Rust)                                      │
│  ┌──────────┐ ┌──────────┐ ┌────────────┐ ┌──────────┐│
│  │ Recording │ │ AudioCue │ │ TrayMgr    │ │ Overlay  ││
│  │ Controller│ │ Player   │ │ (tray.rs)  │ │ Window   ││
│  └────┬─────┘ └────┬─────┘ └─────┬──────┘ └────┬─────┘│
│       │             │             │              │       │
│  ┌────▼─────────────▼─────────────▼──────────────▼─────┐│
│  │              AppStateManager                        ││
│  │  (state machine: Idle→Recording→Transcribing→...)   ││
│  └───────────────────┬─────────────────────────────────┘│
│                      │ IPC_PROTOCOL_V1 (locked)          │
│  ┌───────────────────▼─────────────────────────────────┐│
│  │              Sidecar (Python)                        ││
│  │  ┌──────────┐  ┌──────────┐  ┌──────────────────┐  ││
│  │  │ Parakeet │  │ Whisper  │  │ model_cache.py   │  ││
│  │  │ Pipeline │  │ Pipeline │  │ (HF download)    │  ││
│  │  └──────────┘  └──────────┘  └──────────────────┘  ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│  React Frontend (Vite + Tailwind)                       │
│  ┌────────┐ ┌──────────┐ ┌─────────┐ ┌──────────────┐ │
│  │ Main   │ │ Overlay  │ │ Zustand │ │ Onboarding   │ │
│  │ Window │ │ Window   │ │ Store   │ │ Wizard       │ │
│  │ (tabs) │ │ (pill)   │ │         │ │              │ │
│  └────────┘ └──────────┘ └─────────┘ └──────────────┘ │
└─────────────────────────────────────────────────────────┘
```

**Locked boundaries:**
- IPC Protocol V1 (`shared/ipc/IPC_PROTOCOL_V1.md`) — additive-only changes
- `AppState` enum — new states require migration plan
- Config `schema_version: 1` — new fields get defaults, existing fields unchanged
- Sidecar boundary — Python owns audio+ASR, Rust owns orchestration+UI+injection

---

## Phase Breakdown

### Phase 1: UI/UX Overhaul — Tab Layout & Polish

**Goal:** Reorganize monolithic settings panel into navigable tabbed interface.
**Risk:** Low — purely frontend, no backend changes.
**Parallelism:** Tasks 1.1 must complete first; then 1.2–1.5 can run in parallel.

---

#### Task 1.1: Tab Navigation Component [Size: S]

**Files to create:**
- `src/components/Layout/TabBar.tsx`
- `src/components/Layout/TabPanel.tsx`

**Files to modify:**
- `src/App.tsx` — replace single-panel render with `<TabBar>` + `<TabPanel>` layout
- `src/store/appStore.ts` — add `activeTab: 'status' | 'settings' | 'history' | 'replacements'` to Zustand store (ephemeral, not persisted to config)

**Implementation details:**
- `TabBar` renders horizontal tab buttons: **Status** | **Settings** | **History** | **Replacements**
- Each tab button has an inline SVG icon (no icon library — keep bundle small)
- Active tab gets a bottom border indicator with a 150ms CSS transition
- `TabPanel` is a simple container that renders only the active tab's content via conditional rendering (not CSS display:none — avoid mounting unused components)
- Keyboard navigation: Left/Right arrow keys move between tabs when tab bar is focused, per WAI-ARIA Tabs pattern (`role="tablist"`, `role="tab"`, `role="tabpanel"`, `aria-selected`)
- Tab bar is sticky at top of window

**Acceptance criteria:**
- [ ] Four tabs render with icons and labels
- [ ] Clicking a tab switches content; only one tab content mounted at a time
- [ ] Active tab state persists in Zustand across re-renders (but NOT across app restarts — ephemeral)
- [ ] Arrow key navigation works per ARIA tab pattern
- [ ] No visual regressions on existing settings content

**Tests:**
- `src/components/Layout/__tests__/TabBar.test.tsx` — tab switching, keyboard nav, active state rendering
- Integration: verify `appStore.activeTab` updates correctly

---

#### Task 1.2: Status Dashboard Tab [Size: M]

**Files to create:**
- `src/components/Status/StatusDashboard.tsx`

**Files to modify:**
- `src/components/StatusIndicator.tsx` — extract reusable status display logic; keep existing component as a compact variant

**Implementation details:**
- Large animated state indicator in center:
  - `Idle` → gray circle with subtle pulse
  - `Recording` → red circle with breathing animation
  - `Transcribing` → yellow circle with spinning dots
  - `Error` → red exclamation
  - `LoadingModel` → blue spinner
- Current mode badge below indicator: "Push-to-Talk" or "Push-to-Start/Stop" with small icon
- Hotkey hint: reads `config.hotkey` from store, renders as keyboard shortcut badge ("Hold `Ctrl+Shift+Space` to record")
- Last transcription preview card: text (truncated to 200 chars), timestamp (relative: "2 min ago"), confidence percentage badge
- Quick stats row: "12 transcriptions today · 4m 32s total audio" — computed from history store
- Model status badge: "Parakeet Ready" / "Downloading 45%" / "No model"
- Sidecar health: green/red dot from existing sidecar health check

**Data sources (all from existing Zustand store):**
- `appStore.appState` — current state
- `appStore.config.activation_mode` — mode badge
- `appStore.config.hotkey` — hotkey hint
- `appStore.history` — last transcription, daily stats
- `appStore.modelStatus` — model badge
- `appStore.sidecarHealth` — health indicator

**Acceptance criteria:**
- [ ] Renders correctly for each of the 5 `AppState` values
- [ ] Stats are computed from actual history data
- [ ] No errors when history is empty (first launch)
- [ ] Responsive: works at minimum window size (400px width)

**Tests:**
- Render with each `AppState` — verify correct icon/animation class
- Empty history — verify graceful fallback ("No transcriptions yet")
- Stats computation — verify count and duration math

---

#### Task 1.3: Settings Tab Reorganization [Size: M]

**Files to modify:**
- `src/components/Settings/SettingsPanel.tsx` — restructure into collapsible sections

**Implementation details:**
- Collapsible sections with `<details>`/`<summary>` (native HTML, accessible by default):
  1. **Audio** — microphone selector, audio cues toggle, input level meter
  2. **Hotkeys** — hotkey recorder, activation mode selector
  3. **Injection** — text injection method, clipboard fallback toggle
  4. **Model** — model selector, language (Phase 4 placeholder)
  5. **UI** — theme selector (Phase 6 placeholder), onboarding reset
- Activation mode selector as prominent radio group with description text:
  - "Push-to-Talk (Hold)" — "Hold the hotkey while speaking. Release to stop."
  - "Push-to-Start/Stop (Toggle)" — "Press once to start. Press again to stop."
- Each section header has a small inline SVG icon
- Better spacing: `space-y-4` between fields, `space-y-6` between sections
- Help text under each setting in `text-sm text-gray-500`
- All sections expanded by default on first render; collapsed state is ephemeral

**Acceptance criteria:**
- [ ] All existing settings are accessible (no settings lost in reorganization)
- [ ] Sections expand/collapse correctly
- [ ] Activation mode selector works and persists to config
- [ ] Visual hierarchy is clear: section headers > field labels > help text

**Tests:**
- Section expand/collapse interaction
- Mode switching persists to config store
- All existing settings still render and function

---

#### Task 1.4: History Tab Enhancement [Size: M]

**Files to modify:**
- `src/components/Settings/HistoryPanel.tsx` → **move to** `src/components/History/HistoryPanel.tsx`

**Files to create:**
- `src/components/History/HistoryEntry.tsx` — individual entry card
- `src/components/History/HistorySearch.tsx` — search input

**Implementation details:**
- Search bar at top: `<input>` with debounced (300ms) client-side filter on transcription text (case-insensitive `includes`)
- Entry cards with:
  - Full transcription text (expandable if > 3 lines)
  - Timestamp: relative ("3 min ago") with full datetime on hover tooltip
  - Duration badge: "4.2s"
  - Confidence badge: color-coded (green ≥90%, yellow ≥70%, red <70%)
  - Copy button (right side) — copies text to clipboard, shows "Copied!" toast for 2s
- "Clear All" button at bottom with confirmation dialog (`window.confirm` is fine — no need for custom modal)
- Empty state: centered message "No transcriptions yet. Press [hotkey] to get started."
- Update imports in `src/App.tsx` to point to new location

**Acceptance criteria:**
- [ ] Search filters entries in real-time with debounce
- [ ] Copy button copies correct text to clipboard
- [ ] Clear All shows confirmation before deleting
- [ ] Empty state renders when no history
- [ ] Moved file path works correctly (no broken imports)

**Tests:**
- Search filtering with multiple entries
- Copy action (mock clipboard API)
- Clear confirmation flow (confirm → clear, cancel → keep)
- Empty state rendering

---

#### Task 1.5: Replacements Tab Polish [Size: S]

**Files to modify:**
- `src/components/Replacements/ReplacementsList.tsx` (or equivalent existing component)
- `src/components/Replacements/PresetSelector.tsx` (if exists, else create)

**Implementation details:**
- Visual separation: "Your Rules" section header above user rules, "Presets" section header above presets
- Inline regex validation: when user types a regex pattern, validate with `try { new RegExp(pattern) } catch { showError }` — show red border + error message below input
- Preset cards: name, description, "Apply" button that copies preset rules into user rules
- Rule count badge: shown on the **Replacements** tab label in `TabBar` (e.g., "Replacements (12)")
- Tab badge requires lifting rule count to store or computing from store in TabBar

**Acceptance criteria:**
- [ ] User rules and presets are visually distinct
- [ ] Invalid regex shows inline error immediately
- [ ] Preset "Apply" adds rules without duplicating existing ones
- [ ] Tab badge shows correct count

**Tests:**
- Rule CRUD operations still work
- Regex validation: valid pattern → no error, invalid → error message
- Preset loading: applies rules, doesn't duplicate

---

### Phase 2: Recording Overlay & Tray Indicator

**Goal:** Visual feedback when recording is active.
**Risk:** Medium — new Tauri window, platform-specific always-on-top.
**Parallelism:** Tasks 2.1+2.2 are coupled (window config + UI). Task 2.3 and 2.4 are independent of each other but both depend on existing tray code.

**Dependencies:** None on Phase 1 (can run in parallel).

---

#### Task 2.1: Floating Overlay Window — Tauri Config [Size: M]

**Files to modify:**
- `src-tauri/tauri.conf.json` — add `recording-overlay` window configuration
- `src-tauri/src/lib.rs` — add window creation/management logic

**Implementation details:**

Add to `tauri.conf.json` windows array:
```json
{
  "label": "recording-overlay",
  "url": "/overlay.html",
  "width": 300,
  "height": 60,
  "resizable": false,
  "decorations": false,
  "transparent": true,
  "alwaysOnTop": true,
  "visible": false,
  "skipTaskbar": true,
  "center": true
}
```

In Rust (`lib.rs` or new `src-tauri/src/overlay.rs`):
- `show_overlay(app: &AppHandle)` — gets window by label, calls `.show()`, positions at top-center of primary monitor
- `hide_overlay(app: &AppHandle)` — calls `.hide()`
- Register `AppStateManager` listener: on `Recording` → `show_overlay()`, on any other state → `hide_overlay()`
- Click-through: use Tauri's `set_ignore_cursor_events(true)` (available in Tauri 2)
- Position calculation: get primary monitor dimensions, center horizontally, offset 40px from top

**Platform considerations:**
- Windows: `transparent: true` + `decorations: false` works natively
- macOS: may need `NSPanel` level for proper always-on-top behavior — Tauri 2 handles this via `alwaysOnTop`
- Test that overlay doesn't steal focus from the active application

**Acceptance criteria:**
- [ ] Overlay window exists in Tauri config but is hidden by default
- [ ] Overlay shows when state transitions to `Recording`
- [ ] Overlay hides when state transitions away from `Recording`
- [ ] Window is transparent, borderless, always-on-top, click-through
- [ ] Window doesn't steal focus
- [ ] Positioned at top-center of primary monitor

**Tests:**
- Unit test: state → show/hide mapping logic
- Manual test on Windows and macOS (CI can't easily test window behavior)

---

#### Task 2.2: Overlay React UI [Size: M]

**Files to create:**
- `index-overlay.html` — minimal HTML entry for overlay window
- `src/overlay/main.tsx` — React root for overlay
- `src/overlay/RecordingOverlay.tsx` — main overlay component
- `src/overlay/Waveform.tsx` — canvas-based mini waveform

**Files to modify:**
- `vite.config.ts` — add `overlay` entry point for multi-page build:
  ```ts
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        overlay: resolve(__dirname, 'index-overlay.html'),
      },
    },
  }
  ```

**Implementation details:**

`RecordingOverlay.tsx`:
- Pill-shaped container: `rounded-full bg-black/80 backdrop-blur-sm px-4 py-2 flex items-center gap-3`
- Red dot: `w-3 h-3 rounded-full bg-red-500` with CSS `@keyframes pulse` animation
- Elapsed timer: starts at `00:00`, updates every 100ms via `setInterval`, formats as `MM:SS`
- Mini waveform component (right side)
- Smooth fade-in: `opacity-0 → opacity-100` with 200ms CSS transition on mount
- Listen to Tauri events:
  - `state_changed` — if state ≠ Recording, trigger fade-out (the Rust side hides the window after a 200ms delay to allow animation)
  - `audio:level` — forward RMS level samples to Waveform

`Waveform.tsx`:
- `<canvas>` element, ~100px × 30px
- Maintains ring buffer of last 50 audio level samples (float 0.0–1.0)
- On each new sample: shift buffer, append new value, redraw
- Drawing: vertical bars, width 2px, gap 0px, height = `sample * canvasHeight`
- Color: white with 70% opacity
- Renders via `requestAnimationFrame` for smooth 60fps updates
- Graceful fallback: if no `audio:level` events arrive, show flat line

**Bundle size concern:** Overlay React root is separate from main — keep imports minimal (no Zustand, no full component library). Only import Tauri event listener and React.

**Acceptance criteria:**
- [ ] Overlay renders pill with red dot, timer, waveform
- [ ] Timer counts up accurately (±100ms)
- [ ] Waveform animates with incoming audio level data
- [ ] Separate Vite entry point builds correctly
- [ ] Overlay bundle is < 50KB gzipped

**Tests:**
- `Waveform.test.tsx` — renders with mock data array, canvas draws correct number of bars
- `RecordingOverlay.test.tsx` — timer starts and increments, responds to state events

---

#### Task 2.3: System Tray State Indicator [Size: S]

**Files to modify:**
- `src-tauri/src/tray.rs`

**Files to create (assets):**
- `src-tauri/icons/tray-idle.png` (32x32, default icon)
- `src-tauri/icons/tray-recording.png` (32x32, red variant)
- `src-tauri/icons/tray-transcribing.png` (32x32, yellow variant)
- `src-tauri/icons/tray-disabled.png` (32x32, gray variant)

**Implementation details:**
- Icon mapping function:
  ```rust
  fn tray_icon_for_state(state: &AppState) -> &'static [u8] {
      match state {
          AppState::Idle => include_bytes!("../icons/tray-idle.png"),
          AppState::Recording => include_bytes!("../icons/tray-recording.png"),
          AppState::Transcribing => include_bytes!("../icons/tray-transcribing.png"),
          AppState::Error(_) => include_bytes!("../icons/tray-idle.png"),
          AppState::LoadingModel => include_bytes!("../icons/tray-transcribing.png"),
      }
  }
  ```
- Subscribe to `AppStateManager` state changes; on change, call `tray.set_icon()`
- Tooltip update: `tray.set_tooltip(&format!("Voice Input - {}", state.display_name()))`
- Icon generation: create simple colored circle variants (can be done with a script or manually in any image editor)

**Acceptance criteria:**
- [ ] Tray icon changes on each state transition
- [ ] Tooltip text updates with current state name
- [ ] Icons are visually distinct at 32x32 and 16x16 (macOS retina)

**Tests:**
- Unit test: `tray_icon_for_state` returns correct bytes for each variant
- Unit test: tooltip string format

---

#### Task 2.4: Full Tray Context Menu [Size: M]

**Files to modify:**
- `src-tauri/src/tray.rs`

**Files to create (optional):**
- `src-tauri/src/tray_menu.rs` — if tray.rs gets too large, extract menu building

**Implementation details:**

Menu structure (rebuilt dynamically):
```
✓ Enabled                          (toggle, checkmark when enabled)
─────────────────────────
  Mode ►
    ● Push-to-Talk (Hold)          (radio, selected)
    ○ Push-to-Start/Stop (Toggle)
─────────────────────────
  Recent ►
    "Hello this is a test..."      (click → copy to clipboard)
    "Another transcription..."
    (empty: "No recent transcriptions")
─────────────────────────
  Microphone ►
    ✓ MacBook Pro Microphone       (radio, selected)
      External USB Mic
─────────────────────────
  Open Settings                    (brings main window to front)
  About Voice Input Tool           (shows version dialog)
  Quit
```

- `build_tray_menu(state: &AppState, config: &AppConfig, history: &[HistoryEntry], devices: &[AudioDevice]) -> SystemTrayMenu`
- Menu rebuild triggers: state change, new transcription added, device list refresh, config change
- "Recent" submenu: last 5 entries, text truncated to 50 chars + "..."
- Click handlers:
  - Enable/Disable → toggle `config.enabled`, emit config change event
  - Mode radio → update `config.activation_mode`, emit config change event
  - Recent item → copy full text to clipboard via `app.clipboard_manager()`
  - Microphone radio → update `config.audio.device_id`, emit config change
  - Open Settings → `main_window.show()`, `main_window.set_focus()`
  - About → Tauri dialog with version from `Cargo.toml`
  - Quit → `app.exit(0)`

**Platform note:** macOS menus have a practical limit of ~100 items. Our max is ~20, so no concern.

**Acceptance criteria:**
- [ ] All menu items render correctly
- [ ] Toggle/radio items show correct state
- [ ] Click handlers perform correct actions
- [ ] Menu rebuilds when state/config/history changes
- [ ] "Recent" items copy text to clipboard on click

**Tests:**
- `build_tray_menu` unit test: given specific state/config/history, verify menu structure
- Click handler unit tests for each action type

---

### Phase 3: Audio Feedback

**Goal:** Auditory confirmation for recording lifecycle events.
**Risk:** Low — straightforward audio playback via `rodio`.
**Parallelism:** All 3 tasks are sequential (3.1 → 3.2 → 3.3).
**Dependencies:** None on Phases 1–2 (can run in parallel).

---

#### Task 3.1: Audio Assets [Size: S]

**Files to create:**
- `src-tauri/assets/sounds/start.ogg` (< 50KB)
- `src-tauri/assets/sounds/stop.ogg` (< 50KB)
- `src-tauri/assets/sounds/cancel.ogg` (< 50KB)
- `src-tauri/assets/sounds/error.ogg` (< 50KB)

**Implementation details:**
- Source royalty-free audio cues or generate with a tone generator:
  - `start.ogg` — 440Hz sine, 100ms, quick fade-out (soft "bip")
  - `stop.ogg` — 880Hz sine, 100ms, quick fade-out (higher "bip")
  - `cancel.ogg` — 440Hz→220Hz sweep, 200ms (descending tone)
  - `error.ogg` — 220Hz square wave, 150ms, two pulses (gentle alert)
- OGG Vorbis format: best cross-platform support with `rodio`, small file size
- Total size budget: < 200KB for all 4 cues

**Acceptance criteria:**
- [ ] 4 OGG files exist in `src-tauri/assets/sounds/`
- [ ] Each file is < 50KB
- [ ] Files play correctly in any audio player
- [ ] Sounds are distinct and recognizable

---

#### Task 3.2: Audio Playback in Rust [Size: M]

**Files to create:**
- `src-tauri/src/audio_cue.rs`

**Files to modify:**
- `src-tauri/Cargo.toml` — add `rodio = "0.19"` dependency

**Implementation details:**
```rust
use rodio::{Decoder, OutputStream, Sink};
use std::io::Cursor;

pub enum Cue {
    Start,
    Stop,
    Cancel,
    Error,
}

pub struct AudioCuePlayer {
    start: &'static [u8],
    stop: &'static [u8],
    cancel: &'static [u8],
    error: &'static [u8],
}

impl AudioCuePlayer {
    pub fn new() -> Self {
        Self {
            start: include_bytes!("../assets/sounds/start.ogg"),
            stop: include_bytes!("../assets/sounds/stop.ogg"),
            cancel: include_bytes!("../assets/sounds/cancel.ogg"),
            error: include_bytes!("../assets/sounds/error.ogg"),
        }
    }

    pub fn play(&self, cue: Cue, enabled: bool) {
        if !enabled { return; }
        let bytes = match cue {
            Cue::Start => self.start,
            Cue::Stop => self.stop,
            Cue::Cancel => self.cancel,
            Cue::Error => self.error,
        };
        // Spawn thread to avoid blocking
        let bytes = bytes.to_vec();
        std::thread::spawn(move || {
            if let Ok((_stream, handle)) = OutputStream::try_default() {
                let sink = Sink::try_new(&handle).ok();
                if let Some(sink) = sink {
                    if let Ok(source) = Decoder::new(Cursor::new(bytes)) {
                        sink.append(source);
                        sink.sleep_until_end();
                    }
                }
            }
        });
    }
}
```

- `AudioCuePlayer` is created once at app init, stored in Tauri managed state
- `play()` is fire-and-forget: spawns a short-lived thread for each cue
- Plays on system default **output** device (NOT the recording input)
- Respects `config.audio.audio_cues_enabled` (checked at call site)
- Thread lifetime is ~100-200ms per cue; no cleanup needed

**Config addition:** Add `audio_cues_enabled: bool` to `config.audio` section (default: `true`). This is an additive config change, schema_version stays 1.

**Acceptance criteria:**
- [ ] `AudioCuePlayer` compiles and plays each cue variant
- [ ] Playback does not block the calling thread
- [ ] No audio plays when `enabled = false`
- [ ] Plays on default output device, not recording input
- [ ] `rodio` builds on Windows, macOS, and Linux

**Tests:**
- Unit test: cue selection logic (correct bytes for each variant)
- Unit test: `enabled = false` → no thread spawn (mock or verify)
- Build test: `cargo check` on all 3 platforms (CI)

---

#### Task 3.3: Integration with State Machine [Size: S]

**Files to modify:**
- `src-tauri/src/integration.rs` (or wherever `RecordingController` handles state transitions)
- `src-tauri/src/recording.rs` (if transition hooks live here)

**Implementation details:**
- **Critical timing:** Play `start` cue and wait for it to finish (~100ms) BEFORE activating the microphone, so the beep isn't captured in the recording
- State transition → cue mapping:
  ```
  Idle → Recording:         play(Start), then start mic
  Recording → Transcribing: play(Stop)
  Recording → Idle (cancel): play(Cancel)
  Any → Error:              play(Error)
  ```
- Access `AudioCuePlayer` from Tauri managed state in the transition handler
- Access `config.audio.audio_cues_enabled` from config state

**Timing implementation:**
```rust
// In recording start handler:
audio_cue_player.play_sync(Cue::Start, config.audio.audio_cues_enabled);
// play_sync blocks for ~100ms until cue finishes
// Then start recording...
```

Add `play_sync` variant to `AudioCuePlayer` that blocks until playback completes (for the start cue only). Other cues use fire-and-forget `play`.

**Acceptance criteria:**
- [ ] Start cue plays before mic activation (beep is NOT in the recording)
- [ ] Stop cue plays when recording ends
- [ ] Cancel cue plays on double-tap cancel
- [ ] Error cue plays on error state transition
- [ ] Disabling audio cues in config stops all cues

**Tests:**
- Integration test: mock `AudioCuePlayer`, verify correct cue is requested on each transition
- Verify start cue is synchronous (plays before mic activation)

---

### Phase 4: Language Selection & Whisper Support

**Goal:** Multilingual transcription via Whisper models.
**Risk:** HIGH — touches sidecar ASR, model manifest, config schema, and UI.
**Parallelism:** Task 4.1 and 4.3 can run in parallel (manifest + config). Task 4.2 depends on 4.1. Task 4.4 depends on 4.2+4.3.
**Dependencies:** None on Phases 1–3, but should be scheduled after Phase 1 (needs Settings UI ready for model selector).

---

#### Task 4.1: Expand Model Manifest [Size: S]

**Files to modify:**
- `shared/model/MODEL_MANIFEST.json`

**Implementation details:**

New manifest structure (additive — existing Parakeet entry stays):
```json
[
  {
    "id": "nvidia/parakeet-tdt-0.6b",
    "family": "parakeet",
    "name": "Parakeet TDT 0.6B (English)",
    "languages": ["en"],
    "size_mb": 600,
    "speed": "fast",
    "quality": "excellent",
    "description": "Best English-only model. Fast and accurate.",
    "hf_repo": "nvidia/parakeet-tdt_ctc-0.6b-release2",
    "hf_filename": "parakeet-tdt_ctc-0.6b-release2.nemo"
  },
  {
    "id": "openai/whisper-base",
    "family": "whisper",
    "name": "Whisper Base (Multilingual)",
    "languages": ["auto", "en", "es", "fr", "de", "it", "pt", "zh", "ja", "ko", "nl", "pl", "ru", "sv", "tr", "ar", "hi", "th", "vi", "uk"],
    "size_mb": 290,
    "speed": "fast",
    "quality": "good",
    "description": "Good balance of speed and accuracy. Supports 99 languages.",
    "hf_repo": "openai/whisper-base",
    "hf_filename": null
  },
  {
    "id": "openai/whisper-small",
    "family": "whisper",
    "name": "Whisper Small (Multilingual)",
    "languages": ["auto", "en", "es", "fr", "de", "it", "pt", "zh", "ja", "ko", "nl", "pl", "ru", "sv", "tr", "ar", "hi", "th", "vi", "uk"],
    "size_mb": 967,
    "speed": "medium",
    "quality": "very good",
    "description": "Better accuracy than Base. Recommended for non-English languages.",
    "hf_repo": "openai/whisper-small",
    "hf_filename": null
  },
  {
    "id": "openai/whisper-medium",
    "family": "whisper",
    "name": "Whisper Medium (Multilingual)",
    "languages": ["auto", "en", "es", "fr", "de", "it", "pt", "zh", "ja", "ko", "nl", "pl", "ru", "sv", "tr", "ar", "hi", "th", "vi", "uk"],
    "size_mb": 3060,
    "speed": "slow",
    "quality": "excellent",
    "description": "Highest accuracy. Requires more RAM and time.",
    "hf_repo": "openai/whisper-medium",
    "hf_filename": null
  }
]
```

- Add `family` field to existing Parakeet entry (must remain backward compatible — consuming code must handle missing `family` as `"parakeet"`)
- `languages` array uses ISO 639-1 codes. `"auto"` means auto-detect.
- `hf_filename: null` for Whisper means use the default HuggingFace model download (no specific file)

**Acceptance criteria:**
- [ ] Manifest is valid JSON
- [ ] Existing Parakeet entry unchanged except for new additive fields
- [ ] All Whisper variants have correct size estimates
- [ ] `languages` arrays are consistent across Whisper variants

**Tests:**
- JSON schema validation test
- Manifest parsing test in both Rust and Python

---

#### Task 4.2: Sidecar Whisper Support [Size: L]

**Files to modify:**
- `sidecar/asr/engine.py` (or equivalent ASR module)
- `sidecar/model_cache.py`
- `sidecar/pyproject.toml` — add `faster-whisper` as dependency

**Files to create:**
- `sidecar/asr/whisper_pipeline.py` — Whisper-specific ASR pipeline

**Implementation details:**

1. **Dependency:** Use `faster-whisper` (CTranslate2-based, ~4x faster than OpenAI whisper):
   ```toml
   [project.optional-dependencies]
   whisper = ["faster-whisper>=1.0.0"]
   ```

2. **ASR Engine dispatch** — modify `engine.py`:
   ```python
   class AsrEngine:
       def initialize(self, model_id: str, language: str | None = None):
           manifest_entry = load_manifest_entry(model_id)
           if manifest_entry["family"] == "parakeet":
               self.pipeline = ParakeetPipeline(model_id)
           elif manifest_entry["family"] == "whisper":
               self.pipeline = WhisperPipeline(model_id, language)
           else:
               raise ValueError(f"Unknown model family: {manifest_entry['family']}")
   ```

3. **WhisperPipeline** (`whisper_pipeline.py`):
   ```python
   from faster_whisper import WhisperModel

   class WhisperPipeline:
       def __init__(self, model_id: str, language: str | None):
           model_size = model_id.split("/")[-1]  # "whisper-base" → "base"
           model_size = model_size.replace("whisper-", "")
           self.model = WhisperModel(model_size, device="cpu", compute_type="int8")
           self.language = language if language != "auto" else None

       def transcribe(self, audio_path: str) -> TranscriptionResult:
           segments, info = self.model.transcribe(
               audio_path,
               language=self.language,
               beam_size=5,
               vad_filter=True,
           )
           text = " ".join(seg.text for seg in segments)
           return TranscriptionResult(
               text=text.strip(),
               confidence=info.language_probability if self.language is None else None,
               language=info.language,
           )
   ```

4. **IPC Protocol addendum** — additive change to `asr.initialize`:
   - New optional parameter: `language: string | null` (default: `null`)
   - When `null`: Parakeet uses English, Whisper uses auto-detect
   - This is backward-compatible: old callers that don't send `language` get default behavior

5. **Model cache** — `model_cache.py` modifications:
   - `faster-whisper` handles its own model download from HuggingFace
   - Cache directory: use same base cache dir, subfolder per model family
   - Download progress reporting: hook into `faster-whisper`'s download callback, emit progress events via IPC

**Failure modes:**
- `faster-whisper` not installed → clear error message: "Whisper support requires faster-whisper. Install with: pip install faster-whisper"
- Model download fails → retry logic with exponential backoff (3 attempts)
- Out of memory → catch `RuntimeError`, report to user with model size recommendation

**Acceptance criteria:**
- [ ] Whisper Base transcribes English audio correctly
- [ ] Whisper Base transcribes non-English audio (e.g., Spanish) correctly
- [ ] Language auto-detect works when `language=null`
- [ ] Explicit language selection works (e.g., `language="es"`)
- [ ] Parakeet pipeline unchanged and still works
- [ ] Model download shows progress via IPC
- [ ] Clear error if `faster-whisper` not installed

**Tests:**
- Unit test: `WhisperPipeline` with mock audio file
- Unit test: ASR engine dispatch (parakeet vs whisper based on model_id)
- Integration test: full pipeline with a short test audio file (English + non-English)
- Error handling: missing dependency, download failure, OOM

---

#### Task 4.3: Config Schema Update [Size: S]

**Files to modify:**
- `shared/schema/AppConfig.schema.json` — add new fields
- `src-tauri/src/config.rs` — Rust config struct + migration
- `src/types.ts` — TypeScript types

**Implementation details:**

New config fields (additive, defaults preserve existing behavior):
```json
{
  "model": {
    "model_id": "nvidia/parakeet-tdt-0.6b",
    "language": null,
    "family": "parakeet"
  }
}
```

- `model.language: string | null` — ISO 639-1 code or `null` for auto/default. Default: `null`
- `model.family: "parakeet" | "whisper"` — derived from model_id, but cached for quick access. Default: `"parakeet"`
- `schema_version` stays at `1` — these are additive fields with defaults

Rust migration in `config.rs`:
```rust
// When loading config, fill missing fields with defaults
if config.model.language.is_none() {
    config.model.language = None; // already None, but explicit
}
if config.model.family.is_empty() {
    config.model.family = "parakeet".to_string();
}
```

TypeScript types:
```typescript
interface ModelConfig {
  model_id: string;
  language: string | null;
  family: 'parakeet' | 'whisper';
}
```

**Acceptance criteria:**
- [ ] Old config files load without errors (missing fields get defaults)
- [ ] New config fields serialize/deserialize correctly
- [ ] Schema version unchanged
- [ ] TypeScript types match Rust types match JSON schema

**Tests:**
- Config migration: load v1 config without new fields → verify defaults applied
- Round-trip: write config with new fields, read back, verify equality

---

#### Task 4.4: Model Settings UI [Size: M]

**Files to modify:**
- `src/components/Settings/ModelSettings.tsx` (or create if not exists)

**Implementation details:**

Two-step selector UI:

**Step 1: Model Family Cards**
```
┌─────────────────────────┐  ┌─────────────────────────┐
│ 🇺🇸 Parakeet (English)  │  │ 🌍 Whisper (Multilingual)│
│                         │  │                         │
│ 600 MB · Fast · Best    │  │ 290 MB+ · Multiple sizes│
│ for English dictation   │  │ 99 languages supported  │
│                         │  │                         │
│ [● Selected]            │  │ [○ Select]              │
└─────────────────────────┘  └─────────────────────────┘
```

**Step 2: Whisper Sub-options (shown only when Whisper selected)**
- Model size selector: Base (290MB) / Small (967MB) / Medium (3GB) — radio cards with size/quality/speed badges
- Language dropdown: populated from manifest's `languages` array, with "Auto-detect" as first option
- Search/filter in dropdown for easy language finding

**Common elements:**
- Download status: "Downloaded ✓" / "Not downloaded (290 MB)" / "Downloading... 45%"
- "Download" button when model not cached
- "Currently loaded" green badge on the active model
- Storage usage: "Models: 890 MB used"

**Data flow:**
- Read model manifest from `shared/model/MODEL_MANIFEST.json` (loaded via Tauri command or bundled)
- Config writes: update `config.model.model_id`, `config.model.family`, `config.model.language`
- Trigger model download via existing sidecar IPC
- Listen to download progress events

**Acceptance criteria:**
- [ ] Family cards render with correct info from manifest
- [ ] Language dropdown only visible for Whisper family
- [ ] Model download triggers correctly and shows progress
- [ ] Config updates correctly on selection change
- [ ] "Currently loaded" indicator is accurate

**Tests:**
- Render with Parakeet selected → no language dropdown
- Render with Whisper selected → language dropdown visible
- Model download trigger with mock IPC
- Config update on selection change

---

### Phase 5: First-Run Onboarding

**Goal:** Guide new users through setup in < 2 minutes.
**Risk:** Low — new component, minimal backend changes.
**Dependencies:** Phase 4 recommended (model selector in onboarding), but can ship with Parakeet-only initially.

---

#### Task 5.1: Onboarding Wizard Component [Size: M]

**Files to create:**
- `src/components/Onboarding/OnboardingWizard.tsx` — wizard container
- `src/components/Onboarding/WelcomeStep.tsx`
- `src/components/Onboarding/MicrophoneStep.tsx`
- `src/components/Onboarding/ModelStep.tsx`
- `src/components/Onboarding/HotkeyStep.tsx`

**Implementation details:**

Wizard container:
- `currentStep` state: 0–3
- Progress dots at bottom (4 dots)
- "Next" / "Back" / "Skip" buttons
- Slide transition between steps (CSS `transform: translateX`)

**Step 1 — Welcome:**
- App logo/icon
- "Voice Input Tool" heading
- Brief description: "Transcribe your speech to text in any application. All processing happens locally on your device — your audio never leaves your computer."
- Privacy badge: "🔒 100% Local Processing"
- "Get Started" button

**Step 2 — Microphone:**
- Reuse existing `MicrophoneTest` component (or extract from Settings)
- Microphone device selector dropdown
- Live level meter showing mic input
- "Say something to test your microphone" prompt
- Visual feedback: green checkmark when audio detected above threshold

**Step 3 — Model:**
- If Phase 4 complete: show model family cards + language selector
- If Phase 4 not complete: show Parakeet card with "Download" button
- Download progress bar
- Skip option: "I'll download later" (app will prompt again on first use)

**Step 4 — Hotkey:**
- Show current hotkey binding in large key badge
- Activation mode selector (Push-to-Talk / Push-to-Start/Stop) with descriptions
- "Try it now!" prompt — user can press hotkey to test (if model downloaded)
- "Customize" button to change hotkey (reuse hotkey recorder from Settings)

**Completion screen:**
- "You're all set! 🎉"
- Summary: selected mic, model, hotkey, mode
- "Start Using Voice Input" button → sets `config.ui.onboarding_completed = true`, transitions to main UI

**Acceptance criteria:**
- [ ] 4-step wizard with forward/back navigation
- [ ] Skip button available on each step
- [ ] Progress dots show current step
- [ ] Each step's functionality works (mic test, model download, hotkey test)
- [ ] Completion sets config flag

**Tests:**
- Navigation: forward, back, skip
- Completion flow: verify config flag set
- Each step renders without errors

---

#### Task 5.2: Onboarding Trigger [Size: S]

**Files to modify:**
- `src/App.tsx` — conditional render
- `src/types.ts` — add `onboarding_completed` to config types
- `shared/schema/AppConfig.schema.json` — add field
- `src-tauri/src/config.rs` — add field with default `false`

**Implementation details:**
- On app launch, check `config.ui.onboarding_completed`
- If `false` (or missing — default): render `<OnboardingWizard />` instead of tab layout
- On wizard completion: update config, trigger re-render → main UI appears
- In Settings > UI section: "Reset Onboarding" button that sets `onboarding_completed = false` and requires app restart (or immediate wizard re-render)

**Config field:**
```json
{
  "ui": {
    "onboarding_completed": false
  }
}
```

**Acceptance criteria:**
- [ ] First launch shows onboarding wizard
- [ ] Subsequent launches show main UI
- [ ] "Reset Onboarding" in settings works
- [ ] Missing config field defaults to `false` (triggers onboarding)

**Tests:**
- `App.tsx` render: `onboarding_completed=false` → wizard, `true` → tabs
- Config default: old config without field → onboarding shown

---

### Phase 6: Dark/Light Theme

**Goal:** Respect system theme preference with manual override.
**Risk:** Low — Tailwind dark mode is well-established.
**Parallelism:** Task 6.1 first, then 6.2 and 6.3 in parallel.
**Dependencies:** Phase 1 (tab layout) should be complete for consistent theming.

---

#### Task 6.1: Theme Infrastructure [Size: S]

**Files to modify:**
- `tailwind.config.js` — add `darkMode: 'class'`
- `src/index.css` — add CSS custom properties for theme-aware colors (optional, Tailwind `dark:` may suffice)
- `src/store/appStore.ts` — add `resolvedTheme: 'light' | 'dark'` to store

**Files to create:**
- `src/hooks/useTheme.ts` — theme management hook

**Implementation details:**
```typescript
// src/hooks/useTheme.ts
export function useTheme() {
  const configTheme = useAppStore(s => s.config.ui.theme); // 'system' | 'light' | 'dark'
  const setResolvedTheme = useAppStore(s => s.setResolvedTheme);

  useEffect(() => {
    const resolve = () => {
      if (configTheme === 'system') {
        return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
      }
      return configTheme;
    };

    const resolved = resolve();
    setResolvedTheme(resolved);

    // Apply to <html>
    document.documentElement.classList.toggle('dark', resolved === 'dark');

    // Listen for system theme changes
    if (configTheme === 'system') {
      const mql = window.matchMedia('(prefers-color-scheme: dark)');
      const handler = () => {
        const newResolved = resolve();
        setResolvedTheme(newResolved);
        document.documentElement.classList.toggle('dark', newResolved === 'dark');
      };
      mql.addEventListener('change', handler);
      return () => mql.removeEventListener('change', handler);
    }
  }, [configTheme]);
}
```

Config field: `config.ui.theme: "system" | "light" | "dark"` (default: `"system"`)

**Acceptance criteria:**
- [ ] `darkMode: 'class'` in Tailwind config
- [ ] `dark` class applied to `<html>` based on resolved theme
- [ ] System preference changes detected in real-time
- [ ] Manual override works

**Tests:**
- `useTheme` hook: mock `matchMedia`, verify class toggle
- System → dark, system → light, manual dark, manual light

---

#### Task 6.2: Component Dark Mode Styles [Size: L]

**Files to modify:**
- All component files under `src/components/` — add `dark:` Tailwind variants

**Implementation details:**

Core color mapping:
| Element | Light | Dark |
|---------|-------|------|
| Background | `bg-white` | `dark:bg-gray-900` |
| Surface | `bg-gray-50` | `dark:bg-gray-800` |
| Card | `bg-white border-gray-200` | `dark:bg-gray-800 dark:border-gray-700` |
| Text primary | `text-gray-900` | `dark:text-gray-100` |
| Text secondary | `text-gray-500` | `dark:text-gray-400` |
| Input | `bg-white border-gray-300` | `dark:bg-gray-700 dark:border-gray-600` |
| Button primary | `bg-blue-600 text-white` | `dark:bg-blue-500` |
| Button secondary | `bg-gray-100` | `dark:bg-gray-700` |
| Divider | `border-gray-200` | `dark:border-gray-700` |

Components to update:
- `TabBar`, `TabPanel` — tab backgrounds, active indicator
- `StatusDashboard` — status colors, card backgrounds
- `SettingsPanel` — section headers, inputs, toggles
- `HistoryPanel`, `HistoryEntry` — entry cards, search input
- `Replacements` components — rule cards, input fields
- `OnboardingWizard` — step backgrounds, buttons
- All shared elements: buttons, inputs, dropdowns, tooltips

Overlay window: already dark-themed (transparent bg), minimal changes needed.

**Acceptance criteria:**
- [ ] All components have dark variants
- [ ] No white flash on dark theme load
- [ ] Text contrast meets WCAG AA (4.5:1 for normal text)
- [ ] Inputs, buttons, cards all have appropriate dark styles
- [ ] Overlay window looks correct in both themes

**Tests:**
- Visual regression: render each major component in light and dark, snapshot test
- Contrast check: computed styles meet minimum ratios

---

#### Task 6.3: Theme Toggle in Settings [Size: S]

**Files to modify:**
- Settings UI (part of `SettingsPanel.tsx`, in the UI section from Task 1.3)

**Implementation details:**
- Three-way segmented control: `[ System | Light | Dark ]`
- Active option highlighted with filled background
- Updates config immediately (no save button needed)
- Preview: theme changes instantly on selection
- Persist to `config.ui.theme`

**Acceptance criteria:**
- [ ] Three options render correctly
- [ ] Selection persists to config
- [ ] Theme changes immediately on click
- [ ] Current selection matches actual theme

**Tests:**
- Click each option → verify config update
- Verify immediate visual change

---

### Phase 7: CI/CD for Windows & macOS

**Goal:** Cross-platform build and test coverage.
**Risk:** Medium — platform-specific CI issues.
**Dependencies:** All previous phases should be feature-complete before CI hardening.

---

#### Task 7.1: Expand CI Matrix [Size: M]

**Files to modify:**
- `.github/workflows/test.yml`
- `.github/workflows/build.yml`

**Implementation details:**

Test workflow:
```yaml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
  fail-fast: false

steps:
  - uses: actions/checkout@v4

  # Platform-specific deps
  - name: Install Linux deps
    if: runner.os == 'Linux'
    run: sudo apt-get update && sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev libasound2-dev

  - name: Install Rust
    uses: dtolnay/rust-toolchain@stable

  - name: Rust cache
    uses: Swatinem/rust-cache@v2
    with:
      workspaces: "src-tauri -> target"

  - name: Node setup
    uses: actions/setup-node@v4
    with:
      node-version: 20
      cache: 'npm'

  - run: npm ci
  - run: npm run test          # Vitest (frontend)
  - run: npm run lint
  - run: cd src-tauri && cargo test  # Rust tests
```

Build workflow (release builds):
```yaml
strategy:
  matrix:
    include:
      - os: macos-latest
        target: universal-apple-darwin
      - os: windows-latest
        target: x86_64-pc-windows-msvc
  fail-fast: false

steps:
  # ... setup steps ...
  - run: npm run tauri build
  - uses: actions/upload-artifact@v4
    with:
      name: release-${{ matrix.os }}
      path: |
        src-tauri/target/release/bundle/**/*.dmg
        src-tauri/target/release/bundle/**/*.msi
```

Caching strategy:
- Cargo registry + target dir: `Swatinem/rust-cache@v2`
- node_modules: `actions/setup-node` built-in cache
- Python sidecar deps: `actions/setup-python` + pip cache

**Acceptance criteria:**
- [ ] Tests run on all 3 platforms
- [ ] Release builds produce artifacts on macOS and Windows
- [ ] Caching works (second run is significantly faster)
- [ ] Matrix failures don't block other platforms (`fail-fast: false`)

**Tests:**
- CI workflow itself is the test — verify green runs on all platforms

---

#### Task 7.2: Platform-Specific Test Fixes [Size: M]

**Files to modify:** Various — depends on failures discovered.

**Common issues to anticipate and fix:**

1. **Path separators:** `\` vs `/` in test assertions
   - Fix: use `path.join()` or normalize paths in assertions

2. **Audio API stubs:** `rodio`/`cpal` may fail to initialize in CI (no audio device)
   - Fix: gate audio tests with `#[cfg(not(ci))]` or mock audio backend
   - Add `AUDIO_TESTS_ENABLED` env var check

3. **File permissions:** Unix permissions don't exist on Windows
   - Fix: conditional assertions

4. **Line endings:** `\r\n` vs `\n` in text comparison
   - Fix: normalize line endings in test helpers

5. **Process spawning:** sidecar process handling differs across platforms
   - Fix: platform-specific test helpers

6. **Hotkey registration:** global hotkey APIs differ per platform
   - Fix: mock in tests, only test registration logic

**Acceptance criteria:**
- [ ] All existing tests pass on all 3 CI platforms
- [ ] No flaky tests (run matrix 3 times to verify)
- [ ] Platform-specific test skips are documented with `// SKIP: <reason>`

---

## Critical Path & Dependencies

```
                    Phase 1 (UI Tabs)
                    ┌──────────────┐
                    │ T1.1 TabBar  │
                    └──────┬───────┘
            ┌──────────────┼──────────────┬──────────────┐
            ▼              ▼              ▼              ▼
      T1.2 Status   T1.3 Settings  T1.4 History  T1.5 Replacements
            │              │              │              │
            └──────────────┴──────────────┴──────────────┘
                                │
                    ┌───────────▼────────────┐
                    │    Phase 6 (Theming)   │
                    │ T6.1 → T6.2 + T6.3    │
                    └────────────────────────┘

    Phase 2 (Overlay)                Phase 3 (Audio)          Phase 4 (Whisper)
    ┌────────────────┐               ┌──────────────┐         ┌──────────────┐
    │ T2.1 Window    │               │ T3.1 Assets  │         │ T4.1 Manifest│
    │ T2.2 UI       ├─(parallel)─   │ T3.2 Player  │         │ T4.3 Config  │──(parallel)
    │ T2.3 Tray Icon│               │ T3.3 Integrate│         └──────┬───────┘
    │ T2.4 Tray Menu│               └──────────────┘                │
    └────────────────┘                                         T4.2 Sidecar
                                                                    │
                                                               T4.4 Model UI
                                                                    │
                                                         Phase 5 (Onboarding)
                                                         ┌──────────────────┐
                                                         │ T5.1 Wizard      │
                                                         │ T5.2 Trigger     │
                                                         └──────────────────┘

                              ┌──────────────────────────┐
                              │    Phase 7 (CI/CD)       │
                              │ T7.1 Matrix + T7.2 Fixes │
                              │ (after all features done) │
                              └──────────────────────────┘
```

**Critical path:** Phase 4 (Whisper) is the longest sequential chain and highest risk item:
`T4.1 Manifest → T4.2 Sidecar Whisper → T4.4 Model UI → T5.1 Onboarding (model step)`

**Parallel execution strategy for multiple agents:**

| Agent | Phase/Tasks | Dependencies |
|-------|------------|--------------|
| Agent A | Phase 1 (all tasks) then Phase 6 | None initially |
| Agent B | Phase 2 (overlay + tray) | None |
| Agent C | Phase 3 (audio feedback) | None |
| Agent D | Phase 4 (Whisper) then Phase 5 | T4.4 needs Phase 1 Settings UI |
| Agent E | Phase 7 (CI) — after all features | All phases complete |

**File reservation strategy:**
- Agent A: `src/components/**`, `src/App.tsx`, `src/store/`, `tailwind.config.js`
- Agent B: `src-tauri/tauri.conf.json`, `src-tauri/src/tray.rs`, `src/overlay/**`, `vite.config.ts`
- Agent C: `src-tauri/src/audio_cue.rs`, `src-tauri/assets/sounds/`, `src-tauri/Cargo.toml` (shared — coordinate)
- Agent D: `sidecar/**`, `shared/model/`, `shared/schema/`, `src/components/Settings/ModelSettings.tsx`
- Shared files needing coordination: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src/types.ts`, `shared/schema/AppConfig.schema.json`

---

## Risk Mitigation

### High Risk: Phase 4 — Sidecar Whisper Integration

| Risk | Impact | Mitigation |
|------|--------|------------|
| `faster-whisper` dependency conflicts with existing sidecar deps | Blocks all Whisper work | Pin versions early; test in isolated virtualenv; use optional dependency group |
| Whisper model download is slow/unreliable | Poor UX, user frustration | Implement retry with exponential backoff; cache aggressively; show clear progress + cancel button |
| Whisper transcription quality varies by language | User expectations mismatch | Document quality expectations per language; recommend "Small" for non-English; show confidence scores |
| IPC protocol change breaks backward compatibility | Violates guardrail #1 | New `language` param is optional with `null` default — existing callers unaffected |
| Memory usage with larger Whisper models | Crashes on low-RAM machines | Show RAM requirements in UI; warn when selecting Medium model on < 8GB RAM systems |

### Medium Risk: Phase 2 — Multi-Window Overlay

| Risk | Impact | Mitigation |
|------|--------|------------|
| Always-on-top doesn't work consistently on macOS | Overlay hidden behind other apps | Test with multiple macOS versions; use Tauri's native window level API; fallback: tray icon only |
| Click-through doesn't work on all platforms | Overlay blocks user input | Feature-flag overlay; test early on both platforms; provide "disable overlay" setting |
| Separate Vite entry point increases build complexity | CI failures, dev confusion | Test multi-entry build in CI early; document in README |

### Low Risk: Phase 1, 3, 5, 6

| Risk | Impact | Mitigation |
|------|--------|------------|
| Tab layout breaks existing settings | Settings inaccessible | Incremental migration: move one panel at a time, keep old code until verified |
| `rodio` audio playback issues on some systems | No audio cues | Make audio cues optional (config toggle); graceful failure (log warning, continue) |
| Theme flash on load | Poor UX | Apply theme class in `<script>` tag in `index.html` head (before React mounts) |

### Cross-Cutting Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Multiple agents editing shared files | Merge conflicts, broken code | File reservations via agent mail; shared files (`Cargo.toml`, `types.ts`) edited by one agent at a time |
| Config schema changes across phases | Incompatible defaults | All config changes are additive with explicit defaults; test migration from base config in each phase |
| Bundle size growth | Slower downloads | Budget: main bundle < 500KB gzip, overlay < 50KB gzip; monitor with `vite-bundle-visualizer` |

---

## Estimated Effort Summary

| Phase | Tasks | Complexity | Est. Total |
|-------|-------|------------|------------|
| Phase 1: UI/UX Tabs | 5 | S+M+M+M+S | Medium |
| Phase 2: Overlay & Tray | 4 | M+M+S+M | Medium-Large |
| Phase 3: Audio Feedback | 3 | S+M+S | Small-Medium |
| Phase 4: Whisper | 4 | S+L+S+M | Large |
| Phase 5: Onboarding | 2 | M+S | Small-Medium |
| Phase 6: Dark/Light | 3 | S+L+S | Medium |
| Phase 7: CI/CD | 2 | M+M | Medium |

**Total: 23 tasks across 7 phases.**

With 4 agents running in parallel on Phases 1–4, followed by sequential work on Phases 5–7, the critical path runs through Phase 4 (Whisper sidecar integration).
