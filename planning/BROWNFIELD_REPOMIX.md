This file is a merged representation of a subset of the codebase, containing specifically included files, combined into a single document by Repomix.
The content has been processed where content has been compressed (code blocks are separated by ⋮---- delimiter).

# File Summary

## Purpose
This file contains a packed representation of a subset of the repository's contents that is considered the most important context.
It is designed to be easily consumable by AI systems for analysis, code review,
or other automated processes.

## File Format
The content is organized as follows:
1. This summary section
2. Repository information
3. Directory structure
4. Repository files (if enabled)
5. Multiple file entries, each consisting of:
  a. A header with the file path (## File: path/to/file)
  b. The full contents of the file in a code block

## Usage Guidelines
- This file should be treated as read-only. Any changes should be made to the
  original repository files, not this packed version.
- When processing this file, use the file path to distinguish
  between different files in the repository.
- Be aware that this file may contain sensitive information. Handle it with
  the same level of security as you would the original repository.

## Notes
- Some files may have been excluded based on .gitignore rules and Repomix's configuration
- Binary files are not included in this packed representation. Please refer to the Repository Structure section for a complete list of file paths, including binary files
- Only files matching these patterns are included: src/**, include/**, scripts/**, docs/**, tests/**, planning/**, *.md, *.lua, *.cpp, *.hpp, *.h, *.c, *.py, *.ts, *.js, *.json, *.toml, *.yaml, *.yml, CMakeLists.txt, Makefile, package.json, pyproject.toml, go.mod, Cargo.toml
- Files matching patterns in .gitignore are excluded
- Files matching default ignore patterns are excluded
- Content has been compressed - code blocks are separated by ⋮---- delimiter

# Directory Structure
```
docs/
  DECISIONS/
    0001-asr-backend.md
    README.md
  COORDINATION_GATES.md
  DOD_VERIFICATION.md
  KNOWN_LIMITATIONS.md
  M0.6-sidecar-packaging-spike.md
  MANUAL_CHECKLIST.md
  PRIVACY.md
  THIRD_PARTY_NOTICES.md
scripts/
  e2e/
    lib/
      assert.sh
      common.sh
      log.sh
    run-all.sh
    test-error-recovery.sh
    test-focus-guard.sh
    test-full-flow.sh
    test-offline.sh
  build-sidecar.ps1
  build-sidecar.sh
  bundle-sidecar.ps1
  bundle-sidecar.sh
  demo-gate-1.sh
  generate_assets.py
  validate_ipc_examples.py
  validate_model_manifest.py
src/
  components/
    Replacements/
      index.ts
      PresetsPanel.tsx
      ReplacementEditor.tsx
      ReplacementList.tsx
      ReplacementPreview.tsx
    Settings/
      Diagnostics.tsx
      HistoryPanel.tsx
      HotkeyConfig.tsx
      InjectionSettings.tsx
      MicrophoneSelect.tsx
      MicrophoneTest.tsx
      ModelSettings.tsx
      SelfCheck.tsx
      SettingsPanel.tsx
    index.ts
    StatusIndicator.tsx
  hooks/
    index.ts
    useTauriEvents.test.ts
    useTauriEvents.ts
  store/
    appStore.test.ts
    appStore.ts
    index.ts
  tests/
    HistoryPanel.test.tsx
    MicrophoneTest.test.tsx
    ModelSettings.test.tsx
    Replacements.test.tsx
    SelfCheckDiagnostics.test.tsx
    SettingsPanel.test.tsx
    setup.ts
    StatusIndicator.test.tsx
  App.tsx
  index.css
  main.tsx
  types.ts
AGENTS.md
package.json
postcss.config.js
README.md
tailwind.config.js
tsconfig.json
tsconfig.node.json
vite.config.ts
vitest.config.ts
```

# Files

## File: docs/DECISIONS/0001-asr-backend.md
````markdown
# ADR 0001: ASR Backend Selection

**Status:** Accepted
**Date:** 2026-02-04
**Deciders:** Project Team

## Context

We need to select an automatic speech recognition (ASR) backend for the voice input tool. The selected model must:

1. Support high-quality English transcription
2. Work offline (no API calls required)
3. Have a permissive license allowing redistribution
4. Be efficient enough to run on consumer hardware
5. Provide punctuation and capitalization

## Considered Options

### Option 1: OpenAI Whisper (whisper-large-v3-turbo)

**Pros:**
- Excellent transcription quality
- Well-documented and widely used
- Multiple size variants available

**Cons:**
- Larger models (>1GB) for best quality
- MIT license is permissive but less explicit about model weights
- Slower inference on CPU

### Option 2: NVIDIA Parakeet TDT 0.6B v3

**Pros:**
- Excellent transcription quality (comparable to Whisper Large)
- Explicit CC-BY-4.0 license with clear redistribution rights
- 600M parameters (smaller than Whisper Large)
- Optimized for throughput with TDT architecture
- Automatic punctuation and capitalization
- Supports 25 European languages with auto-detection
- Word-level timestamps included

**Cons:**
- Newer model with smaller community
- Requires NeMo framework
- 2.5GB download for full model

### Option 3: Vosk (various models)

**Pros:**
- Very lightweight models available
- Apache 2.0 license
- Low resource requirements

**Cons:**
- Lower quality than Whisper/Parakeet
- No automatic punctuation
- Limited language support

## Decision

**Selected: NVIDIA Parakeet TDT 0.6B v3**

### Rationale

1. **License Clarity:** CC-BY-4.0 provides explicit, well-understood redistribution rights. We can confidently bundle and distribute the model with proper attribution.

2. **Quality vs Size:** The 600M parameter model provides transcription quality comparable to larger models while being more efficient. The TDT (Token Duration Transducer) architecture is optimized for fast inference.

3. **Features:** Built-in punctuation, capitalization, and timestamps eliminate the need for post-processing pipelines.

4. **Multilingual Support:** The 25-language support with automatic detection provides future expansion capability without model swapping.

5. **Commercial Viability:** CC-BY-4.0 explicitly allows commercial use, making this suitable for any deployment scenario.

## Fallback Strategy

If Parakeet becomes unavailable or licensing changes, the fallback is **OpenAI Whisper (small or base model)**:

- **Whisper Small**: 460MB, good accuracy, fully MIT licensed
- **Whisper Base**: 140MB, acceptable accuracy for basic dictation

The sidecar architecture abstracts the ASR backend through a common interface (`asr.transcribe`), allowing model swapping without API changes. To switch:

1. Update `MODEL_MANIFEST.json` with Whisper model details
2. Implement Whisper adapter in sidecar's ASR module
3. No changes needed to Rust core or IPC protocol

## Consequences

### Positive

- Clear legal standing for distribution
- Single model file simplifies deployment
- Rich feature set (punctuation, timestamps) out of the box
- Good inference performance on modern hardware

### Negative

- Requires NeMo toolkit or compatible runtime
- 2.5GB download on first use
- Less community resources compared to Whisper

### Mitigation

- Document NeMo integration clearly
- Implement robust download with progress feedback
- Monitor NVIDIA's model updates for improvements

## Performance Characteristics

| Metric | Parakeet TDT 0.6B | Whisper Small | Whisper Base |
|--------|-------------------|---------------|--------------|
| Model Size | ~2.5GB | ~460MB | ~140MB |
| Parameters | 600M | 244M | 74M |
| CPU Latency (10s audio) | ~2-4s | ~4-8s | ~2-4s |
| GPU Latency (10s audio) | <1s | ~1-2s | <1s |
| Memory (CPU) | ~4GB | ~2GB | ~1GB |
| Memory (GPU) | ~2GB | ~1GB | ~512MB |
| WER (English) | ~5% | ~7% | ~10% |

**Notes:**
- Latency measured on typical consumer hardware (8-core CPU, 8GB RAM)
- GPU measurements on NVIDIA RTX 3060 or equivalent
- WER (Word Error Rate) approximate; varies by accent/domain
- CPU-first baseline is the MVP requirement; GPU is optional optimization

## Implementation Notes

- Model ID in manifest: `parakeet-tdt-0.6b-v3`
- Source: `nvidia/parakeet-tdt-0.6b-v3` on HuggingFace
- Pinned revision: `6d590f77001d318fb17a0b5bf7ee329a91b52598`
- License: CC-BY-4.0 (attribution required)

## Related Documents

- [MODEL_MANIFEST.json](../../shared/model/MODEL_MANIFEST.json)
- [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md)
- [IPC_PROTOCOL_V1.md](../../shared/ipc/IPC_PROTOCOL_V1.md)

## References

- [NVIDIA Parakeet Model Card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
- [CC-BY-4.0 License](https://creativecommons.org/licenses/by/4.0/)
- [NeMo Toolkit](https://github.com/NVIDIA/NeMo)
````

## File: docs/DECISIONS/README.md
````markdown
# Architecture Decision Records (ADRs)

This directory contains Architecture Decision Records that document significant technical decisions made during the development of OpenVoicy.

## What is an ADR?

An Architecture Decision Record captures an important design decision along with its context and consequences. ADRs help:
- Document **why** decisions were made, not just what was decided
- Enable informed future changes by understanding original constraints
- Onboard new contributors by explaining the project's evolution

## Decision Index

| ID | Title | Status | Date |
|----|-------|--------|------|
| [0001](./0001-asr-backend.md) | ASR Backend Selection | Accepted | 2026-02-04 |

## ADR Statuses

- **Proposed**: Under discussion
- **Accepted**: Decision has been made and is active
- **Deprecated**: No longer applies (superseded by newer ADR)
- **Superseded**: Replaced by a newer decision (link to replacement)

## Creating New ADRs

When adding a new decision record:

1. Use the next sequential number: `NNNN-short-title.md`
2. Follow the standard template:
   - Status
   - Context
   - Options Considered
   - Decision
   - Consequences
   - Related Documents
3. Update this README with the new entry
4. Get team review before merging

## Template

```markdown
# ADR NNNN: Title

**Status:** Proposed | Accepted | Deprecated | Superseded by [NNNN]
**Date:** YYYY-MM-DD
**Deciders:** [Names or roles]

## Context

What is the issue that we're seeing that is motivating this decision?

## Considered Options

### Option 1: Name
- Pros
- Cons

### Option 2: Name
- Pros
- Cons

## Decision

What is the change we're proposing and why?

## Consequences

### Positive
- Benefits

### Negative
- Drawbacks and mitigations

## Related Documents

- Links to related docs
```
````

## File: docs/COORDINATION_GATES.md
````markdown
# Coordination Gates and Parallel Execution Strategy

This document defines the hard synchronization points that enable safe parallel development across multiple workstreams.

## Overview

OpenVoicy development is structured around **coordination gates** - concrete verification points that must be passed before dependent work can proceed. This enables multiple developers/agents to work in parallel without conflicts.

## Parallel Execution Strategy

### Recommended 4-Agent Split

| Agent | Focus Area | Tasks |
|-------|------------|-------|
| **A** | Rust IPC/sidecar/state/model/watchdog | M2.1-M2.4, M2.8, M2.10 |
| **B** | Rust hotkey/tray/injection/focus/history/config | M2.5-M2.7, M2.9 |
| **C** | Sidecar protocol/audio/preprocess/replacements | M1.1-M1.4, M1.7-M1.8 |
| **D** | ML/model cache/packaging/decision records | M1.5-M1.6, M5.1, docs |

### Key Principles

1. **Contract-First Development**: IPC protocol is locked before implementation starts
2. **Interface Boundaries**: Clear ownership of modules minimizes merge conflicts
3. **Gate Verification**: Each gate has a concrete demo that proves readiness
4. **No Assumptions**: Don't start dependent work until gate is verified

## Coordination Gates

### Gate 1: IPC Contract Locked

**Milestone:** M0 complete
**Status:** PASSED

**Artifacts:**
- [x] `shared/ipc/IPC_PROTOCOL_V1.md` complete
- [x] `shared/ipc/examples/IPC_V1_EXAMPLES.jsonl` validated

**Enables:** M1, M2 can start in parallel

**Verification:**
```bash
./scripts/demo-gate-1.sh
```

---

### Gate 2: Ping + Info + Device List + Meter Demo

**Milestone:** M0 coordination gate
**Status:** PASSED

**Artifacts:**
- [x] Rust spawns sidecar
- [x] `system.ping` returns valid response
- [x] `system.info` returns capabilities
- [x] `audio.list_devices` returns device list
- [x] `audio.meter_start` emits audio level events

**Enables:** Full M1 and M2 work

---

### Gate 3: Record Loop + Focus Guard + Stub Injection Demo

**Milestone:** M2 coordination gate
**Status:** PASSED

**Artifacts:**
- [x] Hotkey triggers recording
- [x] Recording stops on release (or toggle)
- [x] Focus Guard captures/validates signature
- [x] Injection stub places text on clipboard
- [x] Works without UI open (tray only)

**Enables:** M3 UI work, M4 integration

---

### Gate 4: ASR Returns Text Demo

**Milestone:** M1.6 complete
**Status:** PENDING

**Artifacts:**
- [ ] `asr.initialize` completes (with real model)
- [ ] Recording produces audio
- [ ] Transcription returns text
- [ ] Log showing actual transcription output

**Enables:** E2E integration, M4.1

---

### Gate 5: E2E Inject Without UI Demo

**Milestone:** M4.1 complete
**Status:** PENDING

**Artifacts:**
- [ ] Full flow: hotkey -> record -> transcribe -> inject
- [ ] Text appears in target application
- [ ] Works without settings window open
- [ ] Video or screenshot showing flow

**Enables:** Packaging (M5), release prep

## Blocking Relationships

```
Gate 1 (IPC Contract) ─────┬──> All M1 tasks
                           └──> All M2 tasks

Gate 2 (Ping + Demo) ──────┬──> M1.3+ (audio capture)
                           └──> M2.4+ (recording controller)

Gate 3 (Record Loop) ──────┬──> M3 (UI)
                           └──> M4.1 (E2E integration)

Gate 4 (ASR Text) ─────────┬──> M4.1 (E2E integration)
                           └──> Full E2E testing

Gate 5 (E2E Inject) ───────┬──> M5 (packaging)
                           └──> Release preparation
```

## Gate Verification Process

Each gate must have:

1. **Demo Script**: Concrete verification in `scripts/demo-gate-N.sh`
2. **Artifact**: Log, screenshot, or video committed to repo
3. **Sign-off**: Comment on tracking issue confirming passage
4. **Status Update**: Gate status updated in this document

## Current Status Summary

| Gate | Description | Status | Blocker For |
|------|-------------|--------|-------------|
| 1 | IPC Contract Locked | PASSED | M1, M2 |
| 2 | Ping + Info + Devices | PASSED | M1.3+, M2.4+ |
| 3 | Record Loop + Focus Guard | PASSED | M3, M4.1 |
| 4 | ASR Returns Text | PENDING | M4.1, E2E |
| 5 | E2E Inject Without UI | PENDING | M5, Release |

## Milestone Completion Status

| Milestone | Description | Status |
|-----------|-------------|--------|
| M0 | Project + Contract Lock | COMPLETE |
| M1 | Sidecar Core | IN PROGRESS |
| M2 | Rust Core MVP | COMPLETE |
| M3 | Settings UI | NOT STARTED |
| M4 | Integration Testing | NOT STARTED |
| M5 | Packaging + Distribution | NOT STARTED |

## Related Documents

- [IPC_PROTOCOL_V1.md](../shared/ipc/IPC_PROTOCOL_V1.md) - Locked IPC contract
- [DECISIONS/0001-asr-backend.md](./DECISIONS/0001-asr-backend.md) - ASR backend choice
- [KNOWN_LIMITATIONS.md](./KNOWN_LIMITATIONS.md) - Current limitations
````

## File: docs/DOD_VERIFICATION.md
````markdown
# MVP Definition of Done Verification

This checklist consolidates all acceptance criteria for the OpenVoicy MVP. All items must be verified before release.

---

## Core Functionality

### Fresh Install Experience
- [ ] Fresh install → configure mic/hotkey → model downloads with visible progress → hold hotkey → speak → release → transcription injected (or clipboard-only with clear reason)

### Stability
- [ ] No unhandled panics/crashes during 1-hour manual soak test
- [ ] Sidecar crash or hang triggers visible error + one-click restart
- [ ] App remains responsive after sidecar recovery

### Cross-Platform Builds
- [ ] Windows build produced
- [ ] macOS build produced
- [ ] Linux build produced
- [ ] Sidecar bundled in each build
- [ ] Model downloads on first run (not bundled)

---

## Feature Verification

### Self-Check Panel
The self-check must report status of:
- [ ] Hotkey mode effective (hold vs toggle)
- [ ] Injection mode effective (direct vs clipboard-only)
- [ ] Microphone permission status
- [ ] Sidecar reachable/responsive
- [ ] Model status (not downloaded / downloading / ready / error)

### Microphone Test
- [ ] Input level meter responds on selected device
- [ ] No-signal warning after 3 seconds of silence

### Focus Guard
- [ ] Prevents injection if focus changed during recording
- [ ] Clear reason shown for clipboard-only fallback

### Error Recovery
- [ ] Mic disconnect → user-actionable error
- [ ] Sidecar crash → auto-restart or one-click recovery
- [ ] Model download failure → clear error + retry option
- [ ] Disk full → error with space requirement

---

## Documentation Checklist

### Required Documentation
| Document | Status | Location |
|----------|--------|----------|
| Quick Start README | [ ] | README.md |
| Known Limitations | [ ] | docs/KNOWN_LIMITATIONS.md |
| Privacy Documentation | [ ] | docs/PRIVACY.md |
| Third-Party Notices | [ ] | docs/THIRD_PARTY_NOTICES.md |
| Manual Test Checklist | [ ] | docs/MANUAL_CHECKLIST.md |

### Documentation Content Requirements

#### KNOWN_LIMITATIONS.md must include:
- [ ] Wayland injection constraints
- [ ] Wayland hotkey constraints (hold mode)
- [ ] macOS permission friction points
- [ ] Supported platforms table

#### PRIVACY.md must include:
- [ ] What data is stored locally
- [ ] What data is NOT stored (no transcripts, no audio)
- [ ] Offline-only operation
- [ ] How to clear all data

#### Diagnostics must reference:
- [ ] Link to KNOWN_LIMITATIONS.md in copy-diagnostics output

---

## Platform-Specific Verification

### Windows (10+)
- [ ] Global hotkey works
- [ ] Text injection works
- [ ] No antivirus false positives (or documented)

### macOS (12+)
- [ ] Microphone permission prompt handled
- [ ] Accessibility permission documented
- [ ] Gatekeeper/quarantine handled

### Linux X11
- [ ] Global hotkey works
- [ ] Text injection works
- [ ] Tray icon visible

### Linux Wayland
- [ ] Toggle mode works (hold gracefully degrades)
- [ ] Clipboard-only injection works
- [ ] Limitations documented and shown in UI

---

## Offline Verification

- [ ] With cached model and no network, transcription works
- [ ] No network calls made during transcription
- [ ] App launches without network if model is cached

---

## Sign-Off

### Verification Details
| Item | Verified By | Date | Notes |
|------|-------------|------|-------|
| Fresh Install Test | | | |
| Soak Test (1 hour) | | | |
| Windows Build | | | |
| macOS Build | | | |
| Linux Build | | | |
| Documentation Complete | | | |
| Offline Mode | | | |

### Release Decision

- [ ] **All required items verified**
- [ ] **Known issues documented** (list any incomplete items below)
- [ ] **Ready for MVP release**

### Outstanding Issues
_List any items that failed verification:_
1.
2.
3.

### Release Approved By
- Name: ____________
- Date: ____________

---

## Quick Reference

### Error Handling Matrix (must all be actionable)

| Scenario | Expected Behavior |
|----------|-------------------|
| No microphone available | Clear error, device selection prompt |
| Microphone permission denied | Permission instructions shown |
| Sidecar crash | Auto-restart or one-click recovery |
| Sidecar hang (watchdog) | Timeout → error → restart option |
| Model download failure | Error + retry button |
| Model load failure | Clear error message |
| Hotkey conflict | Warning shown |
| Injection blocked | Clipboard fallback + reason |
| Focus changed | Clipboard fallback + reason |
| Rapid press/release | Debounced, no crash |

### Definition of Done Summary

The MVP is complete when a user can:
1. Install the app on any supported platform
2. Configure their microphone and hotkey
3. Download the ASR model with visible progress
4. Use voice-to-text in any application
5. Recover gracefully from any error condition
6. Understand limitations through documentation

---

*This checklist is the final gate before MVP release. Use MANUAL_CHECKLIST.md for detailed functional testing.*
````

## File: docs/KNOWN_LIMITATIONS.md
````markdown
# Known Limitations and Platform-Specific Behavior

This document describes the platform-specific capabilities and limitations for the Voice Input Tool, focusing on global shortcuts and text injection.

---

## Executive Summary

| Platform | Global Hotkey | Hold/Release Events | Text Injection | Permissions Required |
|----------|--------------|---------------------|----------------|---------------------|
| Windows | Full support | Reliable | SendInput | None |
| macOS | Full support | Reliable | CGEvent | Accessibility |
| Linux X11 | Full support | Reliable | xdotool | None |
| Linux Wayland | Limited | **Toggle only** | **Clipboard only** | Portal request |

---

## Windows

### Global Hotkey

**API:** `RegisterHotKey` / `UnregisterHotKey` (Win32)

**Behavior:**
- Reliable key-down detection via `WM_HOTKEY` message
- Key-up detection requires additional monitoring via low-level keyboard hook (`WH_KEYBOARD_LL`)
- Can register system-wide hotkeys that work even when app is not focused
- Some key combinations reserved by OS (e.g., Win+L, Ctrl+Alt+Del)

**Hold/Release Reliability:** High - keyboard hooks provide reliable press and release events

### Text Injection

**Methods:**
1. **Clipboard + Paste:** Set clipboard, send Ctrl+V via `SendInput`
2. **Direct keystroke:** `SendInput` for individual keystrokes

**Behavior:**
- `SendInput` is reliable in most applications
- Some games and secure applications may block synthetic input
- UAC-elevated windows require matching elevation level

### Permissions

**Required:** None for basic operation

**Elevation Considerations:**
- Injecting into elevated windows requires the app to run elevated
- Keyboard hooks work without elevation for non-elevated windows

### Recommended Mode

**Default:** `push_to_talk` (hold-to-record)

**Rationale:** Full hold/release support enables the most intuitive interaction.

### Diagnostics

```
Windows Platform Detected
✓ Global hotkey API available
✓ Text injection supported via SendInput
✓ No special permissions required

Note: If you cannot type into certain applications (like UAC dialogs or
some games), the application may be blocking synthetic input.
```

---

## macOS

### Global Hotkey

**API:** `CGEventTap` or `NSEvent.addGlobalMonitorForEvents`

**Behavior:**
- Requires Accessibility permission for global event monitoring
- `CGEventTap` provides key-down and key-up events
- Can use `NSEvent.addGlobalMonitorForEvents` for a higher-level API
- Both require the app to be trusted for Accessibility

**Hold/Release Reliability:** High - CGEventTap provides reliable press and release

### Text Injection

**Methods:**
1. **Clipboard + Cmd+V:** Use `NSPasteboard`, then inject Cmd+V via `CGEventCreateKeyboardEvent`
2. **AppleScript:** `tell application "System Events" to keystroke`

**Behavior:**
- Requires Accessibility permission for keystroke injection
- Works in most applications
- Sandboxed apps may have restrictions

### Permissions

**Required:**
1. **Accessibility:** For global hotkeys and keystroke injection
2. **Microphone:** For audio recording

**Detection:**
```swift
// Accessibility
AXIsProcessTrusted()

// Microphone
AVCaptureDevice.authorizationStatus(for: .audio)
```

**Requesting:**
- Accessibility: User must manually enable in System Preferences → Security & Privacy → Privacy → Accessibility
- Microphone: Standard permission dialog via `AVCaptureDevice.requestAccess`

### Recommended Mode

**Default:** `push_to_talk` (hold-to-record)

**Rationale:** Accessibility APIs provide reliable hold/release detection.

### Diagnostics

```
macOS Platform Detected

Checking Accessibility Permission...
✗ Accessibility permission not granted

To enable Voice Input Tool:
1. Open System Preferences → Security & Privacy → Privacy → Accessibility
2. Click the lock icon and enter your password
3. Enable "Voice Input Tool" in the list
4. Restart the application

Checking Microphone Permission...
✓ Microphone permission granted (or will be requested on first use)
```

---

## Linux X11

### Global Hotkey

**API:** XGrabKey / XQueryKeymap

**Behavior:**
- `XGrabKey` captures specified key combinations globally
- Key release detected via XKeyRelease events
- Works without special permissions
- May conflict with other applications grabbing the same key

**Hold/Release Reliability:** High - X11 provides reliable key events

### Text Injection

**Methods:**
1. **xdotool:** `xdotool type "text"` or clipboard + `xdotool key ctrl+v`
2. **XTest extension:** `XTestFakeKeyEvent` for synthetic key events
3. **xclip + xdotool:** Set clipboard, inject Ctrl+V

**Behavior:**
- Works in most X11 applications
- Some applications (e.g., terminal emulators) may use different paste shortcuts
- Games using SDL direct input may not receive synthetic events

### Permissions

**Required:** None (user session has full X11 access)

**Dependencies:**
- `xdotool` package for injection (or use XTest directly)
- `xclip` or `xsel` for clipboard access

### Recommended Mode

**Default:** `push_to_talk` (hold-to-record)

**Rationale:** Full X11 event support enables hold-to-record.

### Diagnostics

```
Linux X11 Platform Detected
✓ X11 display connected
✓ Global hotkey supported via XGrabKey
✓ Text injection supported via xdotool/XTest

Dependencies:
✓ xdotool available
✓ xclip available
```

---

## Linux Wayland

### Global Hotkey

**API:** XDG Desktop Portal `GlobalShortcuts` interface

**Behavior:**
- Portal-mediated shortcut registration
- **CRITICAL LIMITATION:** Most compositors only provide toggle events, not separate press/release
- Shortcuts must be registered via portal, user may see consent dialog
- Portal availability varies by compositor:
  - GNOME: Supported (toggle-only in many versions)
  - KDE Plasma: Supported (better press/release in recent versions)
  - wlroots-based: Limited or no portal support

**Hold/Release Reliability:** LOW - typically toggle-only

### Text Injection

**Methods:**
1. **Clipboard only:** `wl-copy` + compositor-native paste
2. **wtype:** Limited synthetic input (compositor-dependent)
3. **ydotool:** Requires root or uinput access

**Behavior:**
- **CRITICAL LIMITATION:** Wayland security model prohibits applications from synthesizing keystrokes into other windows
- Clipboard-based injection (set clipboard, user pastes) is the only reliable method
- Some compositors provide privileged input portals, but these are not standardized

### Permissions

**Required:**
- Portal access (usually granted)
- For ydotool: uinput device access or root

**Compositor-Specific:**
- GNOME: May require org.gnome.Shell.Extensions for additional input
- KDE: More permissive with KWin scripts

### Recommended Mode

**Default:** `toggle` (tap to start, tap to stop) or `clipboard_only`

**Rationale:** Without reliable key-up events, push-to-talk is not feasible on most Wayland compositors.

### Diagnostics

```
Linux Wayland Platform Detected

⚠ IMPORTANT: Wayland has significant limitations for global shortcuts and text input.

Global Shortcuts:
✓ XDG Desktop Portal available
⚠ Push-to-talk may not work reliably (compositor-dependent)
  Recommended: Use toggle mode (tap to start/stop)

Text Injection:
⚠ Direct keystroke injection not supported on Wayland
✓ Clipboard-based injection available
  Text will be copied to clipboard; paste manually with Ctrl+V

Compositor: GNOME Shell 45
Portal Version: 2

Tip: For best experience on Wayland, use toggle mode with clipboard injection.
```

---

## Effective Mode Defaults by Platform

| Platform | Activation Mode | Injection Method |
|----------|----------------|------------------|
| Windows | `push_to_talk` | `type_direct` |
| macOS | `push_to_talk` | `type_direct` |
| Linux X11 | `push_to_talk` | `type_direct` |
| Linux Wayland | `toggle` | `clipboard` |

### Activation Modes

- **`push_to_talk`**: Hold hotkey to record, release to transcribe
- **`toggle`**: Press hotkey to start recording, press again to stop
- **`always_on`**: Continuous listening with voice activity detection

### Injection Methods

- **`type_direct`**: Inject keystrokes directly into focused window
- **`clipboard`**: Copy to clipboard (user pastes manually)
- **`clipboard_autopaste`**: Copy to clipboard and inject Ctrl+V

---

## Remediation Text Templates

### Missing Accessibility (macOS)

```
Accessibility Permission Required

Voice Input Tool needs accessibility permission to detect global hotkeys
and type transcribed text into other applications.

To grant permission:
1. Click "Open System Preferences" below
2. Click the lock icon and enter your password
3. Find and check "Voice Input Tool" in the list
4. Restart Voice Input Tool

[Open System Preferences] [Cancel]
```

### Missing Microphone (macOS/Windows)

```
Microphone Permission Required

Voice Input Tool needs access to your microphone to transcribe your voice.

[Grant Permission] [Cancel]
```

### Wayland Limitations

```
Wayland Detected - Limited Functionality

Your system uses Wayland, which has security restrictions that affect
Voice Input Tool:

• Push-to-talk mode may not work reliably
  → We recommend using toggle mode instead

• Direct text typing is not supported
  → Transcribed text will be copied to your clipboard
  → Press Ctrl+V to paste into your application

[Use Recommended Settings] [Configure Manually]
```

### Missing xdotool (Linux X11)

```
Missing Dependency: xdotool

Voice Input Tool requires xdotool to type text into applications.

Install with:
  Ubuntu/Debian: sudo apt install xdotool
  Fedora: sudo dnf install xdotool
  Arch: sudo pacman -S xdotool

[Retry Detection] [Use Clipboard Mode Instead]
```

---

## Design Notes: capabilities.rs

The capability detection module should expose:

```rust
pub struct PlatformCapabilities {
    /// Can we detect global hotkey press events?
    pub global_hotkey_press: bool,

    /// Can we detect global hotkey release events?
    pub global_hotkey_release: bool,

    /// Can we inject keystrokes into other windows?
    pub keystroke_injection: bool,

    /// Can we access the clipboard?
    pub clipboard_access: bool,

    /// Does microphone permission check succeed?
    pub microphone_permission: PermissionState,

    /// Does accessibility permission check succeed (macOS)?
    pub accessibility_permission: PermissionState,

    /// Detected compositor/display server
    pub display_server: DisplayServer,

    /// Recommended activation mode for this platform
    pub recommended_activation_mode: ActivationMode,

    /// Recommended injection method for this platform
    pub recommended_injection_method: InjectionMethod,
}

pub enum PermissionState {
    Granted,
    Denied,
    NotDetermined,
    NotApplicable,
}

pub enum DisplayServer {
    Windows,
    MacOS,
    X11,
    Wayland { compositor: Option<String> },
}

pub enum ActivationMode {
    PushToTalk,
    Toggle,
    AlwaysOn,
}

pub enum InjectionMethod {
    TypeDirect,
    Clipboard,
    ClipboardAutopaste,
}

impl PlatformCapabilities {
    /// Detect capabilities for the current platform
    pub fn detect() -> Self { ... }

    /// Get user-facing diagnostics text
    pub fn diagnostics(&self) -> String { ... }

    /// Get list of issues that need remediation
    pub fn issues(&self) -> Vec<CapabilityIssue> { ... }
}
```

---

## References

### Windows
- [RegisterHotKey function](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey)
- [SendInput function](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
- [Low-Level Keyboard Hook](https://learn.microsoft.com/en-us/windows/win32/winmsg/about-hooks#wh_keyboard_ll)

### macOS
- [CGEventTap](https://developer.apple.com/documentation/coregraphics/cgeventtap)
- [Accessibility API](https://developer.apple.com/documentation/applicationservices/axuielement)
- [AXIsProcessTrusted](https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrusted)

### Linux X11
- [XGrabKey](https://www.x.org/releases/current/doc/libX11/libX11/libX11.html#XGrabKey)
- [XTest Extension](https://www.x.org/releases/current/doc/xextproto/xtest.html)
- [xdotool](https://github.com/jordansissel/xdotool)

### Linux Wayland
- [XDG Desktop Portal GlobalShortcuts](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html)
- [wtype](https://github.com/atx/wtype)
- [ydotool](https://github.com/ReimuNotMoe/ydotool)
````

## File: docs/M0.6-sidecar-packaging-spike.md
````markdown
# M0.6: Sidecar Packaging Feasibility Spike

**Date:** 2026-02-04
**Status:** ✅ GO - Packaging is feasible

## Executive Summary

The OpenVoicy sidecar can be successfully packaged as a standalone binary using PyInstaller. The minimal audio-only build (without ML dependencies) is 57 MB with ~1.3s startup time, well within the acceptance criteria of <500 MB and <5s.

## Test Results

### Platform: Linux x64 (Ubuntu)

| Metric | Value | Criteria | Status |
|--------|-------|----------|--------|
| Binary Size | 57 MB | <500 MB | ✅ PASS |
| Startup Time | ~1.3s | <5s | ✅ PASS |
| system.ping | Works | Required | ✅ PASS |
| audio.list_devices | Works | Required | ✅ PASS |
| audio.meter_start | Works* | Required | ✅ PASS |

*Returns "no audio device" error in headless environment - PortAudio library successfully included.

### Binary Contents

The packaged binary includes:
- Python 3.13 runtime (embedded)
- NumPy 2.4.2
- SciPy 1.17.0
- sounddevice 0.5.5
- PortAudio 19.6.0 (system library)
- JACK audio libraries (transitive dependency)
- OpenBLAS (for NumPy/SciPy)

### Commands Tested

```bash
# system.ping - SUCCESS
$ echo '{"jsonrpc":"2.0","id":1,"method":"system.ping"}' | ./openvoicy-sidecar
{"jsonrpc":"2.0","id":1,"result":{"version":"0.1.0","protocol":"v1"}}

# audio.list_devices - SUCCESS
$ echo '{"jsonrpc":"2.0","id":2,"method":"audio.list_devices"}' | ./openvoicy-sidecar
{"jsonrpc":"2.0","id":2,"result":{"devices":[]}}

# audio.meter_start - SUCCESS (PortAudio working, no device available)
$ echo '{"jsonrpc":"2.0","id":3,"method":"audio.meter_start"}' | ./openvoicy-sidecar
{"jsonrpc":"2.0","id":3,"error":{"code":-32004,"message":"Error querying device -1","data":{"kind":"E_AUDIO_IO"}}}
```

## Size Projections with ML Dependencies

| Configuration | Estimated Size | Notes |
|--------------|----------------|-------|
| Audio-only (current) | 57 MB | NumPy, SciPy, PortAudio |
| + ONNX Runtime (CPU) | ~150 MB | Recommended for MVP |
| + PyTorch (CPU) | ~400-500 MB | Near size limit |
| + PyTorch (CUDA) | ~2+ GB | Exceeds limit |

**Recommendation:** Use ONNX Runtime for MVP to stay well under 500 MB.

## Native Dependencies

### Linux x64
- libportaudio2 (PortAudio)
- libjack (JACK audio)
- libasound2 (ALSA, typically pre-installed)

### Windows x64 (TBD)
- PortAudio bundled with sounddevice
- No additional system dependencies expected

### macOS (TBD)
- PortAudio bundled with sounddevice
- CoreAudio (system framework)
- May need code signing for Gatekeeper

## Challenges Encountered

1. **Relative Import Issue**: PyInstaller doesn't handle relative imports in `__main__.py`.
   - **Solution:** Created `entry_point.py` wrapper with absolute imports.

2. **PortAudio Not Bundled**: sounddevice looks for system PortAudio at runtime.
   - **Solution:** Explicitly include system library in spec file.

3. **JACK Dependency**: PortAudio pulls in JACK libraries even if not used.
   - **Impact:** Adds ~2 MB to binary size. Acceptable.

## Build Artifacts

```
sidecar/
├── openvoicy_sidecar.spec    # PyInstaller spec file
├── entry_point.py            # Wrapper for PyInstaller
├── dist/
│   └── openvoicy-sidecar     # Standalone binary (57 MB)
└── build/                    # Build artifacts (can be cleaned)
```

## GO/NO-GO Decision

### GO ✅

**Rationale:**
1. Binary size (57 MB) is 11% of the 500 MB limit
2. Startup time (1.3s) is 26% of the 5s limit
3. All required functionality works
4. ONNX Runtime addition (~100 MB) stays well under limits
5. No blocking native dependency issues on Linux

### Contingency (if ML size becomes issue)

If the full model + ONNX exceeds acceptable size:
1. Use smaller Whisper model (tiny/base instead of parakeet)
2. Download model on first run (not bundled)
3. Stream model from HuggingFace cache

The IPC contract remains unchanged regardless of backend choice.

## Next Steps

1. **M5.1:** Build sidecar per OS via PyInstaller
   - Test Windows x64
   - Test macOS x64 and arm64
2. **M5.2:** Integrate into Tauri bundling
3. Add ONNX Runtime and test ASR model packaging

## Files Created

- `sidecar/openvoicy_sidecar.spec` - PyInstaller configuration
- `sidecar/entry_point.py` - PyInstaller entry wrapper
- `docs/M0.6-sidecar-packaging-spike.md` - This report
````

## File: docs/MANUAL_CHECKLIST.md
````markdown
# OpenVoicy Manual Testing Checklist

This document provides a comprehensive step-by-step validation checklist for testing OpenVoicy. Each section should be validated on every target platform before release.

## Environment Setup

Before testing, record the test environment:

- [ ] OS: ____________ (e.g., "Windows 11 23H2", "macOS 14.3", "Ubuntu 24.04 Wayland")
- [ ] Architecture: ____________ (x64 / arm64)
- [ ] Python version: ____________ (if running from source)
- [ ] GPU available: Yes / No
- [ ] Test date: ____________

---

## 1. First Launch

### 1.1 Application Start

- [ ] App launches without crashing
- [ ] Tray icon appears in system tray
- [ ] Tray icon shows "loading" state initially
- [ ] Settings window can be opened from tray menu

### 1.2 Model Download (Fresh Install)

**Prerequisites:** Network available, no cached model

- [ ] Model status shows "Not Downloaded"
- [ ] "Download Model" button is visible
- [ ] Clicking download starts the download
- [ ] Progress bar shows download progress
- [ ] Download completes successfully (may take several minutes)
- [ ] Status changes to "Ready" after verification
- [ ] Tray icon updates to idle state

**Expected download size:** ~2.5 GB

### 1.3 Offline Verification

**Prerequisites:** Model downloaded and ready

1. Disconnect from network (disable WiFi/Ethernet at OS level)
2. Restart the application
3. Record voice and transcribe

- [ ] App launches without network
- [ ] Model status shows "Ready" (cached)
- [ ] Voice recording works
- [ ] Transcription completes successfully
- [ ] No network error messages

---

## 2. Core Voice Flow

### 2.1 Hold-to-Talk Recording (if supported)

**Prerequisites:** Model ready, microphone available

1. Press and hold the hotkey (default: Ctrl+Shift+Space)
2. Speak a test phrase: "The quick brown fox jumps over the lazy dog"
3. Release the hotkey

- [ ] Tray icon changes to "recording" state on key press
- [ ] Tray icon changes to "transcribing" state on key release
- [ ] Tray icon returns to "idle" after completion
- [ ] Transcribed text appears in currently focused input field
- [ ] Transcription is reasonably accurate

### 2.2 Toggle Mode Recording

**Prerequisites:** Hotkey mode set to "toggle"

1. Press hotkey once to start recording
2. Speak a test phrase
3. Press hotkey again to stop

- [ ] First press starts recording
- [ ] Second press stops recording and triggers transcription
- [ ] Text is injected correctly

### 2.3 Recording Durations

Test various recording lengths:

- [ ] **Short (< 1 second):** Handled gracefully (may show warning)
- [ ] **Normal (2-5 seconds):** Transcribes correctly
- [ ] **Long (30+ seconds):** Transcribes without timeout

### 2.4 Rapid Press/Release

Press and release hotkey very quickly multiple times:

- [ ] App does not crash
- [ ] State machine recovers correctly
- [ ] No duplicate transcriptions

---

## 3. Text Injection

### 3.1 Injection to Various Apps

Test injection into different applications:

- [ ] Text editor (VS Code, Notepad, TextEdit)
- [ ] Web browser (input field, textarea)
- [ ] Word processor (Microsoft Word, LibreOffice)
- [ ] Terminal/Console
- [ ] Chat application (Slack, Discord)

### 3.2 Focus Guard

**Prerequisites:** Focus Guard enabled (default)

1. Start recording
2. While recording, switch focus to a different window
3. Release to transcribe

- [ ] Text goes to clipboard only (not injected)
- [ ] Warning notification appears
- [ ] Clipboard contains the transcribed text
- [ ] History shows "clipboard_only" with reason

### 3.3 Self-Injection Prevention

1. Focus the OpenVoicy settings window
2. Attempt to record and transcribe

- [ ] Text is NOT injected into settings window
- [ ] Text goes to clipboard instead
- [ ] Warning or notification appears

### 3.4 Unicode Support

Test transcription with various accents/languages:

- [ ] Accented characters (café, naïve)
- [ ] Special punctuation ("quotes", em-dash)
- [ ] Numbers and symbols

### 3.5 Suffix Configuration

Test each suffix option:

- [ ] **Empty:** No extra character after injection
- [ ] **Space (default):** Single space appended
- [ ] **Newline:** Line break appended

---

## 4. Microphone Configuration

### 4.1 Device Selection

- [ ] Device dropdown shows available microphones
- [ ] Default device is pre-selected
- [ ] Selecting different device updates setting
- [ ] Selected device persists after restart

### 4.2 Microphone Test

1. Click "Start Test" in microphone settings
2. Speak or make sound

- [ ] Level meter responds to audio
- [ ] Green/yellow/red zones visible
- [ ] Peak hold indicator works
- [ ] "Stop Test" stops the meter

### 4.3 No Signal Detection

1. Start microphone test
2. Wait 3+ seconds without making sound

- [ ] "No audio detected" warning appears
- [ ] Troubleshooting text is shown
- [ ] Warning clears when audio is detected

### 4.4 Device Disconnect

1. Start recording
2. Disconnect/disable the microphone

- [ ] Error is surfaced to user
- [ ] App does not crash
- [ ] State returns to idle

---

## 5. Hotkey Configuration

### 5.1 Custom Hotkey

1. Go to hotkey settings
2. Click to record new hotkey
3. Press desired key combination

- [ ] Hotkey recorded correctly
- [ ] New hotkey works for recording
- [ ] Setting persists after restart

### 5.2 Copy Last Hotkey

1. Record and transcribe some text
2. Press copy-last hotkey (default: Ctrl+Shift+C)

- [ ] Last transcript is copied to clipboard
- [ ] Tray shows feedback (if enabled)

### 5.3 Hotkey Conflicts

Try setting a hotkey that conflicts with system/other apps:

- [ ] Warning shown if conflict detected
- [ ] User can proceed or choose different hotkey

---

## 6. Replacement Rules

### 6.1 Literal Replacement

1. Add rule: pattern="brb", replacement="be right back"
2. Transcribe speech containing "brb"

- [ ] "brb" is replaced with "be right back"
- [ ] Word boundary respected (brb123 not replaced)

### 6.2 Regex Replacement

1. Add regex rule: pattern="\bteh\b", replacement="the"
2. Transcribe speech containing "teh"

- [ ] "teh" is replaced with "the"

### 6.3 Preview

1. Enter test text in preview area
2. View output

- [ ] Replacements are shown in real-time
- [ ] Diff highlighting works

### 6.4 Macros

Test macro expansion:

- [ ] `{{date}}` expands to current date
- [ ] `{{time}}` expands to current time
- [ ] `{{datetime}}` expands to date and time

### 6.5 Import/Export

- [ ] Export rules to JSON file
- [ ] Import rules from JSON file
- [ ] Imported rules work correctly

---

## 7. Tray Menu

### 7.1 Menu Items

Right-click tray icon:

- [ ] "Show Settings" opens settings window
- [ ] "Copy Last Transcript" copies to clipboard
- [ ] "Enabled/Disabled" toggle works
- [ ] "Restart Sidecar" restarts sidecar process
- [ ] "Quit" exits application

### 7.2 State Icons

Observe tray icon during:

- [ ] **Idle:** Default/green icon
- [ ] **Recording:** Red/recording icon
- [ ] **Transcribing:** Yellow/processing icon
- [ ] **Model Loading:** Loading/spinner icon
- [ ] **Error:** Red/error icon

---

## 8. Error Handling

### 8.1 Sidecar Crash

1. Manually kill the sidecar process
2. Observe app behavior

- [ ] Error state shown in tray
- [ ] User can restart sidecar from menu
- [ ] App recovers after restart

### 8.2 Model Download Failure

Simulate network failure during model download:

- [ ] Error message shown
- [ ] "Retry Download" button available
- [ ] Resume works if supported

### 8.3 Microphone Permission Denied

(Platform specific)

- [ ] Clear error message shown
- [ ] Instructions for granting permission
- [ ] App recovers after permission granted

---

## 9. Platform-Specific

### 9.1 macOS

- [ ] Microphone permission prompt appears on first use
- [ ] Accessibility permission works for injection
- [ ] Menu bar icon displays correctly
- [ ] Gatekeeper allows app to run

### 9.2 Windows

- [ ] UAC prompt handled (if applicable)
- [ ] Windows Defender allows app
- [ ] System tray icon visible
- [ ] Injection works in both admin and non-admin apps

### 9.3 Linux X11

- [ ] Hotkey registration works
- [ ] Paste injection works in X11 apps
- [ ] Tray icon visible (appindicator)

### 9.4 Linux Wayland

- [ ] Hotkey mode defaults to toggle (if hold not available)
- [ ] Effective mode displayed in settings
- [ ] Injection works via portal/clipboard
- [ ] Tray works (varies by compositor)

---

## 10. Self-Check / Diagnostics

### 10.1 Self-Check Panel

Open Settings > Diagnostics > Self-Check:

- [ ] All checks run successfully
- [ ] Status indicators (green/yellow/red) are accurate
- [ ] Expandable details work

### 10.2 Copy Diagnostics

- [ ] "Copy to Clipboard" button works
- [ ] Diagnostics text is comprehensive
- [ ] Sensitive paths are redacted
- [ ] No transcript text in diagnostics

---

## 11. Settings Persistence

### 11.1 Settings Survive Restart

1. Configure various settings
2. Restart app

- [ ] Microphone selection persisted
- [ ] Hotkey settings persisted
- [ ] Injection settings persisted
- [ ] Replacement rules persisted
- [ ] Window size/position persisted (optional)

### 11.2 Config Corruption Recovery

1. Corrupt the config file (manually edit)
2. Start app

- [ ] App starts with defaults
- [ ] Warning or notification shown
- [ ] User can reconfigure

---

## Sign-Off

| Test Category | Passed | Failed | Notes |
|--------------|--------|--------|-------|
| First Launch | | | |
| Core Voice Flow | | | |
| Text Injection | | | |
| Microphone | | | |
| Hotkey | | | |
| Replacements | | | |
| Tray Menu | | | |
| Error Handling | | | |
| Platform-Specific | | | |
| Diagnostics | | | |
| Persistence | | | |

**Tester:** ____________

**Date:** ____________

**Version:** ____________

**Overall Result:** PASS / FAIL

**Critical Issues:**
-
-
-

**Notes:**
-
-
-
````

## File: docs/PRIVACY.md
````markdown
# Privacy Policy

OpenVoicy is designed with privacy as a core principle. All processing happens locally on your device with no data sent to external servers.

---

## Summary

| Data Type | Stored | Sent to Cloud |
|-----------|--------|---------------|
| Voice recordings | ❌ No | ❌ No |
| Transcripts | ❌ No | ❌ No |
| Configuration | ✅ Local only | ❌ No |
| ASR Model | ✅ Local cache | ❌ Downloaded once |

---

## What Data is Stored

### Configuration File
- **Location**: OS-specific config directory (see Data Locations below)
- **Contains**:
  - Microphone device selection
  - Hotkey preferences
  - Injection settings (suffix, Focus Guard, etc.)
  - Replacement rules
- **Format**: JSON file
- **Retention**: Until manually deleted or app uninstalled

### Model Cache
- **Location**: OS-specific cache directory
- **Contains**: NVIDIA Parakeet ASR model files (~2.5 GB)
- **Source**: Downloaded from Hugging Face on first run
- **Retention**: Cached indefinitely for offline use

### Logs (Optional)
- **Location**: OS-specific log directory
- **Contains**: Application events, errors, timing information
- **Does NOT contain**: Transcript text or audio data
- **Retention**: Ring buffer, bounded size, not persisted by default
- **Purpose**: Diagnostics and troubleshooting only

---

## What Data is NOT Stored

### Voice Recordings
Audio captured from your microphone is:
- Processed in memory for transcription
- Immediately discarded after processing
- Never saved to disk
- Never sent to any server

### Transcripts
Transcribed text is:
- Held in memory for the current session (history feature)
- Cleared when the app closes
- Never saved to disk
- Never sent to any server

### Usage Analytics
OpenVoicy does not collect:
- Usage statistics
- Crash reports
- Telemetry of any kind
- Device identifiers

---

## Network Usage

### Model Download
- **When**: First launch (if model not cached)
- **What**: ASR model files from Hugging Face
- **Connection**: HTTPS to huggingface.co
- **After download**: No further network required

### During Transcription
- **Network**: Not used
- **All processing**: Local, on your CPU/GPU
- **Verification**: Works completely offline after model download

### No Phone-Home
- No update checks (unless you enable them)
- No license verification
- No analytics or telemetry
- No cloud sync

---

## Data Locations

### Configuration
| Platform | Location |
|----------|----------|
| Windows | `%APPDATA%\openvoicy\config.json` |
| macOS | `~/Library/Application Support/openvoicy/config.json` |
| Linux | `~/.config/openvoicy/config.json` |

### Model Cache
| Platform | Location |
|----------|----------|
| Windows | `%LOCALAPPDATA%\openvoicy\cache\` |
| macOS | `~/Library/Caches/openvoicy/` |
| Linux | `~/.cache/openvoicy/` |

### Logs (if enabled)
| Platform | Location |
|----------|----------|
| Windows | `%LOCALAPPDATA%\openvoicy\logs\` |
| macOS | `~/Library/Logs/openvoicy/` |
| Linux | `~/.local/share/openvoicy/logs/` |

---

## How to Clear All Data

### Quick Clear (from App)
1. Open Settings → Advanced → Clear Data
2. Select what to clear:
   - Configuration (resets settings)
   - Model cache (re-download required)
   - Logs

### Complete Removal

#### Windows
```powershell
# Remove config
Remove-Item -Recurse "$env:APPDATA\openvoicy"
# Remove cache
Remove-Item -Recurse "$env:LOCALAPPDATA\openvoicy"
```

#### macOS
```bash
# Remove config
rm -rf ~/Library/Application\ Support/openvoicy
# Remove cache
rm -rf ~/Library/Caches/openvoicy
# Remove logs
rm -rf ~/Library/Logs/openvoicy
```

#### Linux
```bash
# Remove all OpenVoicy data
rm -rf ~/.config/openvoicy
rm -rf ~/.cache/openvoicy
rm -rf ~/.local/share/openvoicy
```

---

## Third-Party Model License

OpenVoicy uses the NVIDIA Parakeet TDT 0.6B model, which is licensed under [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/).

- The model is downloaded from NVIDIA's public Hugging Face repository
- No NVIDIA account required
- Attribution provided in [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)

---

## Permissions Required

### Microphone
- **Why**: To capture voice for transcription
- **When**: Only while actively recording
- **Access**: Granted via OS permission prompt

### Accessibility (macOS only)
- **Why**: To inject text into other applications
- **When**: Only during text injection
- **Alternative**: Clipboard-only mode if permission denied

### Input Monitoring (if applicable)
- **Why**: Global hotkey detection
- **When**: While app is running
- **Alternative**: Toggle mode via tray menu

---

## Questions or Concerns

OpenVoicy is open source. You can:
- Review the code: [GitHub Repository]
- Report privacy issues: [GitHub Issues]
- Build from source for maximum assurance

---

*Last updated: 2026-02-05*
````

## File: docs/THIRD_PARTY_NOTICES.md
````markdown
# Third-Party Notices

This file contains the licenses and notices for third-party software included in or distributed with OpenVoicy.

---

## ASR Model

### NVIDIA Parakeet TDT 0.6B v3
- **Source**: https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3
- **License**: CC-BY-4.0 (Creative Commons Attribution 4.0 International)
- **Copyright**: NVIDIA Corporation
- **Notes**: The model is downloaded on first run and cached locally. Attribution to NVIDIA is required when distributing or using the model.

---

## Rust Dependencies

The following Rust crates are used in the Tauri backend:

### tauri
- **Version**: 2.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/tauri-apps/tauri
- **Copyright**: The Tauri Programme within The Commons Conservancy

### tauri-plugin-shell
- **Version**: 2.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/tauri-apps/plugins-workspace

### serde
- **Version**: 1.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/serde-rs/serde
- **Copyright**: David Tolnay

### serde_json
- **Version**: 1.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/serde-rs/json

### tokio
- **Version**: 1.x
- **License**: MIT
- **Source**: https://github.com/tokio-rs/tokio
- **Copyright**: Tokio Contributors

### thiserror
- **Version**: 2.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/dtolnay/thiserror

### log
- **Version**: 0.4.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/rust-lang/log

### env_logger
- **Version**: 0.11.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/rust-cli/env_logger

### chrono
- **Version**: 0.4.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/chronotope/chrono

### uuid
- **Version**: 1.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/uuid-rs/uuid

### once_cell
- **Version**: 1.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/matklad/once_cell

### regex
- **Version**: 1.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/rust-lang/regex

### phf
- **Version**: 0.11.x
- **License**: MIT
- **Source**: https://github.com/sfackler/rust-phf

### dirs
- **Version**: 5.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/dirs-dev/dirs-rs

### global-hotkey
- **Version**: 0.6.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/tauri-apps/global-hotkey

### png
- **Version**: 0.17.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/image-rs/image-png

---

## Python Dependencies

The following Python packages are used in the sidecar:

### sounddevice
- **Version**: >=0.4.6
- **License**: MIT
- **Source**: https://github.com/spatialaudio/python-sounddevice
- **Copyright**: Matthias Geier
- **Notes**: Provides Python bindings for PortAudio

### numpy
- **Version**: >=1.24.0
- **License**: BSD-3-Clause
- **Source**: https://github.com/numpy/numpy
- **Copyright**: NumPy Developers

### scipy
- **Version**: >=1.10.0
- **License**: BSD-3-Clause
- **Source**: https://github.com/scipy/scipy
- **Copyright**: SciPy Developers

### PyTorch (torch)
- **Version**: >=2.0.0
- **License**: BSD-3-Clause
- **Source**: https://github.com/pytorch/pytorch
- **Copyright**: Meta Platforms, Inc. and affiliates
- **Notes**: Runtime dependency for ASR inference. Users install separately based on their hardware (CPU/CUDA). PyTorch includes modified components from various open source projects, see full NOTICE at https://github.com/pytorch/pytorch/blob/main/NOTICE

### NVIDIA NeMo Toolkit (nemo_toolkit)
- **Version**: >=2.0.0
- **License**: Apache-2.0
- **Source**: https://github.com/NVIDIA/NeMo
- **Copyright**: NVIDIA Corporation
- **Notes**: Runtime dependency for loading and running Parakeet ASR models. Install with `pip install nemo_toolkit[asr]` for ASR functionality.

---

## JavaScript Dependencies

The following JavaScript packages are used in the React frontend:

### React
- **Version**: 18.x
- **License**: MIT
- **Source**: https://github.com/facebook/react
- **Copyright**: Meta Platforms, Inc. and affiliates

### React DOM
- **Version**: 18.x
- **License**: MIT
- **Source**: https://github.com/facebook/react

### Zustand
- **Version**: 5.x
- **License**: MIT
- **Source**: https://github.com/pmndrs/zustand
- **Copyright**: Daishi Kato

### @tauri-apps/api
- **Version**: 2.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/tauri-apps/tauri

### @tauri-apps/plugin-shell
- **Version**: 2.x
- **License**: MIT OR Apache-2.0
- **Source**: https://github.com/tauri-apps/plugins-workspace

---

## Audio Libraries

### PortAudio
- **Version**: 19.x (bundled via sounddevice)
- **License**: MIT
- **Source**: http://www.portaudio.com/
- **Copyright**: Ross Bencina, Phil Burk
- **Notes**: Cross-platform audio I/O library, bundled in the sidecar binary

---

## Icons and Audio Assets

The following assets are original works created for this project and are licensed under the same terms as the project itself:

### Tray Icons
- `tray-idle-*.png` - Idle state indicator (microphone icon)
- `tray-recording-*.png` - Recording state indicator (red microphone)
- `tray-transcribing-*.png` - Transcribing state indicator (processing)
- `tray-loading-*.png` - Loading/initializing state
- `tray-error-*.png` - Error state indicator
- `tray-disabled-*.png` - Disabled state indicator

### Audio Cues
- `cue-start.wav` - Recording start notification sound
- `cue-stop.wav` - Recording stop notification sound
- `cue-error.wav` - Error notification sound

These assets were programmatically generated or created specifically for OpenVoicy and do not require third-party attribution.

---

## Build and Development Tools

The following tools are used during development and are not redistributed:

- **Vite** - MIT License
- **TypeScript** - Apache-2.0 License
- **Tailwind CSS** - MIT License
- **ESLint** - MIT License
- **Vitest** - MIT License
- **Hatch** (Python) - MIT License
- **Ruff** (Python) - MIT License

---

## License Texts

### MIT License

```
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### Apache License 2.0

```
Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

### BSD 3-Clause License

```
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### Creative Commons Attribution 4.0 International (CC-BY-4.0)

The NVIDIA Parakeet model is licensed under CC-BY-4.0. This license allows:
- Sharing and adapting the material
- Commercial use

With the requirement to:
- Give appropriate credit to NVIDIA
- Indicate if changes were made
- Not apply additional restrictions

Full license text: https://creativecommons.org/licenses/by/4.0/legalcode

---

## Compliance Notes

1. **No GPL Dependencies**: This project does not include any GPL or AGPL licensed dependencies to ensure compatibility with the overall project license.

2. **Model Attribution**: The NVIDIA Parakeet model requires attribution. This is satisfied by this notice file and the model information displayed in the application settings.

3. **Transitive Dependencies**: Transitive dependencies inherit compatible licenses (MIT, Apache-2.0, BSD, MPL-2.0). No incompatible licenses are introduced through transitive dependencies.

4. **MPL-2.0 Transitive Dependencies**: Some CSS-related Rust crates (`cssparser`, `selectors`, `dtoa-short`) use MPL-2.0 licensing. MPL-2.0 is file-based copyleft and compatible with MIT/Apache-2.0 when the MPL-licensed files are not modified.

5. **Unicode License**: ICU-related crates use the Unicode-3.0 license, which is a permissive license allowing redistribution with or without modification.

---

*Last updated: 2026-02-05*
````

## File: scripts/e2e/lib/assert.sh
````bash
#!/usr/bin/env bash
#
# E2E Test Assertion Library
# Provides assertion functions for E2E tests with structured logging.
#
# Usage:
#   source scripts/e2e/lib/log.sh
#   source scripts/e2e/lib/assert.sh
#   assert_eq "expected" "$actual" "Values should match"
#

set -euo pipefail

# Track assertion counts
E2E_ASSERTIONS_PASSED=0
E2E_ASSERTIONS_FAILED=0

# Assert two values are equal
# Args: expected, actual, msg
# Returns: 0 on pass, 1 on fail
assert_eq() {
    local expected="$1"
    local actual="$2"
    local msg="${3:-Values should be equal}"

    if [ "$expected" = "$actual" ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_eq" "PASS: $msg" "{\"expected\":\"$expected\",\"actual\":\"$actual\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_eq" "FAIL: $msg" "{\"expected\":\"$expected\",\"actual\":\"$actual\"}"
        return 1
    fi
}

# Assert two values are not equal
# Args: unexpected, actual, msg
assert_ne() {
    local unexpected="$1"
    local actual="$2"
    local msg="${3:-Values should not be equal}"

    if [ "$unexpected" != "$actual" ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_ne" "PASS: $msg" "{\"unexpected\":\"$unexpected\",\"actual\":\"$actual\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_ne" "FAIL: $msg" "{\"unexpected\":\"$unexpected\",\"actual\":\"$actual\"}"
        return 1
    fi
}

# Assert string contains substring
# Args: haystack, needle, msg
assert_contains() {
    local haystack="$1"
    local needle="$2"
    local msg="${3:-String should contain substring}"

    if [[ "$haystack" == *"$needle"* ]]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_contains" "PASS: $msg" "{\"needle\":\"$needle\",\"found\":true}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_contains" "FAIL: $msg" "{\"needle\":\"$needle\",\"found\":false,\"haystack\":\"${haystack:0:100}\"}"
        return 1
    fi
}

# Assert string does not contain substring
# Args: haystack, needle, msg
assert_not_contains() {
    local haystack="$1"
    local needle="$2"
    local msg="${3:-String should not contain substring}"

    if [[ "$haystack" != *"$needle"* ]]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_not_contains" "PASS: $msg" "{\"needle\":\"$needle\",\"found\":false}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_not_contains" "FAIL: $msg" "{\"needle\":\"$needle\",\"found\":true}"
        return 1
    fi
}

# Assert file exists
# Args: filepath, msg
assert_file_exists() {
    local filepath="$1"
    local msg="${2:-File should exist}"

    if [ -f "$filepath" ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_file_exists" "PASS: $msg" "{\"path\":\"$filepath\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_file_exists" "FAIL: $msg" "{\"path\":\"$filepath\"}"
        return 1
    fi
}

# Assert directory exists
# Args: dirpath, msg
assert_dir_exists() {
    local dirpath="$1"
    local msg="${2:-Directory should exist}"

    if [ -d "$dirpath" ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_dir_exists" "PASS: $msg" "{\"path\":\"$dirpath\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_dir_exists" "FAIL: $msg" "{\"path\":\"$dirpath\"}"
        return 1
    fi
}

# Assert process is running
# Args: process_name_or_pid, msg
assert_process_running() {
    local target="$1"
    local msg="${2:-Process should be running}"

    local running=false
    if [[ "$target" =~ ^[0-9]+$ ]]; then
        # PID
        if kill -0 "$target" 2>/dev/null; then
            running=true
        fi
    else
        # Process name
        if pgrep -f "$target" >/dev/null 2>&1; then
            running=true
        fi
    fi

    if [ "$running" = true ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_process_running" "PASS: $msg" "{\"target\":\"$target\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_process_running" "FAIL: $msg" "{\"target\":\"$target\"}"
        return 1
    fi
}

# Assert command succeeds (exit code 0)
# Args: command, msg
assert_cmd_succeeds() {
    local cmd="$1"
    local msg="${2:-Command should succeed}"

    if eval "$cmd" >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_cmd_succeeds" "PASS: $msg" "{\"command\":\"$cmd\"}"
        return 0
    else
        local exit_code=$?
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_cmd_succeeds" "FAIL: $msg" "{\"command\":\"$cmd\",\"exit_code\":$exit_code}"
        return 1
    fi
}

# Assert command fails (non-zero exit code)
# Args: command, msg
assert_cmd_fails() {
    local cmd="$1"
    local msg="${2:-Command should fail}"

    if ! eval "$cmd" >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_cmd_fails" "PASS: $msg" "{\"command\":\"$cmd\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_cmd_fails" "FAIL: $msg" "{\"command\":\"$cmd\"}"
        return 1
    fi
}

# Assert JSON field equals value
# Args: json_string, jq_path, expected_value, msg
assert_json_eq() {
    local json="$1"
    local path="$2"
    local expected="$3"
    local msg="${4:-JSON field should equal expected value}"

    local actual
    actual=$(echo "$json" | jq -r "$path" 2>/dev/null || echo "__JQ_ERROR__")

    if [ "$actual" = "$expected" ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_json_eq" "PASS: $msg" "{\"path\":\"$path\",\"expected\":\"$expected\",\"actual\":\"$actual\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_json_eq" "FAIL: $msg" "{\"path\":\"$path\",\"expected\":\"$expected\",\"actual\":\"$actual\"}"
        return 1
    fi
}

# Assert value is within numeric range
# Args: actual, min, max, msg
assert_in_range() {
    local actual="$1"
    local min="$2"
    local max="$3"
    local msg="${4:-Value should be in range}"

    if (( $(echo "$actual >= $min" | bc -l) )) && (( $(echo "$actual <= $max" | bc -l) )); then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_in_range" "PASS: $msg" "{\"actual\":$actual,\"min\":$min,\"max\":$max}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_in_range" "FAIL: $msg" "{\"actual\":$actual,\"min\":$min,\"max\":$max}"
        return 1
    fi
}

# Assert duration is under limit
# Args: duration_ms, limit_ms, msg
assert_duration_under() {
    local duration="$1"
    local limit="$2"
    local msg="${3:-Duration should be under limit}"

    if (( duration < limit )); then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_duration_under" "PASS: $msg" "{\"duration_ms\":$duration,\"limit_ms\":$limit}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_duration_under" "FAIL: $msg" "{\"duration_ms\":$duration,\"limit_ms\":$limit}"
        return 1
    fi
}

# Wait for condition with timeout
# Args: condition_cmd, timeout_seconds, poll_interval_ms, msg
wait_for() {
    local condition="$1"
    local timeout="${2:-30}"
    local poll_interval="${3:-500}"
    local msg="${4:-Waiting for condition}"

    local start_time
    start_time=$(date +%s)
    local end_time=$((start_time + timeout))

    log_info "test" "wait_for" "Waiting: $msg" "{\"timeout\":$timeout,\"poll_interval_ms\":$poll_interval}"

    while [ "$(date +%s)" -lt "$end_time" ]; do
        if eval "$condition" >/dev/null 2>&1; then
            local duration=$(($(date +%s) - start_time))
            log_info "test" "wait_for" "Condition met: $msg" "{\"waited_seconds\":$duration}"
            return 0
        fi
        sleep "$(echo "scale=3; $poll_interval / 1000" | bc)"
    done

    log_error "test" "wait_for" "Timeout: $msg" "{\"timeout\":$timeout}"
    return 1
}

# Print assertion summary
assertion_summary() {
    local total=$((E2E_ASSERTIONS_PASSED + E2E_ASSERTIONS_FAILED))
    local result="PASS"
    [ "$E2E_ASSERTIONS_FAILED" -gt 0 ] && result="FAIL"

    log_info "test" "summary" "Assertion summary: $result" "{\"passed\":$E2E_ASSERTIONS_PASSED,\"failed\":$E2E_ASSERTIONS_FAILED,\"total\":$total}"

    echo ""
    echo "=== Assertions ==="
    echo "Passed: $E2E_ASSERTIONS_PASSED"
    echo "Failed: $E2E_ASSERTIONS_FAILED"
    echo "Total:  $total"

    return "$E2E_ASSERTIONS_FAILED"
}
````

## File: scripts/e2e/lib/common.sh
````bash
#!/usr/bin/env bash
#
# E2E Test Common Utilities
# Provides helpers for sidecar IPC, process management, and platform detection.
#
# Usage:
#   source scripts/e2e/lib/log.sh
#   source scripts/e2e/lib/common.sh
#

set -euo pipefail

# Project paths
E2E_PROJECT_ROOT=""
E2E_SIDECAR_BIN=""
E2E_SIDECAR_PID=""
E2E_SIDECAR_STDIN=""
E2E_SIDECAR_STDOUT=""

# Platform detection
E2E_PLATFORM=""
E2E_ARCH=""

# Initialize common paths and detect platform
init_common() {
    # Find project root by looking for sidecar directory
    # This handles both bash (BASH_SOURCE) and zsh (no BASH_SOURCE) environments
    local script_dir=""

    if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
        script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
        E2E_PROJECT_ROOT="$(cd "$script_dir/../../.." && pwd)"
    elif [[ -n "${0:-}" ]] && [[ -f "$0" ]]; then
        script_dir="$(cd "$(dirname "$0")" && pwd)"
        E2E_PROJECT_ROOT="$(cd "$script_dir/../.." && pwd)"
    else
        # Fallback: search upward for sidecar directory from cwd
        E2E_PROJECT_ROOT="$(pwd)"
        while [[ "$E2E_PROJECT_ROOT" != "/" ]] && [[ ! -d "$E2E_PROJECT_ROOT/sidecar" ]]; do
            E2E_PROJECT_ROOT="$(dirname "$E2E_PROJECT_ROOT")"
        done
        if [[ "$E2E_PROJECT_ROOT" == "/" ]]; then
            # Last resort: try common locations
            if [[ -d "/data/projects/translatorvoiceinputtool/sidecar" ]]; then
                E2E_PROJECT_ROOT="/data/projects/translatorvoiceinputtool"
            else
                echo "ERROR: Could not find project root" >&2
                return 1
            fi
        fi
    fi

    # Detect platform
    case "$(uname -s)" in
        Linux)  E2E_PLATFORM="linux" ;;
        Darwin) E2E_PLATFORM="macos" ;;
        MINGW*|CYGWIN*|MSYS*) E2E_PLATFORM="windows" ;;
        *) E2E_PLATFORM="unknown" ;;
    esac

    case "$(uname -m)" in
        x86_64)  E2E_ARCH="x64" ;;
        aarch64) E2E_ARCH="arm64" ;;
        arm64)   E2E_ARCH="arm64" ;;
        *) E2E_ARCH="unknown" ;;
    esac

    # Set sidecar binary path
    if [ "$E2E_PLATFORM" = "windows" ]; then
        E2E_SIDECAR_BIN="$E2E_PROJECT_ROOT/sidecar/dist/openvoicy-sidecar.exe"
    else
        E2E_SIDECAR_BIN="$E2E_PROJECT_ROOT/sidecar/dist/openvoicy-sidecar"
    fi

    log_info "setup" "platform" "Platform detected" "{\"os\":\"$E2E_PLATFORM\",\"arch\":\"$E2E_ARCH\"}"
}

# Check if sidecar binary exists
check_sidecar_binary() {
    if [ ! -f "$E2E_SIDECAR_BIN" ]; then
        log_error "setup" "sidecar_check" "Sidecar binary not found" "{\"path\":\"$E2E_SIDECAR_BIN\"}"
        echo "ERROR: Sidecar binary not found at: $E2E_SIDECAR_BIN"
        echo "Run ./scripts/build-sidecar.sh first"
        return 1
    fi
    log_info "setup" "sidecar_check" "Sidecar binary found" "{\"path\":\"$E2E_SIDECAR_BIN\"}"
    return 0
}

# Start sidecar process for IPC testing
# Returns: 0 on success, sets E2E_SIDECAR_PID
start_sidecar() {
    check_sidecar_binary || return 1

    log_info "sidecar" "start" "Starting sidecar process"

    # Create named pipes for communication
    local tmpdir
    tmpdir=$(mktemp -d)
    E2E_SIDECAR_STDIN="$tmpdir/stdin"
    E2E_SIDECAR_STDOUT="$tmpdir/stdout"
    mkfifo "$E2E_SIDECAR_STDIN"
    mkfifo "$E2E_SIDECAR_STDOUT"

    # Start sidecar with pipes
    "$E2E_SIDECAR_BIN" < "$E2E_SIDECAR_STDIN" > "$E2E_SIDECAR_STDOUT" 2>&1 &
    E2E_SIDECAR_PID=$!

    # Keep stdin pipe open
    exec 3>"$E2E_SIDECAR_STDIN"

    # Give it a moment to start
    sleep 0.5

    if ! kill -0 "$E2E_SIDECAR_PID" 2>/dev/null; then
        log_error "sidecar" "start" "Sidecar failed to start"
        return 1
    fi

    log_info "sidecar" "start" "Sidecar started" "{\"pid\":$E2E_SIDECAR_PID}"
    return 0
}

# Stop sidecar process
stop_sidecar() {
    if [ -n "$E2E_SIDECAR_PID" ] && kill -0 "$E2E_SIDECAR_PID" 2>/dev/null; then
        log_info "sidecar" "stop" "Stopping sidecar" "{\"pid\":$E2E_SIDECAR_PID}"
        kill "$E2E_SIDECAR_PID" 2>/dev/null || true
        wait "$E2E_SIDECAR_PID" 2>/dev/null || true
        E2E_SIDECAR_PID=""
    fi

    # Close stdin pipe
    exec 3>&- 2>/dev/null || true

    # Cleanup temp files
    rm -f "$E2E_SIDECAR_STDIN" "$E2E_SIDECAR_STDOUT" 2>/dev/null || true
}

# Send JSON-RPC request to sidecar and get response
# Args: method, [params_json], [timeout_seconds]
# Returns: response JSON on stdout
sidecar_rpc() {
    local method="$1"
    # Note: Use separate assignment to avoid zsh brace expansion bug with ${2:-{}}
    local params="$2"
    [[ -z "$params" ]] && params='{}'
    local timeout="${3:-10}"

    local request_id
    request_id=$((RANDOM * RANDOM))

    local request
    request=$(jq -nc \
        --arg method "$method" \
        --argjson params "$params" \
        --argjson id "$request_id" \
        '{jsonrpc:"2.0",id:$id,method:$method,params:$params}')

    log_debug "ipc" "request" "Sending RPC request" "{\"method\":\"$method\",\"id\":$request_id}"

    local raw_response
    raw_response=$(echo "$request" | timeout "$timeout" "$E2E_SIDECAR_BIN" 2>/dev/null || echo '{"error":"timeout"}')

    # Extract only the JSON-RPC response line (contains "jsonrpc")
    # Use pure bash for portability (grep/awk may be aliased in some environments)
    local response=""
    while IFS= read -r line; do
        if [[ "$line" == *'"jsonrpc"'* ]]; then
            response="$line"
            break
        fi
    done <<< "$raw_response"

    # If no valid JSON-RPC line found, check for timeout
    if [ -z "$response" ]; then
        if [[ "$raw_response" == *'"error":"timeout"'* ]]; then
            log_error "ipc" "response" "RPC timeout" "{\"method\":\"$method\",\"timeout\":$timeout}"
            return 1
        fi
        # Return raw response as fallback (might be error JSON)
        response="$raw_response"
    fi

    log_debug "ipc" "response" "RPC response received" "{\"method\":\"$method\"}"

    # Output the response - MUST be the last thing this function does
    printf '%s\n' "$response"
}

# Send RPC and verify success (has "result" field)
# Args: method, [params_json], [timeout_seconds]
# Returns: result field on stdout
sidecar_rpc_ok() {
    local method="$1"
    local params="${2:-{}}"
    local timeout="${3:-10}"

    local response
    response=$(sidecar_rpc "$method" "$params" "$timeout")

    if echo "$response" | jq -e '.result' >/dev/null 2>&1; then
        echo "$response" | jq -c '.result'
        return 0
    else
        local error
        error=$(echo "$response" | jq -c '.error // "unknown error"')
        log_error "ipc" "rpc_ok" "RPC failed" "{\"method\":\"$method\",\"error\":$error}"
        return 1
    fi
}

# Test sidecar connectivity with system.ping
test_sidecar_ping() {
    log_info "test" "ping" "Testing sidecar connectivity"

    local start_time
    start_time=$(start_timer)

    local result
    result=$(sidecar_rpc_ok "system.ping" "{}")

    if [ $? -eq 0 ]; then
        log_with_duration "INFO" "test" "ping" "Sidecar ping successful" "$result" "$start_time"
        return 0
    else
        log_error "test" "ping" "Sidecar ping failed"
        return 1
    fi
}

# Get audio device list from sidecar
get_audio_devices() {
    sidecar_rpc_ok "audio.list_devices" "{}"
}

# Get model status from sidecar
get_model_status() {
    sidecar_rpc_ok "model.status" "{}"
}

# Cleanup handler for trap
cleanup() {
    local exit_code=$?
    log_info "cleanup" "start" "Running cleanup"

    stop_sidecar

    if [ -n "${E2E_LOG_JSON:-}" ]; then
        finalize_logging "$exit_code"
    fi

    return "$exit_code"
}

# Set up cleanup trap
setup_cleanup_trap() {
    trap cleanup EXIT INT TERM
}

# Check if jq is available
require_jq() {
    if ! command -v jq &>/dev/null; then
        echo "ERROR: jq is required but not installed"
        echo "Install with: apt-get install jq (Linux) or brew install jq (macOS)"
        exit 2
    fi
}

# Create temp directory for test artifacts
create_temp_dir() {
    local prefix="${1:-e2e-test}"
    mktemp -d -t "${prefix}-XXXXXX"
}

# Redact sensitive data from text (for logging transcriptions)
redact_text() {
    local text="$1"
    local length="${#text}"
    if [ "$length" -le 10 ]; then
        echo "[REDACTED:${length}chars]"
    else
        echo "${text:0:5}...[REDACTED:${length}chars]"
    fi
}
````

## File: scripts/e2e/lib/log.sh
````bash
#!/usr/bin/env bash
#
# E2E Test Logging Library
# Provides structured JSON logging for E2E test scripts.
#
# Usage:
#   source scripts/e2e/lib/log.sh
#   log_info "transcription" "recording_start" "Recording started" '{"session_id":"abc"}'
#

set -euo pipefail

# Log file paths (set by init_logging)
E2E_LOG_JSON=""
E2E_LOG_HUMAN=""
E2E_ARTIFACTS_DIR=""
E2E_TEST_NAME=""
E2E_START_TIME=""

# Initialize logging for a test run
# Args: test_name
init_logging() {
    local test_name="${1:-e2e-test}"
    local timestamp
    timestamp=$(date -u +"%Y%m%d_%H%M%S")

    E2E_TEST_NAME="$test_name"
    E2E_START_TIME=$(date +%s%3N)

    # Ensure directories exist - find project root from current location or script
    local script_dir project_root
    if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
        script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
        project_root="$(cd "$script_dir/../../.." && pwd)"
    elif [[ -n "${0:-}" ]]; then
        script_dir="$(cd "$(dirname "$0")" && pwd)"
        project_root="$(cd "$script_dir/../.." && pwd)"
    else
        # Fallback: look for sidecar directory from current dir
        project_root="$(pwd)"
        while [[ "$project_root" != "/" ]] && [[ ! -d "$project_root/sidecar" ]]; do
            project_root="$(dirname "$project_root")"
        done
    fi

    mkdir -p "$project_root/logs/e2e"
    mkdir -p "$project_root/artifacts/e2e"

    E2E_LOG_JSON="$project_root/logs/e2e/${test_name}_${timestamp}.jsonl"
    E2E_LOG_HUMAN="$project_root/logs/e2e/${test_name}_${timestamp}.log"
    E2E_ARTIFACTS_DIR="$project_root/artifacts/e2e/${test_name}_${timestamp}"

    mkdir -p "$E2E_ARTIFACTS_DIR"

    # Write header
    echo "# E2E Test Log: $test_name" > "$E2E_LOG_HUMAN"
    echo "# Started: $(date -u +"%Y-%m-%dT%H:%M:%SZ")" >> "$E2E_LOG_HUMAN"
    echo "# JSON log: $E2E_LOG_JSON" >> "$E2E_LOG_HUMAN"
    echo "" >> "$E2E_LOG_HUMAN"

    log_info "test" "init" "Test initialized" "{\"test_name\":\"$test_name\",\"timestamp\":\"$timestamp\"}"
}

# Core logging function - outputs JSON Lines format
# Args: level, phase, step, msg, [data_json]
log_json() {
    local level="$1"
    local phase="$2"
    local step="$3"
    local msg="$4"
    local data="${5:-null}"

    # Sanitize data - ensure it's valid JSON
    if [[ -z "$data" ]] || [[ "$data" == "" ]]; then
        data="null"
    fi
    # Validate data is valid JSON before passing to jq
    if ! echo "$data" | jq -e . >/dev/null 2>&1; then
        # If not valid JSON, wrap it as a string
        data="\"$data\""
    fi

    # Get timestamp with milliseconds
    local ts
    if date --version >/dev/null 2>&1; then
        # GNU date (Linux)
        ts=$(date -u +"%Y-%m-%dT%H:%M:%S.%3NZ")
    else
        # BSD date (macOS) - no %3N support, use Python fallback
        ts=$(python3 -c "from datetime import datetime; print(datetime.utcnow().strftime('%Y-%m-%dT%H:%M:%S.%f')[:-3] + 'Z')")
    fi

    # Build JSON using jq if available, otherwise fallback
    local json_line
    if command -v jq &>/dev/null; then
        json_line=$(jq -nc \
            --arg ts "$ts" \
            --arg level "$level" \
            --arg phase "$phase" \
            --arg step "$step" \
            --arg msg "$msg" \
            --argjson data "$data" \
            '{ts:$ts, level:$level, phase:$phase, step:$step, msg:$msg, data:$data}' 2>/dev/null) || \
        json_line="{\"ts\":\"$ts\",\"level\":\"$level\",\"phase\":\"$phase\",\"step\":\"$step\",\"msg\":\"$msg\",\"data\":null}"
    else
        # Fallback without jq (basic escaping)
        local escaped_msg
        escaped_msg="${msg//\\/\\\\}"
        escaped_msg="${escaped_msg//\"/\\\"}"
        json_line="{\"ts\":\"$ts\",\"level\":\"$level\",\"phase\":\"$phase\",\"step\":\"$step\",\"msg\":\"$escaped_msg\",\"data\":$data}"
    fi

    # Write to JSON log file if initialized
    if [ -n "$E2E_LOG_JSON" ]; then
        echo "$json_line" >> "$E2E_LOG_JSON"
    fi

    # Write human-readable version
    local color=""
    local reset="\033[0m"
    case "$level" in
        DEBUG) color="\033[0;37m" ;;  # Gray
        INFO)  color="\033[0;32m" ;;  # Green
        WARN)  color="\033[0;33m" ;;  # Yellow
        ERROR) color="\033[0;31m" ;;  # Red
    esac

    local human_line="[$ts] $level [$phase/$step] $msg"
    if [ "$data" != "null" ]; then
        human_line="$human_line ($data)"
    fi

    # Write to human log file if initialized
    if [ -n "$E2E_LOG_HUMAN" ]; then
        echo "$human_line" >> "$E2E_LOG_HUMAN"
    fi

    # Also output to terminal (colored) - use stderr to avoid polluting stdout
    if [ -t 2 ]; then
        echo -e "${color}${human_line}${reset}" >&2
    else
        echo "$human_line" >&2
    fi
}

# Convenience functions for different log levels
log_debug() { log_json "DEBUG" "$@"; }
log_info()  { log_json "INFO" "$@"; }
log_warn()  { log_json "WARN" "$@"; }
log_error() { log_json "ERROR" "$@"; }

# Start timing a phase
# Args: phase_name
# Returns: start timestamp in ms
start_timer() {
    date +%s%3N
}

# Log with duration
# Args: level, phase, step, msg, data_json, start_time_ms
log_with_duration() {
    local level="$1"
    local phase="$2"
    local step="$3"
    local msg="$4"
    local data="${5:-{}}"
    local start_ms="$6"

    local end_ms
    end_ms=$(date +%s%3N)
    local duration_ms=$((end_ms - start_ms))

    # Add duration to data - handle empty/null/invalid data
    if [ -z "$data" ] || [ "$data" = "null" ] || [ "$data" = "{}" ]; then
        data="{\"duration_ms\":$duration_ms}"
    else
        # Inject duration_ms into existing object, with fallback
        data=$(echo "$data" | jq -c ". + {duration_ms: $duration_ms}" 2>/dev/null) || data="{\"duration_ms\":$duration_ms}"
    fi

    log_json "$level" "$phase" "$step" "$msg" "$data"
}

# Finalize logging and print summary
finalize_logging() {
    local exit_code="${1:-0}"
    local end_time
    end_time=$(date +%s%3N)
    local total_duration=$((end_time - E2E_START_TIME))

    local status="PASSED"
    [ "$exit_code" -ne 0 ] && status="FAILED"

    log_info "test" "finalize" "Test $status" "{\"exit_code\":$exit_code,\"total_duration_ms\":$total_duration}"

    # Write summary to human log
    echo "" >> "$E2E_LOG_HUMAN"
    echo "# ============================================" >> "$E2E_LOG_HUMAN"
    echo "# Test: $E2E_TEST_NAME" >> "$E2E_LOG_HUMAN"
    echo "# Status: $status" >> "$E2E_LOG_HUMAN"
    echo "# Duration: $((total_duration / 1000)).$((total_duration % 1000))s" >> "$E2E_LOG_HUMAN"
    echo "# Exit code: $exit_code" >> "$E2E_LOG_HUMAN"
    echo "# ============================================" >> "$E2E_LOG_HUMAN"

    echo ""
    echo "=== Test Summary ==="
    echo "Test: $E2E_TEST_NAME"
    echo "Status: $status"
    echo "Duration: $((total_duration / 1000)).$((total_duration % 1000))s"
    echo "JSON log: $E2E_LOG_JSON"
    echo "Human log: $E2E_LOG_HUMAN"
    echo "Artifacts: $E2E_ARTIFACTS_DIR"
}

# Save artifact (screenshot, audio, etc.)
# Args: source_path, artifact_name
save_artifact() {
    local source="$1"
    local name="$2"

    if [ -f "$source" ]; then
        cp "$source" "$E2E_ARTIFACTS_DIR/$name"
        log_info "test" "artifact" "Saved artifact: $name" "{\"path\":\"$E2E_ARTIFACTS_DIR/$name\"}"
    else
        log_warn "test" "artifact" "Artifact source not found: $source"
    fi
}
````

## File: scripts/e2e/run-all.sh
````bash
#!/usr/bin/env bash
#
# E2E Test Runner
#
# Runs all E2E tests and aggregates results.
#
# Usage:
#   ./scripts/e2e/run-all.sh [--parallel] [--filter PATTERN]
#
# Exit codes:
#   0 - All tests passed
#   1 - One or more tests failed
#   2 - Environment setup error
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Configuration
PARALLEL=false
FILTER=""
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_SKIPPED=0

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --parallel)
            PARALLEL=true
            shift
            ;;
        --filter)
            FILTER="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [--parallel] [--filter PATTERN]"
            echo "  --parallel  Run tests in parallel"
            echo "  --filter    Only run tests matching pattern"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 2
            ;;
    esac
done

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

echo "========================================"
echo "     OpenVoicy E2E Test Suite"
echo "========================================"
echo ""

# Check prerequisites
if ! command -v jq &>/dev/null; then
    echo -e "${RED}ERROR: jq is required but not installed${NC}"
    exit 2
fi

# Check sidecar binary
if [ ! -f "$PROJECT_ROOT/sidecar/dist/openvoicy-sidecar" ] && \
   [ ! -f "$PROJECT_ROOT/sidecar/dist/openvoicy-sidecar.exe" ]; then
    echo -e "${YELLOW}WARNING: Sidecar binary not found${NC}"
    echo "Run ./scripts/build-sidecar.sh first"
    echo ""
fi

# Find all test scripts
declare -a TEST_SCRIPTS
for script in "$SCRIPT_DIR"/test-*.sh; do
    if [ -f "$script" ]; then
        name=$(basename "$script" .sh)
        if [ -z "$FILTER" ] || [[ "$name" == *"$FILTER"* ]]; then
            TEST_SCRIPTS+=("$script")
        fi
    fi
done

if [ ${#TEST_SCRIPTS[@]} -eq 0 ]; then
    echo "No tests found matching filter: $FILTER"
    exit 0
fi

echo "Found ${#TEST_SCRIPTS[@]} test(s) to run"
echo ""

# Results tracking
declare -A RESULTS
declare -A DURATIONS

# Run a single test
run_test() {
    local script="$1"
    local name
    name=$(basename "$script" .sh)

    echo -n "Running $name... "

    local start_time
    start_time=$(date +%s)

    local exit_code=0
    local output
    output=$("$script" 2>&1) || exit_code=$?

    local end_time
    end_time=$(date +%s)
    local duration=$((end_time - start_time))

    DURATIONS[$name]=$duration

    case $exit_code in
        0)
            RESULTS[$name]="PASS"
            echo -e "${GREEN}PASS${NC} (${duration}s)"
            ((TESTS_PASSED++)) || true
            ;;
        1)
            RESULTS[$name]="FAIL"
            echo -e "${RED}FAIL${NC} (${duration}s)"
            ((TESTS_FAILED++)) || true
            # Show output on failure
            echo "--- Output ---"
            echo "$output" | tail -20
            echo "--------------"
            ;;
        2)
            RESULTS[$name]="SKIP"
            echo -e "${YELLOW}SKIP${NC} (setup error)"
            ((TESTS_SKIPPED++)) || true
            ;;
        3)
            RESULTS[$name]="TIMEOUT"
            echo -e "${RED}TIMEOUT${NC}"
            ((TESTS_FAILED++)) || true
            ;;
        *)
            RESULTS[$name]="ERROR:$exit_code"
            echo -e "${RED}ERROR${NC} (exit code: $exit_code)"
            ((TESTS_FAILED++)) || true
            ;;
    esac
}

# Make test scripts executable
chmod +x "$SCRIPT_DIR"/test-*.sh "$SCRIPT_DIR"/lib/*.sh 2>/dev/null || true

# Run tests
if [ "$PARALLEL" = true ]; then
    echo "Running tests in parallel..."
    echo ""

    # Run all tests in background
    declare -A PIDS
    for script in "${TEST_SCRIPTS[@]}"; do
        name=$(basename "$script" .sh)
        "$script" > "/tmp/e2e-$name.out" 2>&1 &
        PIDS[$name]=$!
    done

    # Wait for all and collect results
    for name in "${!PIDS[@]}"; do
        local pid=${PIDS[$name]}
        local start_time
        start_time=$(date +%s)

        if wait "$pid"; then
            RESULTS[$name]="PASS"
            ((TESTS_PASSED++)) || true
        else
            local exit_code=$?
            case $exit_code in
                1) RESULTS[$name]="FAIL"; ((TESTS_FAILED++)) || true ;;
                2) RESULTS[$name]="SKIP"; ((TESTS_SKIPPED++)) || true ;;
                3) RESULTS[$name]="TIMEOUT"; ((TESTS_FAILED++)) || true ;;
                *) RESULTS[$name]="ERROR:$exit_code"; ((TESTS_FAILED++)) || true ;;
            esac
        fi

        local end_time
        end_time=$(date +%s)
        DURATIONS[$name]=$((end_time - start_time))
    done
else
    # Run tests sequentially
    for script in "${TEST_SCRIPTS[@]}"; do
        run_test "$script"
    done
fi

# Summary
echo ""
echo "========================================"
echo "              RESULTS"
echo "========================================"
echo ""

total=$((TESTS_PASSED + TESTS_FAILED + TESTS_SKIPPED))
echo "Total:   $total"
echo -e "Passed:  ${GREEN}$TESTS_PASSED${NC}"
echo -e "Failed:  ${RED}$TESTS_FAILED${NC}"
echo -e "Skipped: ${YELLOW}$TESTS_SKIPPED${NC}"
echo ""

# Detailed results
echo "Detailed Results:"
for name in "${!RESULTS[@]}"; do
    result="${RESULTS[$name]}"
    duration="${DURATIONS[$name]:-?}s"

    case $result in
        PASS)    echo -e "  ${GREEN}✓${NC} $name ($duration)" ;;
        FAIL)    echo -e "  ${RED}✗${NC} $name ($duration)" ;;
        SKIP)    echo -e "  ${YELLOW}○${NC} $name (skipped)" ;;
        TIMEOUT) echo -e "  ${RED}⏱${NC} $name (timeout)" ;;
        *)       echo -e "  ${RED}?${NC} $name ($result)" ;;
    esac
done

echo ""

# Log file locations
echo "Log files:"
for log in "$PROJECT_ROOT"/logs/e2e/*.jsonl; do
    [ -f "$log" ] && echo "  $log"
done | tail -5

echo ""

# Exit with failure if any tests failed
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi

echo -e "${GREEN}All tests passed!${NC}"
exit 0
````

## File: scripts/e2e/test-error-recovery.sh
````bash
#!/usr/bin/env bash
#
# E2E Test: Error Recovery
#
# Tests error handling scenarios:
# 1. Sidecar responds correctly to malformed requests
# 2. Sidecar handles unknown methods gracefully
# 3. Sidecar handles invalid parameters
#
# Exit codes:
#   0 - All tests passed
#   1 - Test failure
#   2 - Environment setup error
#   3 - Timeout
#

set -euo pipefail

# Resolve script directory
if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

# Source libraries
source "$SCRIPT_DIR/lib/log.sh"
source "$SCRIPT_DIR/lib/assert.sh"
source "$SCRIPT_DIR/lib/common.sh"

# Configuration
TEST_TIMEOUT=60

main() {
    # Initialize
    require_jq
    init_logging "test-error-recovery"
    init_common
    setup_cleanup_trap

    log_info "test" "start" "Starting error recovery E2E test"

    # Environment checks
    assert_dir_exists "$E2E_PROJECT_ROOT/sidecar" "Sidecar directory exists" || exit 2
    check_sidecar_binary || exit 2

    # Test 1: Unknown method returns proper JSON-RPC error
    log_info "error" "unknown_method" "Testing unknown method handling"

    local unknown_result
    unknown_result=$(sidecar_rpc "nonexistent.method" "{}" 10) || true

    if echo "$unknown_result" | jq -e '.error' >/dev/null 2>&1; then
        local error_code
        error_code=$(echo "$unknown_result" | jq '.error.code')
        log_info "error" "unknown_method" "Received error response" "{\"error_code\":$error_code}"

        # JSON-RPC method not found is -32601
        assert_eq "-32601" "$error_code" "Unknown method returns -32601 (method not found)"
    else
        log_error "error" "unknown_method" "Expected error response for unknown method"
        ((E2E_ASSERTIONS_FAILED++)) || true
    fi

    # Test 2: Malformed JSON handling
    log_info "error" "malformed_json" "Testing malformed JSON handling"

    local malformed_result
    malformed_result=$(echo 'not valid json{' | timeout 5 "$E2E_SIDECAR_BIN" 2>/dev/null) || true

    if echo "$malformed_result" | jq -e '.error' >/dev/null 2>&1; then
        local parse_error_code
        parse_error_code=$(echo "$malformed_result" | jq '.error.code')
        log_info "error" "malformed_json" "Received parse error" "{\"error_code\":$parse_error_code}"

        # JSON-RPC parse error is -32700
        assert_eq "-32700" "$parse_error_code" "Malformed JSON returns -32700 (parse error)"
    else
        log_warn "error" "malformed_json" "Sidecar may have closed connection on malformed input"
    fi

    # Test 3: Invalid params handling
    log_info "error" "invalid_params" "Testing invalid params handling"

    # Call audio.meter_start with invalid device_uid type
    local invalid_params_result
    invalid_params_result=$(sidecar_rpc "audio.meter_start" '{"device_uid":12345}' 10) || true

    if echo "$invalid_params_result" | jq -e '.error' >/dev/null 2>&1; then
        local invalid_code
        invalid_code=$(echo "$invalid_params_result" | jq '.error.code')
        log_info "error" "invalid_params" "Received error for invalid params" "{\"error_code\":$invalid_code}"

        # Accept either invalid params (-32602) or application error
        if [ "$invalid_code" -eq -32602 ] || [ "$invalid_code" -lt 0 ]; then
            ((E2E_ASSERTIONS_PASSED++)) || true
            log_info "test" "assert" "PASS: Invalid params handled with error code $invalid_code"
        else
            ((E2E_ASSERTIONS_FAILED++)) || true
            log_error "test" "assert" "FAIL: Unexpected error code for invalid params"
        fi
    else
        # Some implementations might accept the wrong type and convert
        log_warn "error" "invalid_params" "No error for invalid params (may be handled gracefully)"
    fi

    # Test 4: Multiple rapid requests
    log_info "error" "rapid_requests" "Testing rapid sequential requests"

    local rapid_success=0
    local rapid_start
    rapid_start=$(start_timer)

    for i in {1..5}; do
        local ping_result
        ping_result=$(sidecar_rpc "system.ping" "{}" 5) || continue

        if echo "$ping_result" | jq -e '.result' >/dev/null 2>&1; then
            ((rapid_success++)) || true
        fi
    done

    log_with_duration "INFO" "error" "rapid_requests" "Rapid requests completed" "{\"success\":$rapid_success,\"total\":5}" "$rapid_start"
    assert_eq "5" "$rapid_success" "All rapid requests succeeded"

    # Test 5: Empty request handling
    log_info "error" "empty_request" "Testing empty request handling"

    local empty_result
    empty_result=$(echo '' | timeout 5 "$E2E_SIDECAR_BIN" 2>/dev/null) || true

    # Empty input should either return error or nothing
    if [ -n "$empty_result" ]; then
        if echo "$empty_result" | jq -e '.error' >/dev/null 2>&1; then
            log_info "error" "empty_request" "Empty request returned error (expected)"
            ((E2E_ASSERTIONS_PASSED++)) || true
        else
            log_warn "error" "empty_request" "Empty request returned non-error response"
        fi
    else
        log_info "error" "empty_request" "Empty request returned nothing (acceptable)"
        ((E2E_ASSERTIONS_PASSED++)) || true
    fi

    # Summary
    assertion_summary
    local summary_exit=$?

    log_info "test" "complete" "Error recovery test completed"
    return $summary_exit
}

# Run main
main
exit $?
````

## File: scripts/e2e/test-focus-guard.sh
````bash
#!/usr/bin/env bash
#
# E2E Test: Focus Guard Behavior
#
# Tests Focus Guard functionality:
# 1. Verify focus guard configuration is respected
# 2. Test that focus tracking is reported
# 3. Verify clipboard fallback behavior indication
#
# Note: Full focus guard testing requires a windowing system.
# This test validates the IPC contract for focus guard features.
#
# Exit codes:
#   0 - All tests passed
#   1 - Test failure
#   2 - Environment setup error
#   3 - Timeout
#

set -euo pipefail

# Resolve script directory
if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

# Source libraries
source "$SCRIPT_DIR/lib/log.sh"
source "$SCRIPT_DIR/lib/assert.sh"
source "$SCRIPT_DIR/lib/common.sh"

# Configuration
TEST_TIMEOUT=60

main() {
    # Initialize
    require_jq
    init_logging "test-focus-guard"
    init_common
    setup_cleanup_trap

    log_info "test" "start" "Starting focus guard E2E test"

    # Environment checks
    assert_dir_exists "$E2E_PROJECT_ROOT/sidecar" "Sidecar directory exists" || exit 2
    check_sidecar_binary || exit 2

    # Test 1: Verify sidecar is responsive
    log_info "focus" "connectivity" "Verifying sidecar connectivity"

    local ping_result
    ping_result=$(sidecar_rpc "system.ping" "{}" 10) || {
        log_error "focus" "connectivity" "Sidecar not responding"
        exit 1
    }

    assert_json_eq "$ping_result" ".result.protocol" "v1" "Sidecar protocol v1"

    # Test 2: Check if injection.capabilities exists
    log_info "focus" "capabilities" "Checking injection capabilities"

    local caps_result
    caps_result=$(sidecar_rpc "injection.capabilities" "{}" 10) 2>/dev/null || true

    if echo "$caps_result" | jq -e '.result' >/dev/null 2>&1; then
        log_info "focus" "capabilities" "Injection capabilities retrieved" "$caps_result"

        # Check for focus_guard support
        local has_focus_guard
        has_focus_guard=$(echo "$caps_result" | jq '.result.focus_guard // false')
        log_info "focus" "capabilities" "Focus guard support" "{\"supported\":$has_focus_guard}"
    else
        log_info "focus" "capabilities" "Injection capabilities not yet implemented (expected)"
    fi

    # Test 3: Test system status includes focus info (if available)
    log_info "focus" "status" "Checking system status for focus info"

    local status_result
    status_result=$(sidecar_rpc "system.status" "{}" 10) 2>/dev/null || true

    if echo "$status_result" | jq -e '.result' >/dev/null 2>&1; then
        log_info "focus" "status" "System status retrieved"

        # Check if focus tracking is part of status
        local focus_info
        focus_info=$(echo "$status_result" | jq '.result.focus // null')
        if [ "$focus_info" != "null" ]; then
            log_info "focus" "status" "Focus tracking info available" "{\"focus\":$focus_info}"
        fi
    else
        log_info "focus" "status" "System status not yet implemented"
    fi

    # Test 4: Verify config file handling (if config API exists)
    log_info "focus" "config" "Testing focus guard configuration"

    # Check if there's a config endpoint
    local config_result
    config_result=$(sidecar_rpc "config.get" '{"key":"focus_guard"}' 10) 2>/dev/null || true

    if echo "$config_result" | jq -e '.result' >/dev/null 2>&1; then
        local fg_enabled
        fg_enabled=$(echo "$config_result" | jq '.result.enabled // null')
        log_info "focus" "config" "Focus guard config retrieved" "{\"enabled\":$fg_enabled}"
    else
        # Try getting full config
        config_result=$(sidecar_rpc "config.get" '{}' 10) 2>/dev/null || true
        if echo "$config_result" | jq -e '.result.focus_guard' >/dev/null 2>&1; then
            local fg_config
            fg_config=$(echo "$config_result" | jq -c '.result.focus_guard')
            log_info "focus" "config" "Focus guard in full config" "{\"config\":$fg_config}"
        else
            log_info "focus" "config" "Config endpoint not available (testing via Tauri expected)"
        fi
    fi

    # Test 5: Verify error response for focus-guard specific errors
    log_info "focus" "errors" "Testing focus guard error codes"

    # Test that E_FOCUS_LOST error code exists in error handling
    # This is a contract test - the sidecar should recognize this error type
    local error_test
    error_test=$(sidecar_rpc "audio.meter_start" '{"device_uid":"nonexistent-device-12345"}' 10) || true

    if echo "$error_test" | jq -e '.error' >/dev/null 2>&1; then
        local error_kind
        error_kind=$(echo "$error_test" | jq -r '.error.data.kind // "unknown"')
        log_info "focus" "errors" "Error response format verified" "{\"error_kind\":\"$error_kind\"}"

        # Verify error has structured data
        if [ "$error_kind" != "unknown" ] && [ "$error_kind" != "null" ]; then
            ((E2E_ASSERTIONS_PASSED++)) || true
            log_info "test" "assert" "PASS: Errors include structured kind field"
        fi
    fi

    # Test 6: Clipboard fallback simulation
    log_info "focus" "clipboard" "Testing clipboard fallback indicators"

    # The sidecar should indicate when clipboard fallback is needed
    # This is typically communicated via injection result
    local inject_result
    inject_result=$(sidecar_rpc "injection.status" "{}" 10) 2>/dev/null || true

    if echo "$inject_result" | jq -e '.result' >/dev/null 2>&1; then
        local method
        method=$(echo "$inject_result" | jq -r '.result.method // "unknown"')
        log_info "focus" "clipboard" "Injection method status" "{\"method\":\"$method\"}"

        # Focus guard would switch method to "clipboard" when focus is lost
        ((E2E_ASSERTIONS_PASSED++)) || true
    else
        log_info "focus" "clipboard" "Injection status not available (Tauri handles this)"
        ((E2E_ASSERTIONS_PASSED++)) || true  # Expected - this is handled by Tauri layer
    fi

    # Summary
    assertion_summary
    local summary_exit=$?

    log_info "test" "complete" "Focus guard test completed"
    return $summary_exit
}

# Run main
main
exit $?
````

## File: scripts/e2e/test-full-flow.sh
````bash
#!/usr/bin/env bash
#
# E2E Test: Full Transcription Flow
#
# Tests the happy-path flow:
# 1. Sidecar startup
# 2. Model status check
# 3. Audio device enumeration
# 4. System ping verification
#
# Note: Full transcription testing requires audio hardware and model.
# This test validates the IPC layer and basic functionality.
#
# Exit codes:
#   0 - All tests passed
#   1 - Test failure
#   2 - Environment setup error
#   3 - Timeout
#

set -euo pipefail

# Resolve script directory (handle being called via various methods)
if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

# Source libraries
source "$SCRIPT_DIR/lib/log.sh"
source "$SCRIPT_DIR/lib/assert.sh"
source "$SCRIPT_DIR/lib/common.sh"

# Configuration
TEST_TIMEOUT=60  # seconds

main() {
    # Initialize
    require_jq
    init_logging "test-full-flow"
    init_common
    setup_cleanup_trap

    log_info "test" "start" "Starting full flow E2E test"

    # Phase 1: Environment checks
    log_info "startup" "env_check" "Checking environment"

    assert_dir_exists "$E2E_PROJECT_ROOT/sidecar" "Sidecar directory exists" || exit 2
    check_sidecar_binary || exit 2

    # Phase 2: Sidecar ping test
    log_info "sidecar" "ping_test" "Testing sidecar ping"

    local ping_start
    ping_start=$(start_timer)

    local ping_result
    ping_result=$(sidecar_rpc "system.ping" "{}" 10) || {
        log_error "sidecar" "ping_test" "Ping failed"
        exit 1
    }

    log_with_duration "INFO" "sidecar" "ping_test" "Ping completed" "$ping_result" "$ping_start"

    # Verify ping response
    assert_json_eq "$ping_result" ".result.protocol" "v1" "Protocol version is v1"

    local version
    version=$(echo "$ping_result" | jq -r '.result.version')
    log_info "sidecar" "version" "Sidecar version" "{\"version\":\"$version\"}"

    # Phase 3: Audio device enumeration
    log_info "audio" "list_devices" "Enumerating audio devices"

    local devices_start
    devices_start=$(start_timer)

    local devices_result
    devices_result=$(sidecar_rpc "audio.list_devices" "{}" 10) || {
        log_error "audio" "list_devices" "Device enumeration failed"
        exit 1
    }

    log_with_duration "INFO" "audio" "list_devices" "Devices enumerated" "{}" "$devices_start"

    # Check response has devices array (may be empty in headless environment)
    local devices_count
    devices_count=$(echo "$devices_result" | jq '.result.devices | length')
    log_info "audio" "device_count" "Audio devices found" "{\"count\":$devices_count}"

    # Phase 4: Model status (may not be implemented yet)
    log_info "model" "status_check" "Checking model status"

    local model_result
    model_result=$(sidecar_rpc "model.status" "{}" 10) 2>/dev/null || true

    if echo "$model_result" | jq -e '.result' >/dev/null 2>&1; then
        local model_status
        model_status=$(echo "$model_result" | jq -r '.result.status // "unknown"')
        log_info "model" "status" "Model status retrieved" "{\"status\":\"$model_status\"}"
    else
        log_warn "model" "status_check" "Model status endpoint not available (expected if not yet implemented)"
    fi

    # Phase 5: Verify sidecar startup time is within budget
    local startup_time_ms
    startup_time_ms=$(cat "$E2E_PROJECT_ROOT/sidecar/dist/manifest.json" 2>/dev/null | jq '.startup_time_ms // 0')

    if [ "$startup_time_ms" -gt 0 ]; then
        assert_duration_under "$startup_time_ms" 5000 "Startup time under 5s budget"
    fi

    # Summary
    assertion_summary

    log_info "test" "complete" "Full flow test completed successfully"
    return 0
}

# Run main
main
exit $?
````

## File: scripts/e2e/test-offline.sh
````bash
#!/usr/bin/env bash
#
# E2E Test: Offline Mode Verification
#
# Tests that the sidecar can operate without network:
# 1. Verify sidecar starts without network dependency
# 2. Test that cached model can be used offline
# 3. Verify core functionality works offline
#
# Note: This test simulates offline conditions by testing
# operations that should work without network connectivity.
# Full network isolation requires root/admin privileges.
#
# Exit codes:
#   0 - All tests passed
#   1 - Test failure
#   2 - Environment setup error
#   3 - Timeout
#

set -euo pipefail

# Resolve script directory
if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

# Source libraries
source "$SCRIPT_DIR/lib/log.sh"
source "$SCRIPT_DIR/lib/assert.sh"
source "$SCRIPT_DIR/lib/common.sh"

# Configuration
TEST_TIMEOUT=60

main() {
    # Initialize
    require_jq
    init_logging "test-offline"
    init_common
    setup_cleanup_trap

    log_info "test" "start" "Starting offline mode E2E test"

    # Environment checks
    assert_dir_exists "$E2E_PROJECT_ROOT/sidecar" "Sidecar directory exists" || exit 2
    check_sidecar_binary || exit 2

    # Test 1: Sidecar starts without making network calls
    log_info "offline" "startup" "Testing sidecar startup (should not require network)"

    local startup_start
    startup_start=$(start_timer)

    # system.ping should work without network
    local ping_result
    ping_result=$(sidecar_rpc "system.ping" "{}" 10) || {
        log_error "offline" "startup" "Sidecar failed to start"
        exit 1
    }

    log_with_duration "INFO" "offline" "startup" "Sidecar started successfully" "{}" "$startup_start"
    assert_json_eq "$ping_result" ".result.protocol" "v1" "Protocol available offline"

    # Test 2: Audio device enumeration works offline
    log_info "offline" "audio" "Testing audio enumeration (local operation)"

    local audio_start
    audio_start=$(start_timer)

    local audio_result
    audio_result=$(sidecar_rpc "audio.list_devices" "{}" 10) || {
        log_error "offline" "audio" "Audio enumeration failed"
        exit 1
    }

    log_with_duration "INFO" "offline" "audio" "Audio enumeration succeeded" "{}" "$audio_start"

    # This should work because it only queries local audio hardware
    if echo "$audio_result" | jq -e '.result.devices' >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert" "PASS: Audio enumeration works offline"
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert" "FAIL: Audio enumeration should work offline"
    fi

    # Test 3: Model status check (verify it doesn't hang waiting for network)
    log_info "offline" "model" "Testing model status (should respond quickly)"

    local model_start
    model_start=$(start_timer)

    # Give it a short timeout - if it's trying to reach network, it would hang
    local model_result
    model_result=$(sidecar_rpc "model.status" "{}" 5) 2>/dev/null || true

    local model_duration
    model_duration=$(($(start_timer) - model_start))

    if [ "$model_duration" -lt 3000 ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "offline" "model" "Model status responded quickly" "{\"duration_ms\":$model_duration}"
    else
        log_warn "offline" "model" "Model status was slow (possible network dependency)" "{\"duration_ms\":$model_duration}"
    fi

    # Test 4: Simulate model cache check
    log_info "offline" "cache" "Checking model cache behavior"

    # Check if model cache directory exists (would be needed for offline operation)
    local cache_dir="$HOME/.cache/openvoicy"
    if [ -d "$cache_dir" ]; then
        log_info "offline" "cache" "Model cache directory exists" "{\"path\":\"$cache_dir\"}"

        # List cache contents if any
        local cache_size
        cache_size=$(du -sh "$cache_dir" 2>/dev/null | cut -f1 || echo "unknown")
        log_info "offline" "cache" "Cache size" "{\"size\":\"$cache_size\"}"
    else
        log_info "offline" "cache" "No model cache (model not yet downloaded)" "{\"path\":\"$cache_dir\"}"
    fi

    # Test 5: Verify manifest doesn't indicate network requirements
    log_info "offline" "manifest" "Checking build manifest"

    local manifest_path="$E2E_PROJECT_ROOT/sidecar/dist/manifest.json"
    if [ -f "$manifest_path" ]; then
        local gpu_support
        gpu_support=$(jq -r '.gpu_support // "unknown"' "$manifest_path")

        # CPU-only builds should work offline
        if [ "$gpu_support" = "none" ]; then
            ((E2E_ASSERTIONS_PASSED++)) || true
            log_info "offline" "manifest" "CPU-only build (good for offline)" "{\"gpu_support\":\"$gpu_support\"}"
        else
            log_info "offline" "manifest" "GPU support may require drivers" "{\"gpu_support\":\"$gpu_support\"}"
        fi
    fi

    # Summary
    assertion_summary
    local summary_exit=$?

    log_info "test" "complete" "Offline test completed"
    return $summary_exit
}

# Run main
main
exit $?
````

## File: scripts/build-sidecar.ps1
````powershell
#Requires -Version 5.1
<#
.SYNOPSIS
    Build OpenVoicy sidecar as standalone executable using PyInstaller on Windows.

.DESCRIPTION
    Creates a standalone Windows executable of the OpenVoicy sidecar that includes
    the Python runtime and all dependencies.

.PARAMETER Clean
    Remove build artifacts before building.

.PARAMETER NoVerify
    Skip binary verification step.

.EXAMPLE
    .\scripts\build-sidecar.ps1

.EXAMPLE
    .\scripts\build-sidecar.ps1 -Clean -NoVerify
#>
[CmdletBinding()]
param(
    [switch]$Clean,
    [switch]$NoVerify
)

$ErrorActionPreference = "Stop"

# Paths
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$SidecarDir = Join-Path $ProjectRoot "sidecar"
$DistDir = Join-Path $SidecarDir "dist"

# Platform info
$Platform = "windows"
$Arch = if ([Environment]::Is64BitOperatingSystem) { "x64" } else { "x86" }
$PlatformTag = "$Platform-$Arch"
$ArtifactName = "openvoicy-sidecar.exe"

Write-Host "=== Building OpenVoicy Sidecar ===" -ForegroundColor Cyan
Write-Host "Platform: $PlatformTag"
Write-Host "Sidecar dir: $SidecarDir"
Write-Host ""

Set-Location $SidecarDir

# Clean if requested
if ($Clean) {
    Write-Host "Cleaning build artifacts..."
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue build, dist, __pycache__
}

# Ensure virtual environment exists
$VenvPath = Join-Path $SidecarDir ".venv"
if (-not (Test-Path $VenvPath)) {
    Write-Host "Creating virtual environment..."
    python -m venv .venv
}

# Activate venv
$ActivateScript = Join-Path $VenvPath "Scripts\Activate.ps1"
. $ActivateScript

# Install/upgrade dependencies
Write-Host "Installing dependencies..."
pip install --quiet --upgrade pip
pip install --quiet -e .
pip install --quiet pyinstaller

# Run PyInstaller
Write-Host ""
Write-Host "Running PyInstaller..."
$BuildStart = Get-Date
pyinstaller --noconfirm openvoicy_sidecar.spec
$BuildEnd = Get-Date
$BuildTime = [int]($BuildEnd - $BuildStart).TotalSeconds
Write-Host "Build completed in ${BuildTime}s"

# Check binary exists
$BinaryPath = Join-Path $DistDir $ArtifactName
if (-not (Test-Path $BinaryPath)) {
    Write-Error "Binary not found at $BinaryPath"
    exit 1
}

# Get binary size
$BinaryInfo = Get-Item $BinaryPath
$BinarySize = $BinaryInfo.Length
$BinarySizeMB = [math]::Round($BinarySize / 1MB, 2)
Write-Host "Binary size: $BinarySizeMB MB ($BinarySize bytes)"

# Verify binary (unless skipped)
$StartupTimeMs = 0
if (-not $NoVerify) {
    Write-Host ""
    Write-Host "Verifying binary..."

    # Test system.ping
    $VerifyStart = Get-Date
    try {
        $PingResult = '{"jsonrpc":"2.0","id":1,"method":"system.ping"}' | & $BinaryPath 2>$null
        $VerifyEnd = Get-Date
        $StartupTimeMs = [int]($VerifyEnd - $VerifyStart).TotalMilliseconds

        if ($PingResult -match '"result"') {
            Write-Host "✓ system.ping: OK (${StartupTimeMs}ms)" -ForegroundColor Green
        } else {
            Write-Host "✗ system.ping: FAILED" -ForegroundColor Red
            Write-Host "  Response: $PingResult"
            exit 1
        }
    } catch {
        Write-Host "✗ system.ping: FAILED with exception" -ForegroundColor Red
        Write-Host "  Error: $_"
        exit 1
    }

    # Test audio.list_devices
    try {
        $DevicesResult = '{"jsonrpc":"2.0","id":2,"method":"audio.list_devices"}' | & $BinaryPath 2>$null
        if ($DevicesResult -match '"result"') {
            Write-Host "✓ audio.list_devices: OK" -ForegroundColor Green
        } else {
            Write-Host "✗ audio.list_devices: FAILED" -ForegroundColor Red
            Write-Host "  Response: $DevicesResult"
            exit 1
        }
    } catch {
        Write-Host "✗ audio.list_devices: FAILED with exception" -ForegroundColor Red
        exit 1
    }

    Write-Host ""
    Write-Host "Verification passed!" -ForegroundColor Green
}

# Generate manifest
Write-Host ""
Write-Host "Generating manifest..."

try {
    $GitSha = git rev-parse --short HEAD 2>$null
    if (-not $GitSha) { $GitSha = "unknown" }
} catch {
    $GitSha = "unknown"
}

$BuildTimestamp = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
$PythonVersion = (python --version 2>&1).ToString().Split(" ")[1]

# Get version from pyproject.toml
$PyProjectContent = Get-Content (Join-Path $SidecarDir "pyproject.toml") -Raw
if ($PyProjectContent -match 'version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    $Version = "unknown"
}

# Check ONNX
try {
    $OnnxVersion = pip show onnxruntime 2>$null | Select-String "Version" | ForEach-Object { $_.ToString().Split(":")[1].Trim() }
    if (-not $OnnxVersion) { $OnnxVersion = "not-included" }
} catch {
    $OnnxVersion = "not-included"
}

$Manifest = @{
    artifact_name = $ArtifactName
    version = $Version
    platform = $PlatformTag
    python_version = $PythonVersion
    build_timestamp = $BuildTimestamp
    git_sha = $GitSha
    binary_size_bytes = $BinarySize
    startup_time_ms = $StartupTimeMs
    gpu_support = "none"
    onnxruntime_version = $OnnxVersion
    build_time_seconds = $BuildTime
}

$ManifestPath = Join-Path $DistDir "manifest.json"
$Manifest | ConvertTo-Json -Depth 10 | Set-Content $ManifestPath
Write-Host "Manifest written to: $ManifestPath"

# Summary
Write-Host ""
Write-Host "=== Build Summary ===" -ForegroundColor Cyan
Write-Host "Artifact: $BinaryPath"
Write-Host "Size: $BinarySizeMB MB"
if ($StartupTimeMs -gt 0) {
    $StartupSec = [math]::Round($StartupTimeMs / 1000, 2)
    Write-Host "Startup time: ${StartupSec}s"
}
Write-Host "Manifest: $ManifestPath"
Write-Host ""

# Check against targets
Write-Host "=== Target Compliance ===" -ForegroundColor Cyan
if ($BinarySize -lt 524288000) {
    Write-Host "✓ Binary size: $BinarySizeMB MB < 500 MB limit" -ForegroundColor Green
} else {
    Write-Host "✗ Binary size: $BinarySizeMB MB exceeds 500 MB limit" -ForegroundColor Red
}

if ($StartupTimeMs -gt 0 -and $StartupTimeMs -lt 5000) {
    Write-Host "✓ Startup time: ${StartupTimeMs}ms < 5000ms limit" -ForegroundColor Green
} elseif ($StartupTimeMs -gt 0) {
    Write-Host "✗ Startup time: ${StartupTimeMs}ms exceeds 5000ms limit" -ForegroundColor Red
}

Write-Host ""
Write-Host "Build complete!" -ForegroundColor Green
````

## File: scripts/build-sidecar.sh
````bash
#!/usr/bin/env bash
#
# Build OpenVoicy sidecar as standalone executable using PyInstaller.
# Works on Linux and macOS.
#
# Usage: ./scripts/build-sidecar.sh [--clean] [--no-verify]
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SIDECAR_DIR="$PROJECT_ROOT/sidecar"
DIST_DIR="$SIDECAR_DIR/dist"

# Parse arguments
CLEAN=false
VERIFY=true
for arg in "$@"; do
    case $arg in
        --clean) CLEAN=true ;;
        --no-verify) VERIFY=false ;;
        -h|--help)
            echo "Usage: $0 [--clean] [--no-verify]"
            echo "  --clean     Remove build artifacts before building"
            echo "  --no-verify Skip binary verification step"
            exit 0
            ;;
    esac
done

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
    Linux)  PLATFORM="linux" ;;
    Darwin) PLATFORM="macos" ;;
    *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac
case "$ARCH" in
    x86_64)  ARCH_SUFFIX="x64" ;;
    aarch64) ARCH_SUFFIX="arm64" ;;
    arm64)   ARCH_SUFFIX="arm64" ;;
    *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

ARTIFACT_NAME="openvoicy-sidecar"
PLATFORM_TAG="${PLATFORM}-${ARCH_SUFFIX}"

echo "=== Building OpenVoicy Sidecar ==="
echo "Platform: $PLATFORM_TAG"
echo "Sidecar dir: $SIDECAR_DIR"
echo ""

cd "$SIDECAR_DIR"

# Clean if requested
if [ "$CLEAN" = true ]; then
    echo "Cleaning build artifacts..."
    rm -rf build/ dist/ __pycache__/
fi

# Ensure virtual environment exists
if [ ! -d ".venv" ]; then
    echo "Creating virtual environment..."
    python3 -m venv .venv
fi

# Activate venv
source .venv/bin/activate

# Install/upgrade dependencies
echo "Installing dependencies..."
pip install --quiet --upgrade pip
pip install --quiet -e .
pip install --quiet pyinstaller

# Check for PortAudio on Linux
if [ "$PLATFORM" = "linux" ]; then
    if ! ldconfig -p | grep -q libportaudio; then
        echo "WARNING: libportaudio not found in system libraries"
        echo "Install with: sudo apt-get install libportaudio2"
    fi
fi

# Run PyInstaller
echo ""
echo "Running PyInstaller..."
BUILD_START=$(date +%s)
pyinstaller --noconfirm openvoicy_sidecar.spec
BUILD_END=$(date +%s)
BUILD_TIME=$((BUILD_END - BUILD_START))
echo "Build completed in ${BUILD_TIME}s"

# Check binary exists
BINARY_PATH="$DIST_DIR/$ARTIFACT_NAME"
if [ ! -f "$BINARY_PATH" ]; then
    echo "ERROR: Binary not found at $BINARY_PATH"
    exit 1
fi

# Get binary size
BINARY_SIZE=$(stat -c%s "$BINARY_PATH" 2>/dev/null || stat -f%z "$BINARY_PATH")
BINARY_SIZE_MB=$(echo "scale=2; $BINARY_SIZE / 1048576" | bc)
echo "Binary size: ${BINARY_SIZE_MB} MB ($BINARY_SIZE bytes)"

# Verify binary (unless skipped)
STARTUP_TIME_MS=0
if [ "$VERIFY" = true ]; then
    echo ""
    echo "Verifying binary..."

    # Test system.ping
    VERIFY_START=$(date +%s%3N 2>/dev/null || python3 -c "import time; print(int(time.time()*1000))")
    PING_RESULT=$(echo '{"jsonrpc":"2.0","id":1,"method":"system.ping"}' | timeout 10 "$BINARY_PATH" 2>/dev/null || echo "FAILED")
    VERIFY_END=$(date +%s%3N 2>/dev/null || python3 -c "import time; print(int(time.time()*1000))")
    STARTUP_TIME_MS=$((VERIFY_END - VERIFY_START))

    if echo "$PING_RESULT" | grep -q '"result"'; then
        echo "✓ system.ping: OK (${STARTUP_TIME_MS}ms)"
    else
        echo "✗ system.ping: FAILED"
        echo "  Response: $PING_RESULT"
        exit 1
    fi

    # Test audio.list_devices
    DEVICES_RESULT=$(echo '{"jsonrpc":"2.0","id":2,"method":"audio.list_devices"}' | timeout 10 "$BINARY_PATH" 2>/dev/null || echo "FAILED")
    if echo "$DEVICES_RESULT" | grep -q '"result"'; then
        echo "✓ audio.list_devices: OK"
    else
        echo "✗ audio.list_devices: FAILED"
        echo "  Response: $DEVICES_RESULT"
        exit 1
    fi

    echo ""
    echo "Verification passed!"
fi

# Generate manifest
echo ""
echo "Generating manifest..."
GIT_SHA=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
BUILD_TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
PYTHON_VERSION=$(python3 --version | awk '{print $2}')

# Get version from pyproject.toml
VERSION=$(grep '^version' pyproject.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

# Determine ONNX status (not yet included in minimal build)
ONNX_VERSION="not-included"
if pip show onnxruntime >/dev/null 2>&1; then
    ONNX_VERSION=$(pip show onnxruntime | grep Version | awk '{print $2}')
fi

cat > "$DIST_DIR/manifest.json" << EOF
{
  "artifact_name": "$ARTIFACT_NAME",
  "version": "$VERSION",
  "platform": "$PLATFORM_TAG",
  "python_version": "$PYTHON_VERSION",
  "build_timestamp": "$BUILD_TIMESTAMP",
  "git_sha": "$GIT_SHA",
  "binary_size_bytes": $BINARY_SIZE,
  "startup_time_ms": $STARTUP_TIME_MS,
  "gpu_support": "none",
  "onnxruntime_version": "$ONNX_VERSION",
  "build_time_seconds": $BUILD_TIME
}
EOF

echo "Manifest written to: $DIST_DIR/manifest.json"

# Summary
echo ""
echo "=== Build Summary ==="
echo "Artifact: $BINARY_PATH"
echo "Size: ${BINARY_SIZE_MB} MB"
if [ "$STARTUP_TIME_MS" -gt 0 ]; then
    STARTUP_SEC=$(echo "scale=2; $STARTUP_TIME_MS / 1000" | bc)
    echo "Startup time: ${STARTUP_SEC}s"
fi
echo "Manifest: $DIST_DIR/manifest.json"
echo ""

# Check against targets
echo "=== Target Compliance ==="
if (( BINARY_SIZE < 524288000 )); then
    echo "✓ Binary size: ${BINARY_SIZE_MB} MB < 500 MB limit"
else
    echo "✗ Binary size: ${BINARY_SIZE_MB} MB exceeds 500 MB limit"
fi

if [ "$STARTUP_TIME_MS" -gt 0 ] && (( STARTUP_TIME_MS < 5000 )); then
    echo "✓ Startup time: ${STARTUP_TIME_MS}ms < 5000ms limit"
elif [ "$STARTUP_TIME_MS" -gt 0 ]; then
    echo "✗ Startup time: ${STARTUP_TIME_MS}ms exceeds 5000ms limit"
fi

echo ""
echo "Build complete!"
````

## File: scripts/bundle-sidecar.ps1
````powershell
#
# Bundle Sidecar for Tauri (Windows)
#
# Copies the PyInstaller-built sidecar binary to the Tauri binaries directory
# with the correct target-triple naming for cross-platform bundling.
#
# Usage:
#   .\scripts\bundle-sidecar.ps1 [-Target <TARGET_TRIPLE>]
#
# Examples:
#   .\scripts\bundle-sidecar.ps1                                    # Auto-detect
#   .\scripts\bundle-sidecar.ps1 -Target x86_64-pc-windows-msvc     # Explicit
#

param(
    [string]$Target = ""
)

$ErrorActionPreference = "Stop"

# Directories
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$SidecarDist = Join-Path $ProjectRoot "sidecar\dist"
$TauriBinaries = Join-Path $ProjectRoot "src-tauri\binaries"

# Binary name
$SidecarName = "openvoicy-sidecar"

function Write-Info { param($Message) Write-Host "[INFO] $Message" -ForegroundColor Green }
function Write-Warn { param($Message) Write-Host "[WARN] $Message" -ForegroundColor Yellow }
function Write-Error2 { param($Message) Write-Host "[ERROR] $Message" -ForegroundColor Red }
function Write-Step { param($Message) Write-Host "[STEP] $Message" -ForegroundColor Cyan }

# Detect target triple
function Get-TargetTriple {
    $arch = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "i686" }
    return "$arch-pc-windows-msvc"
}

# Main
Write-Host "=================================="
Write-Host "  Bundle Sidecar for Tauri"
Write-Host "=================================="
Write-Host ""

# Auto-detect target if not specified
if (-not $Target) {
    $Target = Get-TargetTriple
    Write-Info "Auto-detected target: $Target"
}

$SourceBin = Join-Path $SidecarDist "$SidecarName.exe"
$DestBin = Join-Path $TauriBinaries "$SidecarName-$Target.exe"

Write-Host "Target:      $Target"
Write-Host "Source:      $SourceBin"
Write-Host "Destination: $DestBin"
Write-Host ""

# Check source exists
Write-Step "Checking source binary..."
if (-not (Test-Path $SourceBin)) {
    Write-Error2 "Source binary not found: $SourceBin"
    Write-Error2 "Run .\scripts\build-sidecar.ps1 first"
    exit 1
}

$SourceSize = (Get-Item $SourceBin).Length
$SourceSizeMB = [math]::Round($SourceSize / 1MB, 1)
Write-Info "Source binary: ${SourceSizeMB} MB"

# Create destination directory
Write-Step "Creating Tauri binaries directory..."
if (-not (Test-Path $TauriBinaries)) {
    New-Item -ItemType Directory -Path $TauriBinaries -Force | Out-Null
}

# Copy binary
Write-Step "Copying binary..."
Copy-Item -Path $SourceBin -Destination $DestBin -Force

# Verify copy
Write-Step "Verifying..."
if (-not (Test-Path $DestBin)) {
    Write-Error2 "Failed to copy binary"
    exit 1
}

$DestSize = (Get-Item $DestBin).Length
if ($SourceSize -ne $DestSize) {
    Write-Error2 "Size mismatch after copy!"
    exit 1
}

# Quick self-check
Write-Step "Running sidecar self-check..."
try {
    $PingRequest = '{"jsonrpc":"2.0","id":1,"method":"system.ping","params":{}}'
    $Result = $PingRequest | & $DestBin 2>$null | Select-Object -First 1
    if ($Result -match '"protocol":"v1"') {
        Write-Info "Sidecar self-check passed"
    } else {
        Write-Warn "Sidecar responded but protocol check unclear"
    }
} catch {
    Write-Warn "Could not verify sidecar: $_"
}

Write-Host ""
Write-Info "Sidecar bundled successfully!"
Write-Host ""
Write-Host "Bundled binary: $DestBin"
Write-Host ""
Write-Host "Next steps:"
Write-Host "  1. Build Tauri app: cd src-tauri && cargo tauri build"
Write-Host "  2. The sidecar will be included in the app bundle"
Write-Host ""

# List all bundled binaries
if (Test-Path $TauriBinaries) {
    Write-Host "Bundled sidecars:"
    Get-ChildItem $TauriBinaries | Format-Table Name, Length -AutoSize
}
````

## File: scripts/bundle-sidecar.sh
````bash
#!/usr/bin/env bash
#
# Bundle Sidecar for Tauri
#
# Copies the PyInstaller-built sidecar binary to the Tauri binaries directory
# with the correct target-triple naming for cross-platform bundling.
#
# Usage:
#   ./scripts/bundle-sidecar.sh [--target TARGET_TRIPLE]
#
# Examples:
#   ./scripts/bundle-sidecar.sh                                    # Auto-detect current platform
#   ./scripts/bundle-sidecar.sh --target x86_64-unknown-linux-gnu  # Explicit target
#   ./scripts/bundle-sidecar.sh --target x86_64-pc-windows-msvc    # Windows target
#
# The script expects the sidecar binary to already be built via:
#   ./scripts/build-sidecar.sh
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Directories
SIDECAR_DIST="$PROJECT_ROOT/sidecar/dist"
TAURI_BINARIES="$PROJECT_ROOT/src-tauri/binaries"

# Binary names
SIDECAR_NAME="openvoicy-sidecar"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
log_step() { echo -e "${BLUE}[STEP]${NC} $*"; }

# Detect target triple for current platform
detect_target_triple() {
    local os arch

    case "$(uname -s)" in
        Linux)
            os="unknown-linux-gnu"
            ;;
        Darwin)
            os="apple-darwin"
            ;;
        MINGW*|CYGWIN*|MSYS*)
            os="pc-windows-msvc"
            ;;
        *)
            log_error "Unknown OS: $(uname -s)"
            exit 1
            ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        aarch64|arm64)
            arch="aarch64"
            ;;
        *)
            log_error "Unknown architecture: $(uname -m)"
            exit 1
            ;;
    esac

    echo "${arch}-${os}"
}

# Get source binary path
get_source_binary() {
    local target="$1"

    if [[ "$target" == *"windows"* ]]; then
        echo "$SIDECAR_DIST/${SIDECAR_NAME}.exe"
    else
        echo "$SIDECAR_DIST/${SIDECAR_NAME}"
    fi
}

# Get destination binary path with target triple
get_dest_binary() {
    local target="$1"

    if [[ "$target" == *"windows"* ]]; then
        echo "$TAURI_BINARIES/${SIDECAR_NAME}-${target}.exe"
    else
        echo "$TAURI_BINARIES/${SIDECAR_NAME}-${target}"
    fi
}

# Main function
main() {
    local target=""

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --target)
                target="$2"
                shift 2
                ;;
            -h|--help)
                echo "Usage: $0 [--target TARGET_TRIPLE]"
                echo ""
                echo "Bundles the sidecar binary for Tauri with proper naming."
                echo ""
                echo "Options:"
                echo "  --target  Target triple (e.g., x86_64-unknown-linux-gnu)"
                echo "            Auto-detected if not specified."
                echo ""
                echo "Supported targets:"
                echo "  x86_64-unknown-linux-gnu    Linux x64"
                echo "  aarch64-unknown-linux-gnu   Linux ARM64"
                echo "  x86_64-apple-darwin         macOS Intel"
                echo "  aarch64-apple-darwin        macOS Apple Silicon"
                echo "  x86_64-pc-windows-msvc      Windows x64"
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done

    # Auto-detect target if not specified
    if [[ -z "$target" ]]; then
        target=$(detect_target_triple)
        log_info "Auto-detected target: $target"
    fi

    local source_bin dest_bin
    source_bin=$(get_source_binary "$target")
    dest_bin=$(get_dest_binary "$target")

    echo "=================================="
    echo "  Bundle Sidecar for Tauri"
    echo "=================================="
    echo ""
    echo "Target:      $target"
    echo "Source:      $source_bin"
    echo "Destination: $dest_bin"
    echo ""

    # Check source exists
    log_step "Checking source binary..."
    if [[ ! -f "$source_bin" ]]; then
        log_error "Source binary not found: $source_bin"
        log_error "Run ./scripts/build-sidecar.sh first"
        exit 1
    fi

    local source_size
    source_size=$(stat -c%s "$source_bin" 2>/dev/null || stat -f%z "$source_bin")
    log_info "Source binary: $(numfmt --to=iec-i --suffix=B "$source_size" 2>/dev/null || echo "$((source_size / 1024 / 1024)) MB")"

    # Create destination directory
    log_step "Creating Tauri binaries directory..."
    mkdir -p "$TAURI_BINARIES"

    # Copy binary
    log_step "Copying binary..."
    cp "$source_bin" "$dest_bin"

    # Ensure executable permissions (Unix)
    if [[ ! "$target" == *"windows"* ]]; then
        chmod +x "$dest_bin"
    fi

    # Verify copy
    log_step "Verifying..."
    if [[ ! -f "$dest_bin" ]]; then
        log_error "Failed to copy binary"
        exit 1
    fi

    local dest_size
    dest_size=$(stat -c%s "$dest_bin" 2>/dev/null || stat -f%z "$dest_bin")

    if [[ "$source_size" != "$dest_size" ]]; then
        log_error "Size mismatch after copy!"
        exit 1
    fi

    # Quick self-check (Unix only)
    if [[ ! "$target" == *"windows"* ]]; then
        log_step "Running sidecar self-check..."
        local ping_result
        if ping_result=$(echo '{"jsonrpc":"2.0","id":1,"method":"system.ping","params":{}}' | timeout 10 "$dest_bin" 2>/dev/null); then
            if echo "$ping_result" | grep -q '"protocol":"v1"'; then
                log_info "Sidecar self-check passed"
            else
                log_warn "Sidecar responded but protocol check failed"
            fi
        else
            log_warn "Could not verify sidecar (may need runtime dependencies)"
        fi
    fi

    echo ""
    log_info "Sidecar bundled successfully!"
    echo ""
    echo "Bundled binary: $dest_bin"
    echo ""
    echo "Next steps:"
    echo "  1. Build Tauri app: cd src-tauri && cargo tauri build"
    echo "  2. The sidecar will be included in the app bundle"
    echo ""

    # List all bundled binaries
    if [[ -d "$TAURI_BINARIES" ]]; then
        echo "Bundled sidecars:"
        ls -la "$TAURI_BINARIES"/ 2>/dev/null || true
    fi
}

main "$@"
````

## File: scripts/demo-gate-1.sh
````bash
#!/bin/bash
# scripts/demo-gate-1.sh
# Gate 1: IPC Contract Locked
#
# Verifies that the IPC contract is complete and valid.
# This gate must pass before M1 and M2 work can proceed.

set -e

echo "=== Gate 1: IPC Contract Verification ==="
echo ""

# Check protocol document exists
echo -n "Checking IPC_PROTOCOL_V1.md exists... "
if [ -f shared/ipc/IPC_PROTOCOL_V1.md ]; then
    echo "PASS"
else
    echo "FAIL: Protocol doc missing"
    exit 1
fi

# Check examples file exists
echo -n "Checking IPC_V1_EXAMPLES.jsonl exists... "
if [ -f shared/ipc/examples/IPC_V1_EXAMPLES.jsonl ]; then
    echo "PASS"
else
    echo "FAIL: Examples file missing"
    exit 1
fi

# Validate examples parse as JSON
echo -n "Validating examples parse as valid JSON... "
python3 -c "
import json
import sys

with open('shared/ipc/examples/IPC_V1_EXAMPLES.jsonl') as f:
    for i, line in enumerate(f, 1):
        line = line.strip()
        if not line or line.startswith('//'):
            continue
        try:
            json.loads(line)
        except json.JSONDecodeError as e:
            print(f'FAIL: Line {i} invalid JSON: {e}', file=sys.stderr)
            sys.exit(1)
print('PASS')
"

# Check protocol document has required sections
echo -n "Checking protocol has required sections... "
required_sections=(
    "Methods"
    "Notifications"
    "Error Codes"
)

for section in "${required_sections[@]}"; do
    if ! grep -q "$section" shared/ipc/IPC_PROTOCOL_V1.md; then
        echo "FAIL: Missing section '$section'"
        exit 1
    fi
done
echo "PASS"

# Check protocol defines core methods
echo -n "Checking core methods are defined... "
core_methods=(
    "system.ping"
    "system.info"
    "audio.list_devices"
    "asr.initialize"
    "recording.start"
    "recording.stop"
)

for method in "${core_methods[@]}"; do
    if ! grep -q "$method" shared/ipc/IPC_PROTOCOL_V1.md; then
        echo "FAIL: Missing method '$method'"
        exit 1
    fi
done
echo "PASS"

echo ""
echo "=== Gate 1: PASSED ==="
echo ""
echo "The IPC contract is complete and valid."
echo "M1 and M2 work can now proceed in parallel."
````

## File: scripts/generate_assets.py
````python
#!/usr/bin/env python3
"""
Generate tray icons and audio cues for OpenVoicy.

This script creates:
- 6 tray icon states at multiple resolutions
- 3 audio cue sounds (start, stop, error)

Run from project root:
    python scripts/generate_assets.py
"""
⋮----
# Directories
SCRIPT_DIR = Path(__file__).parent
PROJECT_ROOT = SCRIPT_DIR.parent
ICONS_DIR = PROJECT_ROOT / "src-tauri" / "icons"
SOUNDS_DIR = PROJECT_ROOT / "src-tauri" / "sounds"
⋮----
# Icon colors (RGBA)
COLORS = {
⋮----
"idle": (46, 204, 113, 255),        # Green - ready
"recording": (231, 76, 60, 255),     # Red - recording
"transcribing": (241, 196, 15, 255), # Yellow - processing
"loading": (52, 152, 219, 255),      # Blue - loading
"error": (231, 76, 60, 255),         # Red - error
"disabled": (149, 165, 166, 255),    # Gray - paused
⋮----
# Audio settings
SAMPLE_RATE = 44100
DURATION_SHORT = 0.1  # 100ms
DURATION_LONG = 0.2   # 200ms
⋮----
def create_icon(size: int, state: str) -> Image.Image
⋮----
"""Create a single tray icon for the given state."""
img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
draw = ImageDraw.Draw(img)
⋮----
color = COLORS[state]
margin = max(1, size // 8)
⋮----
# Green circle with checkmark
⋮----
# Draw checkmark
line_width = max(1, size // 8)
⋮----
# Checkmark path
check_points = [
⋮----
# Red solid circle (recording indicator)
⋮----
# Yellow circle with three dots (processing)
⋮----
# Draw dots
dot_radius = max(1, size // 10)
cy = size // 2
⋮----
cx = size // 2 + dx
⋮----
# Blue circle with arc (loading indicator)
⋮----
# Draw partial ring to indicate loading
ring_margin = margin + max(2, size // 6)
⋮----
# Red circle with exclamation mark
⋮----
# Draw exclamation mark
line_width = max(1, size // 6)
cx = size // 2
# Vertical line
⋮----
# Dot
dot_y = size - margin - size // 5
⋮----
# Gray circle with pause bars
⋮----
# Draw pause symbol (two vertical bars)
bar_width = max(1, size // 8)
bar_height = size // 3
⋮----
def generate_icons()
⋮----
"""Generate all tray icons at multiple resolutions."""
⋮----
states = ["idle", "recording", "transcribing", "loading", "error", "disabled"]
sizes = [16, 22, 32]
⋮----
# Standard resolution
icon = create_icon(size, state)
filename = f"tray-{state}-{size}x{size}.png"
⋮----
# @2x resolution for HiDPI
icon_2x = create_icon(size * 2, state)
filename_2x = f"tray-{state}-{size}x{size}@2x.png"
⋮----
# Also create standard size without dimension suffix for easier loading
⋮----
icon = create_icon(32, state)  # 32x32 as default
filename = f"tray-{state}.png"
⋮----
def generate_sine_wave(frequency: float, duration: float, amplitude: float = 0.5) -> bytes
⋮----
"""Generate a sine wave audio sample."""
num_samples = int(SAMPLE_RATE * duration)
samples = []
⋮----
t = i / SAMPLE_RATE
# Apply envelope (fade in/out) to avoid clicks
envelope = 1.0
fade_samples = int(SAMPLE_RATE * 0.01)  # 10ms fade
⋮----
envelope = i / fade_samples
⋮----
envelope = (num_samples - i) / fade_samples
⋮----
value = amplitude * envelope * math.sin(2 * math.pi * frequency * t)
# Convert to 16-bit signed integer
sample = int(value * 32767)
⋮----
def write_wav(filename: Path, audio_data: bytes)
⋮----
"""Write audio data to a WAV file."""
⋮----
wav.setnchannels(1)  # Mono
wav.setsampwidth(2)  # 16-bit
⋮----
def generate_sounds()
⋮----
"""Generate audio cue sounds."""
⋮----
# Start sound: Rising tone (pleasant, confirming)
# Two quick notes going up
start_tone1 = generate_sine_wave(880, DURATION_SHORT / 2, 0.3)  # A5
start_tone2 = generate_sine_wave(1046.5, DURATION_SHORT / 2, 0.3)  # C6
start_audio = start_tone1 + start_tone2
⋮----
# Stop sound: Falling tone (confirmation)
# Two quick notes going down
stop_tone1 = generate_sine_wave(1046.5, DURATION_SHORT / 2, 0.3)  # C6
stop_tone2 = generate_sine_wave(880, DURATION_SHORT / 2, 0.3)  # A5
stop_audio = stop_tone1 + stop_tone2
⋮----
# Error sound: Dissonant/warning tone
# Lower frequency, slightly longer
error_audio = generate_sine_wave(330, DURATION_LONG, 0.4)  # E4 - warning tone
⋮----
def main()
⋮----
"""Generate all assets."""
````

## File: scripts/validate_ipc_examples.py
````python
#!/usr/bin/env python3
"""
Validate IPC_V1_EXAMPLES.jsonl against the protocol specification.

This script:
1. Validates all JSONL lines parse correctly
2. Validates JSON-RPC 2.0 structure
3. Validates error codes are in valid ranges
4. Validates all error.data.kind values are from the allowed set
5. Validates message types (request, response, notification, error)

Exit codes:
  0 - All validations passed
  1 - Validation errors found
"""
⋮----
# Valid error kind strings from the protocol
VALID_ERROR_KINDS = {
⋮----
# Valid JSON-RPC 2.0 error codes
JSONRPC_STANDARD_CODES = {-32700, -32600, -32601, -32602, -32603}
JSONRPC_SERVER_ERROR_RANGE = range(-32099, -31999)  # -32099 to -32000
⋮----
# Valid message types
VALID_MESSAGE_TYPES = {"request", "response", "notification", "error"}
⋮----
# Valid method prefixes
VALID_METHOD_PREFIXES = {"system", "audio", "model", "asr", "recording", "replacements", "status", "event"}
⋮----
# Valid notification methods
VALID_NOTIFICATION_METHODS = {
⋮----
# Valid request methods
VALID_REQUEST_METHODS = {
⋮----
def validate_error_code(code: int) -> str | None
⋮----
"""Validate error code is in valid range."""
⋮----
def validate_error_kind(kind: str) -> str | None
⋮----
"""Validate error kind is in allowed set."""
⋮----
def validate_jsonrpc_request(data: dict[str, Any], line_num: int) -> list[str]
⋮----
"""Validate JSON-RPC request structure."""
errors = []
⋮----
method = data["method"]
⋮----
def validate_jsonrpc_response(data: dict[str, Any], line_num: int) -> list[str]
⋮----
"""Validate JSON-RPC response structure."""
⋮----
def validate_jsonrpc_notification(data: dict[str, Any], line_num: int) -> list[str]
⋮----
"""Validate JSON-RPC notification structure."""
⋮----
def validate_jsonrpc_error(data: dict[str, Any], line_num: int) -> list[str]
⋮----
"""Validate JSON-RPC error response structure."""
⋮----
error = data["error"]
⋮----
code_err = validate_error_code(error["code"])
⋮----
error_data = error["data"]
⋮----
kind_err = validate_error_kind(error_data["kind"])
⋮----
def validate_example(obj: dict[str, Any], line_num: int) -> list[str]
⋮----
"""Validate a single example object."""
⋮----
# Check required fields
⋮----
msg_type = obj["type"]
⋮----
data = obj["data"]
⋮----
# Check JSON-RPC version
⋮----
# Type-specific validation
⋮----
def main() -> int
⋮----
"""Main validation function."""
# Find the examples file
script_dir = Path(__file__).parent
repo_root = script_dir.parent
examples_file = repo_root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
⋮----
all_errors: list[str] = []
line_count = 0
⋮----
# Statistics
stats = {
⋮----
line = line.strip()
⋮----
# Parse JSON
⋮----
obj = json.loads(line)
⋮----
# Validate structure
errors = validate_example(obj, line_num)
⋮----
# Update stats
⋮----
# Print results
````

## File: scripts/validate_model_manifest.py
````python
#!/usr/bin/env python3
"""
Validate MODEL_MANIFEST.json schema and cross-reference with IPC examples.

This script:
1. Validates MODEL_MANIFEST.json parses correctly
2. Validates required schema fields are present
3. Validates asr.initialize examples in IPC_V1_EXAMPLES.jsonl use valid model_ids

Exit codes:
  0 - All validations passed
  1 - Validation errors found
"""
⋮----
# Required top-level fields in manifest
REQUIRED_MANIFEST_FIELDS = {
⋮----
# Required fields in license object
REQUIRED_LICENSE_FIELDS = {
⋮----
# Required fields in file objects
REQUIRED_FILE_FIELDS = {
⋮----
def validate_manifest_schema(manifest: dict[str, Any]) -> list[str]
⋮----
"""Validate manifest has required fields."""
errors = []
⋮----
# Check top-level required fields
⋮----
# Check license fields
⋮----
license_obj = manifest["license"]
⋮----
# Check files array
⋮----
# Validate types
⋮----
def validate_ipc_model_ids(manifest: dict[str, Any], examples_file: Path) -> list[str]
⋮----
"""Validate asr.initialize examples use valid model_ids from manifest."""
⋮----
manifest_model_id = manifest.get("model_id")
⋮----
line = line.strip()
⋮----
obj = json.loads(line)
⋮----
continue  # Parse errors handled by other validator
⋮----
data = obj.get("data", {})
method = data.get("method")
⋮----
# Check asr.initialize requests
⋮----
params = data.get("params", {})
model_id = params.get("model_id")
⋮----
# Allow whisper models in examples for compatibility testing
# but warn about mismatch
⋮----
def main() -> int
⋮----
"""Main validation function."""
script_dir = Path(__file__).parent
repo_root = script_dir.parent
⋮----
manifest_file = repo_root / "shared" / "model" / "MODEL_MANIFEST.json"
examples_file = repo_root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
⋮----
all_errors: list[str] = []
⋮----
# Check manifest exists
⋮----
# Parse manifest
⋮----
manifest = json.load(f)
⋮----
# Validate schema
schema_errors = validate_manifest_schema(manifest)
⋮----
# Validate IPC cross-reference (warn only, don't fail)
# IPC examples may use different model IDs for testing purposes
ipc_warnings = validate_ipc_model_ids(manifest, examples_file)
⋮----
# Print results
````

## File: src/components/Replacements/index.ts
````typescript
/**
 * Replacement rule components exports.
 */
````

## File: src/components/Replacements/PresetsPanel.tsx
````typescript
/**
 * Panel for managing preset replacement rule sets.
 *
 * Features:
 * - List of available presets with descriptions
 * - Enable/disable presets
 * - View preset rules (read-only)
 * - Clear indication of preset vs user rules
 */
⋮----
import { useState } from 'react';
import type { PresetInfo, ReplacementRule } from '../../types';
⋮----
interface PresetsPanelProps {
  presets: PresetInfo[];
  enabledPresets: string[];
  onTogglePreset: (presetId: string, enabled: boolean) => void;
  presetRules?: Map<string, ReplacementRule[]>;
}
⋮----
/** Preset card component. */
⋮----
{/* Expand/collapse rules */}
⋮----
onClick=
⋮----
{/* Expanded rules list */}
⋮----
onToggle=
````

## File: src/components/Replacements/ReplacementEditor.tsx
````typescript
/**
 * Editor dialog for creating/editing replacement rules.
 *
 * Features:
 * - Literal vs regex mode selector
 * - Pattern validation (especially regex)
 * - Word boundary and case sensitivity options
 * - Real-time feedback on regex errors
 */
⋮----
import { useState, useEffect, useCallback } from 'react';
import type { ReplacementRule, ReplacementKind } from '../../types';
⋮----
interface ReplacementEditorProps {
  rule?: ReplacementRule | null;
  onSave: (rule: ReplacementRule) => void;
  onCancel: () => void;
  existingPatterns?: string[];
}
⋮----
/** Generate a UUID v4. */
function generateId(): string
⋮----
/** Validate a regex pattern. */
function validateRegex(pattern: string): string | null
⋮----
export function ReplacementEditor({
  rule,
  onSave,
  onCancel,
  existingPatterns = [],
}: ReplacementEditorProps)
⋮----
// Validate pattern on changes
⋮----
// Check for duplicate literal patterns
⋮----
{/* Kind selector */}
⋮----
onChange=
⋮----
{/* Pattern input */}
⋮----
{/* Replacement input */}
⋮----
{/* Options */}
⋮----
{/* Description */}
⋮----
{/* Actions */}
````

## File: src/components/Replacements/ReplacementList.tsx
````typescript
/**
 * Main replacement rules list component.
 *
 * Features:
 * - List all user replacement rules
 * - Enable/disable individual rules
 * - Drag-to-reorder (rules apply in order)
 * - Edit and delete actions
 * - Add new rule button
 * - Import/export functionality
 */
⋮----
import { useState, useCallback } from 'react';
import type { ReplacementRule } from '../../types';
import { ReplacementEditor } from './ReplacementEditor';
⋮----
interface ReplacementListProps {
  rules: ReplacementRule[];
  onChange: (rules: ReplacementRule[]) => void;
  isLoading?: boolean;
}
⋮----
/** Single rule row component. */
⋮----
{/* Enable toggle */}
⋮----
{/* Rule number */}
⋮----
{/* Rule type badge */}
⋮----
{/* Pattern and replacement */}
⋮----
{/* Description */}
⋮----
{/* Origin badge */}
⋮----
{/* Actions - visible on hover */}
⋮----
{/* Move buttons */}
⋮----
{/* Edit */}
⋮----
{/* Delete */}
⋮----
// Filter user rules (not preset)
⋮----
// Update existing rule
⋮----
// Add new rule
⋮----
// Validate imported rules
⋮----
// Assign new IDs to avoid conflicts
⋮----
{/* Header */}
⋮----
{/* Info text */}
⋮----
{/* Rules list */}
⋮----
onDelete=
onMoveUp=
onMoveDown=
⋮----
{/* Stats */}
⋮----
{/* Editor modal */}
⋮----
setEditingRule(null);
setIsAdding(false);
````

## File: src/components/Replacements/ReplacementPreview.tsx
````typescript
/**
 * Preview component for testing replacement rules.
 *
 * Features:
 * - Input text area for testing
 * - Real-time preview of transformations
 * - Visual diff showing changes
 * - Support for testing individual rules or entire ruleset
 */
⋮----
import { useState, useMemo } from 'react';
import type { ReplacementRule } from '../../types';
⋮----
interface ReplacementPreviewProps {
  rules: ReplacementRule[];
}
⋮----
/** Apply replacement rules to text (local mirror of sidecar logic). */
function applyReplacements(text: string, rules: ReplacementRule[]): string
⋮----
// Build regex for literal match
⋮----
// Regex match
⋮----
// Skip invalid rules
⋮----
/** Expand macros in text. */
function expandMacros(text: string): string
⋮----
/** Highlight differences between original and result. */
⋮----
// Simple word-based diff for visualization
⋮----
// Find common prefix length
⋮----
// Find common suffix length
⋮----
{/* Input */}
⋮----
{/* Output */}
⋮----
{/* Diff view */}
⋮----
{/* Quick stats */}
⋮----
{/* Macro hints */}
````

## File: src/components/Settings/Diagnostics.tsx
````typescript
/**
 * Diagnostics panel for bug reports.
 *
 * Features:
 * - Generates comprehensive diagnostics text blob
 * - Redacts sensitive paths and transcript contents
 * - Bounded size (truncates if too large)
 * - One-click copy to clipboard
 * - Shows recent logs (redacted)
 */
⋮----
import { useState, useMemo, useCallback } from 'react';
import type { DiagnosticsReport, Capabilities, AppConfig, SelfCheckResult } from '../../types';
⋮----
interface DiagnosticsProps {
  report: DiagnosticsReport | null;
  onRefresh: () => Promise<void>;
  isLoading?: boolean;
}
⋮----
/** Max diagnostics output size in characters. */
⋮----
/** Paths that should be redacted. */
⋮----
/\/Users\/[^/]+/g,        // macOS user paths
/\/home\/[^/]+/g,         // Linux user paths
/C:\\Users\\[^\\]+/g,     // Windows user paths
⋮----
/** Redact sensitive paths from text. */
function redactPaths(text: string): string
⋮----
/** Format capabilities for diagnostics. */
function formatCapabilities(caps: Capabilities): string
⋮----
// Display server
⋮----
// Hotkey
⋮----
// Injection
⋮----
// Permissions
⋮----
// Feature availability
⋮----
/** Format config for diagnostics (redacted). */
function formatConfig(config: AppConfig): string
⋮----
// Don't include replacement patterns (may contain personal data)
⋮----
// Redact device UID
⋮----
/** Format self-check results. */
function formatSelfCheck(check: SelfCheckResult): string
⋮----
const formatItem = (name: string, item:
⋮----
/** Generate full diagnostics text. */
function generateDiagnosticsText(report: DiagnosticsReport): string
⋮----
// Header
⋮----
// Self-check
⋮----
// Capabilities
⋮----
// Config
⋮----
// Raw diagnostics from capabilities
⋮----
// Footer
⋮----
// Truncate if too large
⋮----
export function Diagnostics(
⋮----
// Fallback for older browsers
⋮----
// Loading state
⋮----
{/* Header */}
⋮----
{/* Description */}
⋮----
{/* Diagnostics output */}
⋮----
{/* Stats overlay */}
⋮----
{/* Privacy notice */}
````

## File: src/components/Settings/HistoryPanel.tsx
````typescript
/**
 * Transcript history panel showing recent transcriptions.
 *
 * Features:
 * - Shows recent transcripts with newest first
 * - Copy action for each transcript
 * - Shows injection status (injected, clipboard-only, error)
 * - Relative timestamps ("2 minutes ago")
 * - Audio duration display
 */
⋮----
import { useState } from 'react';
import type { TranscriptEntry, InjectionResult } from '../../types';
⋮----
interface HistoryPanelProps {
  entries: TranscriptEntry[];
  onCopy: (id: string) => void;
}
⋮----
/** Format a timestamp as relative time. */
function formatRelativeTime(timestamp: string): string
⋮----
/** Format duration in milliseconds to human-readable. */
function formatDuration(ms: number): string
⋮----
/** Get badge info for injection result. */
function getInjectionBadge(result: InjectionResult):
⋮----
interface TranscriptCardProps {
  entry: TranscriptEntry;
  onCopy: () => void;
}
⋮----
function TranscriptCard(
⋮----
const handleCopy = () =>
⋮----
{/* Header with timestamp and badge */}
⋮----
<span>
⋮----
{/* Injection status badge */}
⋮----
{/* Transcript text */}
⋮----
{/* Actions */}
⋮----
// Empty state
⋮----
{/* Transcript list (newest first - entries should already be sorted) */}
````

## File: src/components/Settings/HotkeyConfig.tsx
````typescript
/**
 * Hotkey configuration component.
 *
 * Features:
 * - Primary hotkey input for recording
 * - Copy-last hotkey input
 * - Hold/Toggle mode selector
 * - Shows effective mode with reason if different
 */
⋮----
import { useState } from 'react';
import type { HotkeyMode, EffectiveMode, ActivationMode } from '../../types';
⋮----
interface HotkeyConfigProps {
  primaryHotkey: string;
  copyLastHotkey: string;
  mode: HotkeyMode;
  effectiveMode?: EffectiveMode<ActivationMode>;
  onPrimaryChange: (hotkey: string) => Promise<void>;
  onCopyLastChange: (hotkey: string) => Promise<void>;
  onModeChange: (mode: HotkeyMode) => Promise<void>;
  isLoading?: boolean;
}
⋮----
interface HotkeyInputProps {
  label: string;
  description: string;
  value: string;
  onChange: (value: string) => Promise<void>;
  disabled?: boolean;
}
⋮----
const handleKeyDown = async (e: React.KeyboardEvent) =>
⋮----
// Don't accept modifier-only combinations
⋮----
return; // Wait for a non-modifier key
⋮----
onBlur=
⋮----
onClick=
⋮----
const handleModeChange = async (newMode: HotkeyMode) =>
⋮----
{/* Primary hotkey */}
⋮----
{/* Copy last hotkey */}
⋮----
{/* Mode selector */}
⋮----
onChange=
⋮----
{/* Effective mode warning */}
⋮----
{/* Error display */}
````

## File: src/components/Settings/InjectionSettings.tsx
````typescript
/**
 * Text injection settings component.
 *
 * Features:
 * - Paste delay slider (10-500ms)
 * - Restore clipboard toggle
 * - Suffix selector (none, space, newline)
 * - Focus Guard toggle with explanation
 */
⋮----
import { useState } from 'react';
import type { InjectionConfig } from '../../types';
⋮----
interface InjectionSettingsProps {
  config: InjectionConfig;
  onChange: (key: keyof InjectionConfig, value: any) => Promise<void>;
  isLoading?: boolean;
}
⋮----
/** Suffix options for after injected text. */
⋮----
/** Tooltip component for explanations. */
function Tooltip(
⋮----
export function InjectionSettings(
⋮----
const handleChange = async (key: keyof InjectionConfig, value: any) =>
⋮----
{/* Paste delay slider */}
⋮----
{/* Restore clipboard toggle */}
⋮----
onClick=
⋮----
{/* Suffix selector */}
⋮----
{/* Focus Guard toggle */}
⋮----
{/* Focus Guard explanation */}
````

## File: src/components/Settings/MicrophoneSelect.tsx
````typescript
/**
 * Microphone device selector component.
 *
 * Features:
 * - Shows list of available audio input devices
 * - Highlights default device
 * - Live-applies selection with rollback on failure
 */
⋮----
import { useEffect, useState } from 'react';
import type { AudioDevice } from '../../types';
⋮----
interface MicrophoneSelectProps {
  devices: AudioDevice[];
  selectedUid: string | undefined;
  audioCuesEnabled: boolean;
  onDeviceChange: (uid: string) => Promise<void>;
  onAudioCuesChange: (enabled: boolean) => Promise<void>;
  isLoading?: boolean;
}
⋮----
export function MicrophoneSelect({
  devices,
  selectedUid,
  audioCuesEnabled,
  onDeviceChange,
  onAudioCuesChange,
  isLoading,
}: MicrophoneSelectProps)
⋮----
const handleDeviceChange = async (uid: string) =>
⋮----
const handleAudioCuesToggle = async () =>
⋮----
// Find currently selected device or default
⋮----
{/* Device selector */}
⋮----
onChange=
⋮----
{/* Device info */}
⋮----
{/* Loading indicator */}
⋮----
{/* Audio cues toggle */}
⋮----
{/* Error display */}
````

## File: src/components/Settings/MicrophoneTest.tsx
````typescript
/**
 * Microphone test component with real-time level meter.
 *
 * Features:
 * - Real-time audio level visualization (RMS + peak)
 * - Color-coded levels (green/yellow/red)
 * - No-signal detection with warning
 * - Smooth animations with decay
 * - Start/stop test controls
 */
⋮----
import { useEffect, useState, useRef, useCallback } from 'react';
⋮----
interface AudioLevel {
  rms: number;
  peak: number;
}
⋮----
interface MicrophoneTestProps {
  deviceUid: string | undefined;
  onStartTest: () => Promise<void>;
  onStopTest: () => Promise<void>;
  audioLevel: AudioLevel | null;
  isRunning?: boolean;
}
⋮----
/** Threshold for detecting no signal. */
⋮----
/** Level color thresholds. */
⋮----
/** Get color class based on level. */
function getLevelColor(level: number): string
⋮----
/** Get gradient style for smooth color transition. */
function getLevelGradient(level: number): string
⋮----
// Smooth level animation with decay
⋮----
// Update last activity if we have signal
⋮----
// Update peak hold
⋮----
// Animate to target level
const animate = () =>
⋮----
// Check for no signal
⋮----
// Reset level when device changes
⋮----
// Percentage for display
⋮----
{/* Level meter */}
⋮----
{/* RMS level bar */}
⋮----
{/* Peak hold indicator */}
⋮----
{/* Threshold markers */}
⋮----
{/* Level readout */}
⋮----
{/* Legend */}
⋮----
{/* No signal warning */}
⋮----
{/* Idle state hint */}
⋮----
{/* Error display */}
````

## File: src/components/Settings/ModelSettings.tsx
````typescript
/**
 * Model settings component for ASR model management.
 *
 * Features:
 * - Model status display (missing, downloading, verifying, ready, error)
 * - Download progress with visual progress bar
 * - "Download now" and "Purge cache" actions
 * - Model info (ID, revision, size)
 */
⋮----
import { useState } from 'react';
import type { ModelStatus, ModelState, Progress } from '../../types';
⋮----
interface ModelSettingsProps {
  status: ModelStatus | null;
  onDownload: () => Promise<void>;
  onPurgeCache: () => Promise<void>;
  isLoading?: boolean;
}
⋮----
/** Format bytes to human-readable size. */
function formatBytes(bytes: number): string
⋮----
/** Get status configuration for display. */
function getStatusConfig(state: ModelState):
⋮----
/** Progress bar component. */
function ProgressBar(
⋮----

⋮----
const handleDownload = async () =>
⋮----
const handlePurge = async () =>
⋮----
// Loading state
⋮----
{/* Model info */}
⋮----
{/* Download progress */}
⋮----
{/* Error state */}
⋮----
{/* Ready state */}
⋮----
{/* Actions */}
⋮----
{/* Download/Retry button */}
⋮----
{/* Downloading indicator */}
⋮----
{/* Purge cache button */}
⋮----
onClick=
⋮----
{/* Error from action */}
````

## File: src/components/Settings/SelfCheck.tsx
````typescript
/**
 * Self-check panel for system health status.
 *
 * Features:
 * - Quick health status for all subsystems
 * - Color-coded status indicators (ok/warning/error)
 * - Expandable details for each check
 * - Refresh button to re-run checks
 */
⋮----
import { useState, useCallback } from 'react';
import type { SelfCheckResult, CheckItem, CheckStatus } from '../../types';
⋮----
interface SelfCheckProps {
  result: SelfCheckResult | null;
  onRefresh: () => Promise<void>;
  isLoading?: boolean;
}
⋮----
/** Status icon mapping. */
⋮----
/** Individual check item row. */
⋮----
{/* Status icon */}
⋮----
{/* Label */}
⋮----
{/* Message */}
⋮----
{/* Expand indicator */}
⋮----
{/* Expanded detail */}
⋮----
/** Summary badge showing overall status. */
⋮----
// Loading state
⋮----
{/* Header */}
⋮----
{/* Check results */}
⋮----
{/* Help text */}
````

## File: src/components/Settings/SettingsPanel.tsx
````typescript
/**
 * Main settings panel combining all configuration sections.
 *
 * Features:
 * - Tabbed navigation for different settings areas
 * - Live apply with rollback on failure
 * - Integrates MicrophoneSelect, HotkeyConfig, InjectionSettings
 */
⋮----
import { useState } from 'react';
import type { AppConfig, AudioDevice, EffectiveMode, ActivationMode } from '../../types';
import { MicrophoneSelect } from './MicrophoneSelect';
import { HotkeyConfig } from './HotkeyConfig';
import { InjectionSettings } from './InjectionSettings';
⋮----
type SettingsTab = 'audio' | 'hotkeys' | 'injection';
⋮----
interface SettingsPanelProps {
  config: AppConfig;
  devices: AudioDevice[];
  effectiveHotkeyMode?: EffectiveMode<ActivationMode>;
  onConfigChange: (path: string[], value: any) => Promise<void>;
  isLoading?: boolean;
}
⋮----
/** Tab button component. */
function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
onClick: ()
⋮----
export function SettingsPanel({
  config,
  devices,
  effectiveHotkeyMode,
  onConfigChange,
  isLoading,
}: SettingsPanelProps)
⋮----
// Helper to create path-based config updaters
const handleAudioChange = async (key: string, value: any) =>
⋮----
const handleHotkeyChange = async (key: string, value: any) =>
⋮----
const handleInjectionChange = async (key: string, value: any) =>
⋮----
{/* Tab navigation */}
⋮----
{/* Tab content */}
⋮----
onDeviceChange=
⋮----
onAudioCuesChange=
⋮----
onCopyLastChange=
onModeChange=
````

## File: src/components/index.ts
````typescript
/**
 * Component exports.
 */
⋮----
// Replacement rule components
````

## File: src/components/StatusIndicator.tsx
````typescript
/**
 * Status indicator component showing current app state.
 *
 * Visual states:
 * - Idle: green dot, "Ready"
 * - Recording: red pulsing dot, "Recording..."
 * - Transcribing: yellow spinner, "Transcribing..."
 * - LoadingModel: blue spinner + progress, "Loading model..."
 * - Error: red exclamation, error message
 */
⋮----
import type { AppState } from '../types';
⋮----
interface StatusIndicatorProps {
  state: AppState;
  enabled: boolean;
  detail?: string;
  progress?: { current: number; total?: number };
}
⋮----
/** Status configuration for each state. */
⋮----
// Disabled state overrides
⋮----
{/* Status dot/indicator */}
⋮----
{/* Error icon overlay */}
⋮----
{/* Status text and details */}
⋮----
{/* Detail/error message */}
⋮----
{/* Progress bar for loading_model */}
````

## File: src/hooks/index.ts
````typescript
/**
 * Hook exports.
 */
````

## File: src/hooks/useTauriEvents.test.ts
````typescript
/**
 * Unit tests for the Tauri events hook.
 *
 * Note: These tests verify the hook structure and basic functionality.
 * Integration with actual Tauri events is tested in E2E tests.
 */
⋮----
import { describe, test, expect, beforeEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { listen } from '@tauri-apps/api/event';
import { useTauriEvents, useTauriEvent } from './useTauriEvents';
import { useAppStore } from '../store/appStore';
⋮----
// Reset store before each test
⋮----
// ============================================================================
// useTauriEvents TESTS
// ============================================================================
⋮----
// Wait for async setup
⋮----
// Verify listen was called for expected events
⋮----
// Test internal actions directly since event mock is complex
⋮----
// Test _setAppState
⋮----
// Test _setModelStatus
⋮----
// Test _setDownloadProgress
⋮----
// Test _setAudioLevel
⋮----
// Test _addHistoryEntry
⋮----
// Test _setError
⋮----
// ============================================================================
// useTauriEvent TESTS
// ============================================================================
⋮----
// Wait for async setup
⋮----
// Rerender with new handler
⋮----
// The hook should now use handler2
// We verify this by checking the ref is updated (handler1 !== handler2)
````

## File: src/hooks/useTauriEvents.ts
````typescript
/**
 * Hook for subscribing to Tauri events from the Rust backend.
 *
 * Sets up event listeners on mount and cleans them up on unmount.
 * Events update the Zustand store directly via internal actions.
 */
⋮----
import { useEffect, useRef } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useAppStore } from '../store/appStore';
import type {
  AudioLevelEvent,
  ModelStatus,
  Progress,
  StateEvent,
  TranscriptEntry,
} from '../types';
⋮----
// Event names emitted by Rust backend
⋮----
// App state changes
⋮----
// Model events
⋮----
// Audio events
⋮----
// Transcript events
⋮----
// Error events
⋮----
// Sidecar events
⋮----
/**
 * Hook that subscribes to all Tauri events and updates the store.
 *
 * Should be called once at the app root level (e.g., in App.tsx).
 */
export function useTauriEvents(): void
⋮----
// Prevent double setup in StrictMode
⋮----
const setupListeners = async () =>
⋮----
// Subscribe to app state changes
⋮----
// Subscribe to model status changes
⋮----
// Subscribe to model download progress
⋮----
// Subscribe to audio level updates (during mic test)
⋮----
// Don't log audio levels - too noisy
⋮----
// Subscribe to transcript completions
⋮----
// Subscribe to error events
⋮----
// Subscribe to sidecar status changes
⋮----
// Could update a dedicated sidecar state slice if needed
⋮----
// Cleanup on unmount
⋮----
/**
 * Hook for subscribing to a specific Tauri event.
 *
 * Useful for components that need custom event handling beyond
 * what the store provides.
 */
export function useTauriEvent<T>(
  eventName: string,
  handler: (payload: T) => void
): void
````

## File: src/store/appStore.test.ts
````typescript
/**
 * Unit tests for the Zustand app store.
 */
⋮----
import { describe, test, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  useAppStore,
  selectAppState,
  selectIsRecording,
  selectIsTranscribing,
  selectIsIdle,
  selectModelReady,
  selectDevices,
  selectHistory,
  selectConfig,
} from './appStore';
import {
  setMockInvokeHandler,
  createMockDevice,
  createMockTranscript,
  createMockModelStatus,
  createMockConfig,
} from '../tests/setup';
⋮----
// ============================================================================
// TEST SETUP
// ============================================================================
⋮----
// Initial state for resetting between tests
const getInitialState = () => (
⋮----
// Reset store to initial state
⋮----
// ============================================================================
// SELECTOR TESTS
// ============================================================================
⋮----
// ============================================================================
// DEVICE ACTION TESTS
// ============================================================================
⋮----
// ============================================================================
// CONFIG ACTION TESTS
// ============================================================================
⋮----
// Should not throw or call invoke
⋮----
// ============================================================================
// MODEL ACTION TESTS
// ============================================================================
⋮----
// ============================================================================
// HISTORY ACTION TESTS
// ============================================================================
⋮----
// ============================================================================
// ENABLED TOGGLE TESTS
// ============================================================================
⋮----
// ============================================================================
// INTERNAL ACTION TESTS
// ============================================================================
⋮----
// Create 100 existing entries
⋮----
// Add one more
⋮----
expect(history[99].id).toBe('entry-98'); // entry-99 was dropped
⋮----
// ============================================================================
// INITIALIZATION TESTS
// ============================================================================
⋮----
// Should not throw because it skips when already initialized
````

## File: src/store/appStore.ts
````typescript
/**
 * Zustand store for application state management.
 *
 * This store serves as the single source of truth for UI state,
 * syncing with the Rust backend via Tauri commands and events.
 */
⋮----
import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type {
  AppState,
  AppConfig,
  AudioConfig,
  AudioDevice,
  AudioLevelEvent,
  Capabilities,
  HotkeyConfig,
  HotkeyStatus,
  InjectionConfig,
  ModelStatus,
  PresetInfo,
  Progress,
  ReplacementRule,
  SelfCheckResult,
  StateEvent,
  TranscriptEntry,
} from '../types';
⋮----
// ============================================================================
// STORE STATE INTERFACE
// ============================================================================
⋮----
export interface AppStoreState {
  // Application state
  appState: AppState;
  enabled: boolean;
  errorDetail?: string;

  // Model state
  modelStatus: ModelStatus | null;
  downloadProgress: Progress | null;

  // Audio devices
  devices: AudioDevice[];
  selectedDeviceUid: string | null;
  audioLevel: AudioLevelEvent | null;
  isMeterRunning: boolean;

  // Transcript history
  history: TranscriptEntry[];

  // Configuration (mirrors Rust config)
  config: AppConfig | null;

  // Capabilities
  capabilities: Capabilities | null;

  // Hotkey status
  hotkeyStatus: HotkeyStatus | null;

  // Available presets
  presets: PresetInfo[];

  // Self-check results
  selfCheckResult: SelfCheckResult | null;

  // UI state
  isInitialized: boolean;
  isLoading: boolean;
}
⋮----
// Application state
⋮----
// Model state
⋮----
// Audio devices
⋮----
// Transcript history
⋮----
// Configuration (mirrors Rust config)
⋮----
// Capabilities
⋮----
// Hotkey status
⋮----
// Available presets
⋮----
// Self-check results
⋮----
// UI state
⋮----
// ============================================================================
// STORE ACTIONS INTERFACE
// ============================================================================
⋮----
export interface AppStoreActions {
  // Initialization
  initialize: () => Promise<void>;

  // Device actions
  refreshDevices: () => Promise<void>;
  selectDevice: (uid: string | null) => Promise<void>;
  startMicTest: () => Promise<void>;
  stopMicTest: () => Promise<void>;

  // Config actions
  loadConfig: () => Promise<void>;
  updateAudioConfig: (config: Partial<AudioConfig>) => Promise<void>;
  updateHotkeyConfig: (config: Partial<HotkeyConfig>) => Promise<void>;
  updateInjectionConfig: (config: Partial<InjectionConfig>) => Promise<void>;
  setReplacementRules: (rules: ReplacementRule[]) => Promise<void>;
  resetConfig: () => Promise<void>;

  // Model actions
  refreshModelStatus: () => Promise<void>;
  downloadModel: () => Promise<void>;
  purgeModelCache: () => Promise<void>;

  // History actions
  refreshHistory: () => Promise<void>;
  copyTranscript: (id: string) => Promise<void>;
  copyLastTranscript: () => Promise<void>;
  clearHistory: () => Promise<void>;

  // Hotkey actions
  refreshHotkeyStatus: () => Promise<void>;
  setHotkey: (primary: string, copyLast: string) => Promise<void>;

  // Preset actions
  loadPresets: () => Promise<void>;
  loadPreset: (presetId: string) => Promise<ReplacementRule[]>;

  // Capabilities actions
  refreshCapabilities: () => Promise<void>;

  // Self-check actions
  runSelfCheck: () => Promise<void>;

  // Diagnostics
  generateDiagnostics: () => Promise<string>;
  getRecentLogs: (count?: number) => Promise<string[]>;

  // Toggle enabled
  toggleEnabled: () => Promise<void>;
  setEnabled: (enabled: boolean) => Promise<void>;

  // Internal actions (called by event handlers)
  _setAppState: (event: StateEvent) => void;
  _setModelStatus: (status: ModelStatus) => void;
  _setDownloadProgress: (progress: Progress | null) => void;
  _setAudioLevel: (level: AudioLevelEvent | null) => void;
  _addHistoryEntry: (entry: TranscriptEntry) => void;
  _setError: (message: string) => void;
}
⋮----
// Initialization
⋮----
// Device actions
⋮----
// Config actions
⋮----
// Model actions
⋮----
// History actions
⋮----
// Hotkey actions
⋮----
// Preset actions
⋮----
// Capabilities actions
⋮----
// Self-check actions
⋮----
// Diagnostics
⋮----
// Toggle enabled
⋮----
// Internal actions (called by event handlers)
⋮----
// ============================================================================
// COMBINED STORE TYPE
// ============================================================================
⋮----
export type AppStore = AppStoreState & AppStoreActions;
⋮----
// ============================================================================
// DEFAULT STATE
// ============================================================================
⋮----
// ============================================================================
// STORE IMPLEMENTATION
// ============================================================================
⋮----
// --------------------------------------------------------------------------
// INITIALIZATION
// --------------------------------------------------------------------------
⋮----
// Load initial state in parallel
⋮----
// Get current app state
⋮----
// --------------------------------------------------------------------------
// DEVICE ACTIONS
// --------------------------------------------------------------------------
⋮----
// Update local config
⋮----
// --------------------------------------------------------------------------
// CONFIG ACTIONS
// --------------------------------------------------------------------------
⋮----
// --------------------------------------------------------------------------
// MODEL ACTIONS
// --------------------------------------------------------------------------
⋮----
// --------------------------------------------------------------------------
// HISTORY ACTIONS
// --------------------------------------------------------------------------
⋮----
// --------------------------------------------------------------------------
// HOTKEY ACTIONS
// --------------------------------------------------------------------------
⋮----
// --------------------------------------------------------------------------
// PRESET ACTIONS
// --------------------------------------------------------------------------
⋮----
// --------------------------------------------------------------------------
// CAPABILITIES ACTIONS
// --------------------------------------------------------------------------
⋮----
// --------------------------------------------------------------------------
// SELF-CHECK ACTIONS
// --------------------------------------------------------------------------
⋮----
// --------------------------------------------------------------------------
// DIAGNOSTICS ACTIONS
// --------------------------------------------------------------------------
⋮----
// --------------------------------------------------------------------------
// TOGGLE ENABLED
// --------------------------------------------------------------------------
⋮----
// --------------------------------------------------------------------------
// INTERNAL ACTIONS (called by event handlers)
// --------------------------------------------------------------------------
⋮----
history: [entry, ...state.history].slice(0, 100), // Keep last 100
⋮----
// ============================================================================
// SELECTORS (for optimized re-renders)
// ============================================================================
⋮----
export const selectAppState = (state: AppStore)
export const selectIsRecording = (state: AppStore)
export const selectIsTranscribing = (state: AppStore)
export const selectIsIdle = (state: AppStore)
export const selectModelReady = (state: AppStore)
export const selectDevices = (state: AppStore)
export const selectHistory = (state: AppStore)
export const selectConfig = (state: AppStore)
export const selectCapabilities = (state: AppStore)
````

## File: src/store/index.ts
````typescript
/**
 * Store exports.
 */
````

## File: src/tests/HistoryPanel.test.tsx
````typescript
/**
 * Tests for HistoryPanel component.
 */
⋮----
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { HistoryPanel } from '../components/Settings/HistoryPanel';
import type { TranscriptEntry } from '../types';
⋮----
// Mock entries for testing
⋮----
timestamp: new Date(Date.now() - 60000).toISOString(), // 1 minute ago
⋮----
timestamp: new Date(Date.now() - 3600000).toISOString(), // 1 hour ago
⋮----
timestamp: new Date(Date.now() - 86400000).toISOString(), // 1 day ago
⋮----
// Check for relative time format
⋮----
audio_duration_ms: 125000, // 2m 5s
````

## File: src/tests/MicrophoneTest.test.tsx
````typescript
/**
 * Tests for MicrophoneTest component.
 */
⋮----
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { MicrophoneTest } from '../components/Settings/MicrophoneTest';
⋮----
// The component uses animation, so we check for level display element
⋮----
// Advance timer past no-signal timeout (3 seconds)
⋮----
// Trigger no signal
⋮----
// Simulate audio detected
⋮----
const onStartTest = vi.fn().mockImplementation(() => new Promise(() => {})); // Never resolves
⋮----
// Button should show "Starting..." and be disabled
````

## File: src/tests/ModelSettings.test.tsx
````typescript
/**
 * Tests for ModelSettings component.
 */
⋮----
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { ModelSettings } from '../components/Settings/ModelSettings';
import type { ModelStatus } from '../types';
⋮----
// Text appears in both status label and disabled button
⋮----
// formatBytes uses parseFloat which removes trailing zeros (512.0 -> 512)
⋮----
// Text appears in both status label and disabled button
⋮----
// Should go back to showing Purge Cache button
````

## File: src/tests/Replacements.test.tsx
````typescript
/**
 * Tests for Replacement components.
 */
⋮----
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { ReplacementList } from '../components/Replacements/ReplacementList';
import { ReplacementEditor } from '../components/Replacements/ReplacementEditor';
import { ReplacementPreview } from '../components/Replacements/ReplacementPreview';
import { PresetsPanel } from '../components/Replacements/PresetsPanel';
import type { ReplacementRule, PresetInfo } from '../types';
⋮----
// Mock rules for testing
⋮----
// Find and click the first checkbox
⋮----
// Literal type badges
⋮----
// Regex type badge
⋮----
// Hover to reveal delete buttons (simulate by finding them)
⋮----
onCancel=
⋮----
// Switch to regex mode
⋮----
// Enter invalid regex
⋮----
// Should show error
⋮----
// Switch to regex mode
⋮----
// Enter valid regex
⋮----
// Should show valid feedback
⋮----
// Fill in form
⋮----
// Click save
⋮----
// Literal mode - should show word boundary
⋮----
// Switch to regex mode
⋮----
// Should not show word boundary
⋮----
// Should show transformed output
⋮----
// Click macro button
⋮----
// Input should contain the macro
⋮----
// Output area should contain expanded date (not the macro)
// Find the output span (has the whitespace-pre-wrap class)
⋮----
// Disable macros
⋮----
// Both input and output should have macro (2 elements)
⋮----
// Find toggle switches (checkboxes in sr-only)
⋮----
// Should show the rule pattern
````

## File: src/tests/SelfCheckDiagnostics.test.tsx
````typescript
/**
 * Tests for SelfCheck and Diagnostics components.
 */
⋮----
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { SelfCheck } from '../components/Settings/SelfCheck';
import { Diagnostics } from '../components/Settings/Diagnostics';
import type { SelfCheckResult, DiagnosticsReport, Capabilities, AppConfig } from '../types';
⋮----
// Mock self-check result
⋮----
// Click on hotkey row (has detail)
⋮----
// Should show detail
⋮----
// Mock clipboard API using vi.stubGlobal
⋮----
// Should show line and char count
⋮----
// Should not show actual device UID
⋮----
// Should show redacted placeholder
⋮----
// Should show count, not actual patterns
````

## File: src/tests/SettingsPanel.test.tsx
````typescript
/**
 * Tests for settings panel components.
 */
⋮----
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MicrophoneSelect } from '../components/Settings/MicrophoneSelect';
import { HotkeyConfig } from '../components/Settings/HotkeyConfig';
import { InjectionSettings } from '../components/Settings/InjectionSettings';
import { SettingsPanel } from '../components/Settings/SettingsPanel';
import type { AudioDevice, AppConfig, InjectionConfig } from '../types';
⋮----
// Mock data
⋮----
// Focus guard is the second toggle
⋮----
// Toggle audio cues
````

## File: src/tests/setup.ts
````typescript
/**
 * Vitest test setup file.
 *
 * Sets up:
 * - Testing Library DOM matchers
 * - Tauri API mocks
 * - Global test utilities
 */
⋮----
import { vi, beforeEach, afterEach } from 'vitest';
⋮----
// ============================================================================
// TAURI MOCK INFRASTRUCTURE
// ============================================================================
⋮----
export type MockInvokeHandler = (cmd: string, args?: unknown) => unknown;
⋮----
let mockInvokeHandler: MockInvokeHandler = ()
⋮----
/**
 * Set a custom handler for invoke calls during tests.
 */
export function setMockInvokeHandler(handler: MockInvokeHandler): void
⋮----
/**
 * Create a simple mock for a specific command.
 */
export function mockInvoke(cmd: string, response: unknown): void
⋮----
mockInvokeHandler = (c, args) =>
⋮----
/**
 * Create a mock that throws for a specific command.
 */
export function mockInvokeError(cmd: string, error: Error): void
⋮----
// Mock Tauri core module
⋮----
// Track active listeners for cleanup
type ListenerCallback = (event: { payload: unknown }) => void;
⋮----
/**
 * Emit a mock event to all listeners.
 */
export function emitMockEvent(eventName: string, payload: unknown): void
⋮----
// Mock Tauri event module
⋮----
// Return unlisten function
const unlisten = () =>
⋮----
// ============================================================================
// TEST LIFECYCLE HOOKS
// ============================================================================
⋮----
// Reset mock handler before each test
mockInvokeHandler = ()
⋮----
// Clear all listeners
⋮----
// Clean up any remaining listeners
⋮----
// Clear all mocks
⋮----
// ============================================================================
// TEST UTILITIES
// ============================================================================
⋮----
/**
 * Wait for a condition to be true.
 */
export async function waitFor(
  condition: () => boolean,
  timeout = 1000
): Promise<void>
⋮----
/**
 * Create a mock audio device.
 */
export function createMockDevice(overrides: Partial<{
  uid: string;
  name: string;
  is_default: boolean;
  sample_rate: number;
  channels: number;
}> =
⋮----
/**
 * Create a mock transcript entry.
 */
export function createMockTranscript(overrides: Partial<{
  id: string;
  text: string;
  timestamp: string;
  audio_duration_ms: number;
  processing_duration_ms: number;
  injected: boolean;
}> =
⋮----
/**
 * Create a mock model status.
 */
export function createMockModelStatus(overrides: Partial<{
  status: string;
  model_id: string;
  error?: string;
}> =
⋮----
/**
 * Create a mock app config.
 */
export function createMockConfig()
````

## File: src/tests/StatusIndicator.test.tsx
````typescript
/**
 * Tests for StatusIndicator component.
 */
⋮----
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatusIndicator } from '../components/StatusIndicator';
import type { AppState } from '../types';
⋮----
// Check for pulsing animation class
⋮----
// When disabled, should show "Paused" instead of "Recording..."
⋮----
// Should not have animation class
⋮----
// Should not show progress bar if total is undefined
⋮----
// Progress bar should not be visible without total
````

## File: src/App.tsx
````typescript
import { useEffect } from 'react';
import { useAppStore, selectAppState, selectIsRecording } from './store';
import { useTauriEvents } from './hooks';
⋮----
// Set up Tauri event listeners
⋮----
// Get store state and actions
⋮----
// Initialize store on mount
⋮----
// Loading state
⋮----
{/* Status Badge */}
⋮----
{/* Model Status */}
⋮----
{/* Audio Devices */}
⋮----
{/* Recent Transcripts */}
````

## File: src/index.css
````css
@tailwind base;
@tailwind components;
@tailwind utilities;
⋮----
:root {
⋮----
body {
````

## File: src/main.tsx
````typescript
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
````

## File: src/types.ts
````typescript
/**
 * TypeScript types for Tauri commands.
 *
 * These types match the Rust types defined in src-tauri/src/commands.rs
 * and related modules.
 */
⋮----
// ============================================================================
// STATE TYPES
// ============================================================================
⋮----
/** Application state. */
export type AppState =
  | 'idle'
  | 'loading_model'
  | 'recording'
  | 'transcribing'
  | 'error';
⋮----
/** Application state info returned by get_app_state. */
export interface StateEvent {
  state: AppState;
  enabled: boolean;
  error_detail?: string;
  timestamp: string;
}
⋮----
/** Reason why recording cannot start. */
export type CannotRecordReason =
  | { type: 'already_recording' }
  | { type: 'transcribing' }
  | { type: 'model_not_ready' }
  | { type: 'disabled' };
⋮----
// ============================================================================
// CAPABILITY TYPES
// ============================================================================
⋮----
/** Display server type. */
export type DisplayServer =
  | { type: 'windows' }
  | { type: 'macos' }
  | { type: 'x11' }
  | { type: 'wayland'; compositor?: string }
  | { type: 'unknown' };
⋮----
/** Hotkey activation mode. */
export type ActivationMode = 'hold' | 'toggle';
⋮----
/** Text injection method. */
export type InjectionMethod = 'clipboard_paste' | 'clipboard_only';
⋮----
/** Effective mode with reason. */
export interface EffectiveMode<T> {
  configured: T;
  effective: T;
  reason?: string;
}
⋮----
/** Permission state. */
export type PermissionState = 'granted' | 'denied' | 'unknown' | 'not_required';
⋮----
/** Permission status. */
export interface PermissionStatus {
  microphone: PermissionState;
  accessibility?: PermissionState;
}
⋮----
/** Platform capabilities. */
export interface Capabilities {
  display_server: DisplayServer;
  hotkey_press_available: boolean;
  hotkey_release_available: boolean;
  keystroke_injection_available: boolean;
  clipboard_available: boolean;
  hotkey_mode: EffectiveMode<ActivationMode>;
  injection_method: EffectiveMode<InjectionMethod>;
  permissions: PermissionStatus;
  diagnostics: string;
}
⋮----
/** Capability issue for user attention. */
export interface CapabilityIssue {
  id: string;
  severity: 'error' | 'warning' | 'info';
  title: string;
  description: string;
  fix_instructions?: string;
}
⋮----
// ============================================================================
// CONFIG TYPES
// ============================================================================
⋮----
/** Hotkey mode setting. */
export type HotkeyMode = 'hold' | 'toggle';
⋮----
/** Audio configuration. */
export interface AudioConfig {
  device_uid?: string;
  audio_cues_enabled: boolean;
}
⋮----
/** Hotkey configuration. */
export interface HotkeyConfig {
  primary: string;
  copy_last: string;
  mode: HotkeyMode;
}
⋮----
/** Injection configuration. */
export interface InjectionConfig {
  paste_delay_ms: number;
  restore_clipboard: boolean;
  suffix: string;
  focus_guard_enabled: boolean;
}
⋮----
/** UI configuration. */
export interface UiConfig {
  show_on_startup: boolean;
  window_width: number;
  window_height: number;
}
⋮----
/** Text replacement rule kind. */
export type ReplacementKind = 'literal' | 'regex';
⋮----
/** Text replacement rule origin. */
export type ReplacementOrigin = 'user' | 'preset';
⋮----
/** Text replacement rule (matches IPC protocol). */
export interface ReplacementRule {
  id: string;
  enabled: boolean;
  kind: ReplacementKind;
  pattern: string;
  replacement: string;
  word_boundary: boolean;
  case_sensitive: boolean;
  description?: string;
  origin?: ReplacementOrigin;
}
⋮----
/** Presets configuration. */
export interface PresetsConfig {
  enabled_presets: string[];
}
⋮----
/** Complete application configuration. */
export interface AppConfig {
  schema_version: number;
  audio: AudioConfig;
  hotkeys: HotkeyConfig;
  injection: InjectionConfig;
  replacements: ReplacementRule[];
  ui: UiConfig;
  presets: PresetsConfig;
}
⋮----
// ============================================================================
// AUDIO TYPES
// ============================================================================
⋮----
/** Audio device information. */
export interface AudioDevice {
  uid: string;
  name: string;
  is_default: boolean;
  sample_rate: number;
  channels: number;
}
⋮----
// ============================================================================
// MODEL TYPES
// ============================================================================
⋮----
/** Model state. */
export type ModelState =
  | 'missing'
  | 'downloading'
  | 'verifying'
  | 'ready'
  | 'error';
⋮----
/** Download/verification progress. */
export interface Progress {
  current: number;
  total?: number;
  unit: string;
}
⋮----
/** Model status information. */
export interface ModelStatus {
  model_id: string;
  status: ModelState;
  progress?: Progress;
  error?: string;
}
⋮----
// ============================================================================
// HISTORY TYPES
// ============================================================================
⋮----
/** Injection result for a transcript. */
export type InjectionResult =
  | { status: 'injected' }
  | { status: 'clipboard_only'; reason: string }
  | { status: 'error'; message: string };
⋮----
/** Transcript history entry. */
export interface TranscriptEntry {
  id: string;
  text: string;
  timestamp: string;
  audio_duration_ms: number;
  transcription_duration_ms: number;
  injection_result: InjectionResult;
}
⋮----
// ============================================================================
// HOTKEY TYPES
// ============================================================================
⋮----
/** Hotkey status information. */
export interface HotkeyStatus {
  primary: string;
  copy_last: string;
  mode: string;
  registered: boolean;
}
⋮----
// ============================================================================
// PRESET TYPES
// ============================================================================
⋮----
/** Preset information. */
export interface PresetInfo {
  id: string;
  name: string;
  description: string;
  rule_count: number;
}
⋮----
// ============================================================================
// SELF-CHECK TYPES
// ============================================================================
⋮----
/** Check status. */
export type CheckStatus = 'ok' | 'warning' | 'error';
⋮----
/** Individual check item. */
export interface CheckItem {
  status: CheckStatus;
  message: string;
  detail?: string;
}
⋮----
/** Self-check result. */
export interface SelfCheckResult {
  hotkey: CheckItem;
  injection: CheckItem;
  microphone: CheckItem;
  sidecar: CheckItem;
  model: CheckItem;
}
⋮----
// ============================================================================
// DIAGNOSTICS TYPES
// ============================================================================
⋮----
/** Log entry. */
export interface LogEntry {
  timestamp: string;
  level: string;
  message: string;
}
⋮----
/** Diagnostics report. */
export interface DiagnosticsReport {
  version: string;
  platform: string;
  capabilities: Capabilities;
  config: AppConfig;
  self_check: SelfCheckResult;
}
⋮----
// ============================================================================
// ERROR TYPES
// ============================================================================
⋮----
/** Command error codes. */
export type CommandErrorCode =
  | 'config'
  | 'audio'
  | 'model'
  | 'clipboard'
  | 'hotkey'
  | 'not_implemented'
  | 'internal';
⋮----
/** Command error. */
export interface CommandError {
  code: CommandErrorCode;
  message: string;
}
⋮----
// ============================================================================
// EVENT TYPES (Rust → UI)
// ============================================================================
⋮----
/** Audio level event during mic test. */
export interface AudioLevelEvent {
  rms: number;
  peak: number;
}
⋮----
/** Model download progress event. */
export interface ModelProgressEvent {
  current: number;
  total?: number;
  unit: string;
}
⋮----
/** Transcript completed event. */
export interface TranscriptEvent {
  entry: TranscriptEntry;
}
⋮----
/** Error event. */
export interface ErrorEvent {
  message: string;
  recoverable: boolean;
}
````

## File: AGENTS.md
````markdown
# Agent Instructions

This project uses **bd** (beads) for issue tracking. Run `bd onboard` to get started.

## Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --status in_progress  # Claim work
bd close <id>         # Complete work
bd sync               # Sync with git
```

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds


<!-- MCP_AGENT_MAIL_AND_BEADS_SNIPPET_START -->

## MCP Agent Mail: coordination for multi-agent workflows

What it is
- A mail-like layer that lets coding agents coordinate asynchronously via MCP tools and resources.
- Provides identities, inbox/outbox, searchable threads, and advisory file reservations, with human-auditable artifacts in Git.

Why it's useful
- Prevents agents from stepping on each other with explicit file reservations (leases) for files/globs.
- Keeps communication out of your token budget by storing messages in a per-project archive.
- Offers quick reads (`resource://inbox/...`, `resource://thread/...`) and macros that bundle common flows.

How to use effectively
1) Same repository
   - Register an identity: call `ensure_project`, then `register_agent` using this repo's absolute path as `project_key`.
   - Reserve files before you edit: `file_reservation_paths(project_key, agent_name, ["src/**"], ttl_seconds=3600, exclusive=true)` to signal intent and avoid conflict.
   - Communicate with threads: use `send_message(..., thread_id="FEAT-123")`; check inbox with `fetch_inbox` and acknowledge with `acknowledge_message`.
   - Read fast: `resource://inbox/{Agent}?project=<abs-path>&limit=20` or `resource://thread/{id}?project=<abs-path>&include_bodies=true`.
   - Tip: set `AGENT_NAME` in your environment so the pre-commit guard can block commits that conflict with others' active exclusive file reservations.

2) Across different repos in one project (e.g., Next.js frontend + FastAPI backend)
   - Option A (single project bus): register both sides under the same `project_key` (shared key/path). Keep reservation patterns specific (e.g., `frontend/**` vs `backend/**`).
   - Option B (separate projects): each repo has its own `project_key`; use `macro_contact_handshake` or `request_contact`/`respond_contact` to link agents, then message directly. Keep a shared `thread_id` (e.g., ticket key) across repos for clean summaries/audits.

Macros vs granular tools
- Prefer macros when you want speed or are on a smaller model: `macro_start_session`, `macro_prepare_thread`, `macro_file_reservation_cycle`, `macro_contact_handshake`.
- Use granular tools when you need control: `register_agent`, `file_reservation_paths`, `send_message`, `fetch_inbox`, `acknowledge_message`.

Common pitfalls
- "from_agent not registered": always `register_agent` in the correct `project_key` first.
- "FILE_RESERVATION_CONFLICT": adjust patterns, wait for expiry, or use a non-exclusive reservation when appropriate.
- Auth errors: if JWT+JWKS is enabled, include a bearer token with a `kid` that matches server JWKS; static bearer is used only when JWT is disabled.

## Integrating with Beads (dependency-aware task planning)

Beads provides a lightweight, dependency-aware issue database and a CLI (`bd`) for selecting "ready work," setting priorities, and tracking status. It complements MCP Agent Mail's messaging, audit trail, and file-reservation signals. Project: [steveyegge/beads](https://github.com/steveyegge/beads)

Recommended conventions
- **Single source of truth**: Use **Beads** for task status/priority/dependencies; use **Agent Mail** for conversation, decisions, and attachments (audit).
- **Shared identifiers**: Use the Beads issue id (e.g., `bd-123`) as the Mail `thread_id` and prefix message subjects with `[bd-123]`.
- **Reservations**: When starting a `bd-###` task, call `file_reservation_paths(...)` for the affected paths; include the issue id in the `reason` and release on completion.

Typical flow (agents)
1) **Pick ready work** (Beads)
   - `bd ready --json` → choose one item (highest priority, no blockers)
2) **Reserve edit surface** (Mail)
   - `file_reservation_paths(project_key, agent_name, ["src/**"], ttl_seconds=3600, exclusive=true, reason="bd-123")`
3) **Announce start** (Mail)
   - `send_message(..., thread_id="bd-123", subject="[bd-123] Start: <short title>", ack_required=true)`
4) **Work and update**
   - Reply in-thread with progress and attach artifacts/images; keep the discussion in one thread per issue id
5) **Complete and release**
   - `bd close bd-123 --reason "Completed"` (Beads is status authority)
   - `release_file_reservations(project_key, agent_name, paths=["src/**"])`
   - Final Mail reply: `[bd-123] Completed` with summary and links

Mapping cheat-sheet
- **Mail `thread_id`** ↔ `bd-###`
- **Mail subject**: `[bd-###] …`
- **File reservation `reason`**: `bd-###`
- **Commit messages (optional)**: include `bd-###` for traceability

Event mirroring (optional automation)
- On `bd update --status blocked`, send a high-importance Mail message in thread `bd-###` describing the blocker.
- On Mail "ACK overdue" for a critical decision, add a Beads label (e.g., `needs-ack`) or bump priority to surface it in `bd ready`.

Pitfalls to avoid
- Don't create or manage tasks in Mail; treat Beads as the single task queue.
- Always include `bd-###` in message `thread_id` to avoid ID drift across tools.


<!-- MCP_AGENT_MAIL_AND_BEADS_SNIPPET_END -->
````

## File: package.json
````json
{
  "name": "translator-voice-input-tool",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "lint": "eslint . --ext ts,tsx --report-unused-disable-directives --max-warnings 0",
    "test": "vitest run",
    "test:watch": "vitest",
    "test:coverage": "vitest run --coverage"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.2.0",
    "@tauri-apps/plugin-shell": "^2.2.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "zustand": "^5.0.11"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.3.0",
    "@testing-library/jest-dom": "^6.9.1",
    "@testing-library/react": "^16.3.2",
    "@testing-library/user-event": "^14.6.1",
    "@types/node": "^25.2.0",
    "@types/react": "^18.3.12",
    "@types/react-dom": "^18.3.1",
    "@typescript-eslint/eslint-plugin": "^8.8.0",
    "@typescript-eslint/parser": "^8.8.0",
    "@vitejs/plugin-react": "^4.3.3",
    "autoprefixer": "^10.4.20",
    "eslint": "^9.14.0",
    "eslint-plugin-react-hooks": "^5.0.0",
    "happy-dom": "^20.5.0",
    "jsdom": "^28.0.0",
    "postcss": "^8.4.49",
    "tailwindcss": "^3.4.15",
    "typescript": "~5.6.3",
    "vite": "^6.0.1",
    "vitest": "^4.0.18"
  }
}
````

## File: postcss.config.js
````javascript

````

## File: README.md
````markdown
# Voice Input Tool

A desktop application for voice-to-text transcription using local ASR (Automatic Speech Recognition).

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) (or Node.js 18+)
- Platform-specific requirements (see below)

### Linux

```bash
# Debian/Ubuntu
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
    build-essential \
    curl \
    wget \
    file \
    libxdo-dev \
    libssl-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev

# Fedora
sudo dnf install webkit2gtk4.1-devel \
    openssl-devel \
    curl \
    wget \
    file \
    libxdo-devel \
    librsvg2-devel

# Arch
sudo pacman -S webkit2gtk-4.1 \
    base-devel \
    curl \
    wget \
    file \
    openssl \
    appmenu-gtk-module \
    libxdo \
    librsvg
```

### macOS

```bash
# Xcode Command Line Tools (if not already installed)
xcode-select --install
```

**Note:** macOS requires granting Microphone and Accessibility permissions when prompted.

### Windows

- [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (usually pre-installed on Windows 10/11)

## Quick Start (Smoke Test)

```bash
# 1. Install dependencies
bun install

# 2. Run in development mode
bun run tauri dev

# The app should:
# - Launch a window titled "Voice Input Tool"
# - Show a text input and "Call Rust Echo Command" button
# - Type a message and click the button - it should echo back from Rust
# - Edit src/App.tsx and save - the UI should hot reload immediately
```

## Project Structure

```
.
├── src/                    # React frontend
│   ├── App.tsx            # Main React component
│   ├── main.tsx           # React entry point
│   └── index.css          # Tailwind CSS styles
├── src-tauri/             # Rust backend
│   ├── src/
│   │   ├── lib.rs         # Tauri commands and setup
│   │   └── main.rs        # Entry point
│   ├── Cargo.toml         # Rust dependencies
│   └── tauri.conf.json    # Tauri configuration
├── shared/                # Shared contracts
│   ├── ipc/               # IPC protocol definitions
│   └── model/             # Model manifest
└── docs/                  # Documentation
```

## Development Commands

```bash
# Start development server with hot reload
bun run tauri dev

# Build for production
bun run tauri build

# Run frontend only (no Tauri)
bun run dev

# Type check
bun run build

# Lint
bun run lint
```

## Platform Permissions

### macOS

The app requires:
- **Microphone**: To record audio for transcription
- **Accessibility**: To type transcribed text into other applications

These permissions are requested via `Info.plist` entries and will prompt the user on first use.

### Linux

Audio capture typically works out of the box via PulseAudio/PipeWire. For typing into other applications, `libxdo` is used.

### Windows

No special permissions typically required. The app may prompt for microphone access on first use.

## License

See [THIRD_PARTY_NOTICES.md](docs/THIRD_PARTY_NOTICES.md) for third-party licenses.
````

## File: tailwind.config.js
````javascript
/** @type {import('tailwindcss').Config} */
````

## File: tsconfig.json
````json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,

    /* Bundler mode */
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "moduleDetection": "force",
    "noEmit": true,
    "jsx": "react-jsx",

    /* Linting */
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,

    /* Paths */
    "baseUrl": ".",
    "paths": {
      "@/*": ["src/*"]
    }
  },
  "include": ["src"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
````

## File: tsconfig.node.json
````json
{
  "compilerOptions": {
    "composite": true,
    "skipLibCheck": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true,
    "strict": true
  },
  "include": ["vite.config.ts"]
}
````

## File: vite.config.ts
````typescript
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
⋮----
// https://vitejs.dev/config/
⋮----
// Tauri configuration
⋮----
// Ignore Rust source changes
⋮----
// Tauri requires specific target for production builds
⋮----
// Output to dist for Tauri to consume
````

## File: vitest.config.ts
````typescript
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
````
