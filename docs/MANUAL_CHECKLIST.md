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
