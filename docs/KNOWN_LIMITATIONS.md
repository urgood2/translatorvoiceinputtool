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
