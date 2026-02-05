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
