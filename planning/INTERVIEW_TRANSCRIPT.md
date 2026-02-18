# Interview Transcript

**Project:** Voice Input Tool (translator-voice-input-tool)
**Date:** 2026-02-13
**Interviewer:** GreenHollow (Claude Opus 4.6)
**Stakeholder:** Project Owner

---

## Brownfield Baseline

Before starting interviews, the interviewer reviewed `planning/BROWNFIELD_PREFLIGHT.md` and performed a comprehensive codebase exploration. Key findings:

- **Architecture:** Tauri 2 (Rust) + React 18 + Zustand + Python sidecar via JSON-RPC 2.0 over NDJSON
- **State:** Working E2E flow: hotkey → audio capture → ASR transcription → text replacement → clipboard injection
- **Maturity:** Core functionality is implemented. UI is functional but minimal. Platform support exists in code but is undertested.
- **Existing modes:** Hold (push-to-talk) and Toggle (push-to-start/stop) already exist in `hotkey.rs`
- **Existing tray:** Basic tray icon exists in `tray.rs` but has minimal menu
- **Existing config:** Full AppConfig with audio, hotkeys, injection, replacements, UI, presets sections
- **Existing model:** nvidia/parakeet-tdt-0.6b-v2 (English only)
- **Locked protocol:** IPC_PROTOCOL_V1.md is marked LOCKED — any sidecar protocol changes must be additive

---

## Round 1 — Core Scope & Platform Priorities

### Q1: Platform Testing Strategy

> For Windows and Mac testing — the app already has cross-platform support in code. Do you mean you want CI/CD pipelines that actually build and run tests on both platforms, or is this more about manual QA and fixing platform-specific bugs?

**Answer: CI + manual QA**
Set up CI pipelines for both platforms AND fix any platform-specific bugs found during testing.

**Implications:**
- Need GitHub Actions workflows with `macos-latest` and `windows-latest` runners
- Existing `test.yml` / `build.yml` workflows need platform matrix expansion
- Manual QA pass needed for platform-specific features: hotkeys, text injection, tray, audio devices
- Windows-specific: WebView2, WASAPI audio, Win32 hotkey API
- macOS-specific: CoreAudio, Accessibility permissions, code signing

---

### Q2: Recording Indicator Style

> For the recording indicator — what style of popup do you envision?

**Answer: Both (floating overlay + system tray pulse)**
Maximum visibility — floating overlay AND system tray indicator working together.

**Implications:**
- New Tauri window: transparent, always-on-top, click-through, positioned at screen edge
- Tray icon swap: need recording-state tray icon variants (idle, recording, transcribing)
- Tauri 2 supports multiple windows — overlay will be a second window
- Audio level data already flows via `event.audio_level` notifications — reuse for waveform
- Overlay must work on all platforms (Windows always-on-top, macOS NSWindow level, X11/Wayland layer-shell)

---

### Q3: Activation Modes

> The app already supports 'hold' (push-to-talk) and 'toggle' (push-to-start/stop) hotkey modes. Are you asking to make these more discoverable in the UI, or do you want additional modes?

**Answer: Just triple mode**
Clearly label: Push-to-Talk (hold), Push-to-Start/Stop (toggle), and make them easy to switch.

**Implications:**
- Existing `ActivationMode` enum in Rust has `Hold` and `Toggle` — no code changes to core logic
- UI needs a prominent mode selector (radio group or segmented control) in settings
- Third mode label is cosmetic: "Push-to-Talk" = Hold, "Push-to-Start/Stop" = Toggle, but presented with clear descriptions
- Consider adding mode indicator to the status bar so users always know which mode they're in

---

### Q4: Industry Standard Features

> When you say 'industry standard features' — which of these are highest priority?

**Answer (multi-select): Audio feedback, Language selection, Quick actions tray**

#### Audio Feedback
- Sound effects for recording start/stop/cancel/error
- Existing `audio_cues_enabled` config field suggests this was planned but not implemented
- Need embedded audio assets (small WAV/OGG files) bundled with the app
- Play via system audio (not through the recording device)

#### Language Selection
- Current model (Parakeet) is English-only
- Need multilingual model support (Whisper)
- Sidecar already has `asr.initialize` with `model_id` param — protocol supports model switching

#### Quick Actions Tray
- Current tray is minimal
- Need full context menu with state-aware items

---

## Round 2 — Implementation Details & UX Polish

### Q5: Floating Overlay Design

> For the floating recording overlay — should it include a live waveform/audio level visualization, or keep it minimal?

**Answer: With waveform**
Pill includes a small live audio waveform visualization so user sees their voice being captured.

**Implications:**
- Small Canvas or SVG-based waveform renderer in the overlay window
- Audio level data already streams via `audio:level` events (RMS + peak values)
- Overlay window needs its own lightweight React root (separate from main window)
- Waveform should be performant — requestAnimationFrame with circular buffer of ~50 recent samples
- Pill design: `[🔴 Recording 00:03 ~~~~~ ]` with pulsing dot, timer, and mini waveform

---

### Q6: Language & Model Strategy

> For language selection — the current model is English-only. How should we handle multilingual support?

**Answer: Both options**
Keep Parakeet as English option, add Whisper as multilingual option. User picks model family, then language.

**Implications:**
- `MODEL_MANIFEST.json` needs expansion: multiple model entries with metadata (languages, size, speed)
- UI: two-step selection — (1) pick model family, (2) pick language (if multilingual)
- Sidecar `asr.initialize` already accepts `model_id` — protocol change is minimal
- Need to handle different model sizes (Whisper tiny/base/small/medium/large)
- Download progress already works — reuse for new models
- Config needs `model.language` field added to `AppConfig`
- Sidecar `model_cache.py` needs HuggingFace Hub support for Whisper models

---

### Q7: UI/UX Overhaul Direction

> The current UI is a single-window settings panel. What kind of UX overhaul do you want?

**Answer: Tab-based layout**
Organize into clear tabs: Status, Settings, History, Replacements. Add icons and polish.

**Implications:**
- New tab navigation component (horizontal tab bar at top or sidebar)
- Tabs: **Status** (main dashboard), **Settings** (audio, hotkeys, injection, model), **History** (transcription log), **Replacements** (rules + presets)
- Status tab becomes the "home" — shows current state, last transcription, quick stats
- Settings tab consolidates existing settings panels into sections
- Icons for each tab (can use inline SVG or a small icon set)
- Polish: better spacing, consistent typography, subtle animations, loading states

---

### Q8: System Tray Menu Scope

> For the system tray quick actions — what should be in the menu?

**Answer: Full control**
Toggle enabled, mode switcher, last 5 transcriptions, mic selector, Settings, About, Quit.

**Implications:**
- Tray menu needs dynamic items (recent transcriptions, current mic, current mode)
- Tauri 2 tray API supports dynamic menu building
- Items: Enable/Disable toggle, Mode (Hold/Toggle submenu), Recent (5 items, click to copy), Microphone (submenu with device list), separator, Settings (opens main window), About, Quit
- Menu needs to rebuild when state changes (new transcription, device change, etc.)

---

## Interviewer Additions (Recommended)

Based on the codebase analysis and industry standards, the interviewer recommends these additional features:

### 1. First-Run Onboarding Flow
**Why:** Without onboarding, users hit a blank settings panel and don't know where to start. They need to: pick a mic, download a model (which takes time), set a hotkey, and test it works — in that order.
**Scope:** Stepped wizard (4 screens) that only appears on first launch. Stores `onboarding_completed` in config.

### 2. Dark/Light Theme Support
**Why:** Desktop apps in 2026 are expected to respect system theme. The app uses Tailwind which makes theming straightforward.
**Scope:** Detect system preference via `prefers-color-scheme`, add manual toggle in settings, persist in config.

### 3. History Search & Filtering
**Why:** With regular use, history grows quickly. Users need to find past transcriptions.
**Scope:** Search bar in History tab, filter by date range, sort options.

### 4. Transcription Confidence Display
**Why:** ASR models output confidence scores but the UI doesn't show them. Low-confidence words could be highlighted so users know what to double-check.
**Scope:** Show confidence in history entries. Optionally highlight low-confidence words in the overlay.

### 5. Keyboard Navigation & Accessibility
**Why:** A voice input tool should be especially accessible. Users who rely on voice input may also have mobility challenges.
**Scope:** Full keyboard navigation, ARIA labels, screen reader support, focus indicators.

---

## Summary of Agreed Scope

| Feature | Priority | Complexity | New Files |
|---------|----------|-----------|-----------|
| CI/CD for Win + Mac | High | Medium | `.github/workflows/` updates |
| Manual QA + platform fixes | High | Variable | Bug-dependent |
| Floating recording overlay | High | High | New Tauri window + React components |
| System tray indicator | High | Low | `tray.rs` updates |
| Audio feedback sounds | High | Medium | Audio assets + playback code |
| Language selection + Whisper | High | High | Model manifest, UI, sidecar updates |
| Tab-based UI layout | High | Medium | New layout components |
| Full tray context menu | Medium | Medium | `tray.rs` rewrite |
| Triple mode labels in UI | Medium | Low | Settings UI update |
| First-run onboarding | Medium | Medium | New wizard component |
| Dark/light theme | Low | Low | Tailwind config + store |
| History search | Low | Low | History tab enhancement |
| Confidence display | Low | Low | UI + type updates |
| Accessibility | Low | Medium | Cross-cutting |
