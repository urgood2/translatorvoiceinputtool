//! Platform capability detection module.
//!
//! Detects platform-specific capabilities for global hotkeys, text injection,
//! and computes effective modes based on what the system supports.
//!
//! # Platform Support
//!
//! | Platform | Hotkey Press | Hotkey Release | Text Injection |
//! |----------|-------------|----------------|----------------|
//! | Windows  | ✓           | ✓              | SendInput      |
//! | macOS    | ✓ (needs accessibility) | ✓ | CGEvent        |
//! | Linux X11| ✓           | ✓              | xdotool/XTest  |
//! | Linux Wayland | ✓ (portal) | ⚠ (limited) | Clipboard only |

// Platform-conditional code paths mean some variants appear unused on any given platform
#![allow(dead_code)]

use serde::Serialize;
use std::env;

/// Activation mode for voice recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationMode {
    /// Hold hotkey to record, release to transcribe.
    PushToTalk,
    /// Tap hotkey to start, tap again to stop.
    Toggle,
}

/// Method for injecting transcribed text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionMethod {
    /// Inject keystrokes directly + clipboard fallback.
    ClipboardPaste,
    /// Copy to clipboard only (user pastes manually).
    ClipboardOnly,
}

/// Permission state for a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    /// Permission has been granted.
    Granted,
    /// Permission has been explicitly denied.
    Denied,
    /// Permission has not been requested yet.
    NotDetermined,
    /// Permission check is not applicable on this platform.
    NotApplicable,
}

/// Display server / window system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum DisplayServer {
    Windows,
    MacOS,
    X11,
    Wayland {
        #[serde(skip_serializing_if = "Option::is_none")]
        compositor: Option<String>,
    },
    Unknown,
}

/// Effective mode with configured value, actual value, and reason.
#[derive(Debug, Clone, Serialize)]
pub struct EffectiveMode<T: Serialize + Clone> {
    /// What the user configured (or the default).
    pub configured: T,
    /// What will actually be used after platform constraints.
    pub effective: T,
    /// Human-readable reason for why effective may differ from configured.
    pub reason: String,
}

/// Permission status for various capabilities.
#[derive(Debug, Clone, Serialize)]
pub struct PermissionStatus {
    /// Microphone permission state.
    pub microphone: PermissionState,
    /// Accessibility permission state (macOS).
    pub accessibility: PermissionState,
}

/// Platform capabilities detection result.
#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    /// Detected display server.
    pub display_server: DisplayServer,
    /// Whether global hotkey press detection is available.
    pub hotkey_press_available: bool,
    /// Whether global hotkey release detection is available.
    pub hotkey_release_available: bool,
    /// Whether keystroke injection is available.
    pub keystroke_injection_available: bool,
    /// Whether clipboard access is available.
    pub clipboard_available: bool,
    /// Effective hotkey mode after platform constraints.
    pub hotkey_mode: EffectiveMode<ActivationMode>,
    /// Effective injection method after platform constraints.
    pub injection_method: EffectiveMode<InjectionMethod>,
    /// Permission status.
    pub permissions: PermissionStatus,
    /// Human-readable diagnostics text.
    pub diagnostics: String,
}

impl Capabilities {
    /// Detect capabilities for the current platform.
    ///
    /// This performs synchronous capability detection. Some checks
    /// (like portal availability on Wayland) may require async in
    /// the future.
    pub fn detect() -> Self {
        Self::detect_with_defaults(ActivationMode::PushToTalk, InjectionMethod::ClipboardPaste)
    }

    /// Detect capabilities with specific configured defaults.
    pub fn detect_with_defaults(
        configured_hotkey: ActivationMode,
        configured_injection: InjectionMethod,
    ) -> Self {
        let display_server = detect_display_server();

        // Determine capabilities based on platform
        let (hotkey_press, hotkey_release, keystroke_injection) = match &display_server {
            DisplayServer::Windows => (true, true, true),
            DisplayServer::MacOS => {
                // macOS needs accessibility for hotkeys and injection
                let accessibility = check_macos_accessibility();
                let has_accessibility = accessibility == PermissionState::Granted;
                (has_accessibility, has_accessibility, has_accessibility)
            }
            DisplayServer::X11 => (true, true, check_xdotool_available()),
            DisplayServer::Wayland { .. } => {
                // Wayland: press via portal, release is unreliable, no keystroke injection
                let portal_available = check_wayland_portal();
                (portal_available, false, false)
            }
            DisplayServer::Unknown => (false, false, false),
        };

        // Compute effective hotkey mode
        let hotkey_mode = compute_effective_hotkey_mode(configured_hotkey, hotkey_release);

        // Compute effective injection method
        let injection_method =
            compute_effective_injection_method(configured_injection, keystroke_injection);

        // Get permission status
        let permissions = detect_permissions(&display_server);

        // Generate diagnostics
        let diagnostics = generate_diagnostics(
            &display_server,
            hotkey_press,
            hotkey_release,
            keystroke_injection,
            &permissions,
        );

        Self {
            display_server,
            hotkey_press_available: hotkey_press,
            hotkey_release_available: hotkey_release,
            keystroke_injection_available: keystroke_injection,
            clipboard_available: true, // Clipboard is available on all platforms
            hotkey_mode,
            injection_method,
            permissions,
            diagnostics,
        }
    }

    /// Returns a list of issues that need user attention.
    pub fn issues(&self) -> Vec<CapabilityIssue> {
        let mut issues = Vec::new();

        // Check accessibility on macOS
        if matches!(self.display_server, DisplayServer::MacOS)
            && self.permissions.accessibility != PermissionState::Granted
        {
            issues.push(CapabilityIssue {
                severity: IssueSeverity::Blocking,
                category: "permissions".to_string(),
                title: "Accessibility Permission Required".to_string(),
                description: "Voice Input Tool needs accessibility permission to detect global hotkeys and type transcribed text.".to_string(),
                remediation: Some("Open System Preferences → Security & Privacy → Privacy → Accessibility and enable Voice Input Tool.".to_string()),
            });
        }

        // Check microphone permission
        if self.permissions.microphone == PermissionState::Denied {
            issues.push(CapabilityIssue {
                severity: IssueSeverity::Blocking,
                category: "permissions".to_string(),
                title: "Microphone Permission Denied".to_string(),
                description: "Voice Input Tool cannot access the microphone.".to_string(),
                remediation: Some(
                    "Grant microphone permission in system settings.".to_string(),
                ),
            });
        }

        // Check Wayland limitations
        if matches!(self.display_server, DisplayServer::Wayland { .. }) {
            issues.push(CapabilityIssue {
                severity: IssueSeverity::Warning,
                category: "platform".to_string(),
                title: "Wayland Detected - Limited Functionality".to_string(),
                description: "Wayland security restrictions affect hotkey and text injection.".to_string(),
                remediation: Some("Using toggle mode and clipboard injection for best compatibility.".to_string()),
            });
        }

        // Check xdotool on X11
        if matches!(self.display_server, DisplayServer::X11) && !self.keystroke_injection_available
        {
            issues.push(CapabilityIssue {
                severity: IssueSeverity::Warning,
                category: "dependencies".to_string(),
                title: "Missing Dependency: xdotool".to_string(),
                description: "Text injection requires xdotool but it was not found.".to_string(),
                remediation: Some(
                    "Install xdotool: sudo apt install xdotool (Debian/Ubuntu)".to_string(),
                ),
            });
        }

        issues
    }
}

/// Issue severity level.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    /// Issue prevents core functionality.
    Blocking,
    /// Issue affects some functionality.
    Warning,
    /// Informational note.
    Info,
}

/// A capability issue that needs user attention.
#[derive(Debug, Clone, Serialize)]
pub struct CapabilityIssue {
    /// Severity of the issue.
    pub severity: IssueSeverity,
    /// Category (permissions, platform, dependencies).
    pub category: String,
    /// Short title for the issue.
    pub title: String,
    /// Detailed description.
    pub description: String,
    /// How to fix the issue, if applicable.
    pub remediation: Option<String>,
}

// === Platform Detection Functions ===

/// Detect the display server / window system.
fn detect_display_server() -> DisplayServer {
    #[cfg(target_os = "windows")]
    {
        DisplayServer::Windows
    }

    #[cfg(target_os = "macos")]
    {
        DisplayServer::MacOS
    }

    #[cfg(target_os = "linux")]
    {
        // Check for Wayland first (WAYLAND_DISPLAY takes precedence)
        if env::var("WAYLAND_DISPLAY").is_ok() {
            // Try to detect compositor
            let compositor = detect_wayland_compositor();
            DisplayServer::Wayland { compositor }
        } else if env::var("DISPLAY").is_ok() {
            DisplayServer::X11
        } else {
            DisplayServer::Unknown
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        DisplayServer::Unknown
    }
}

/// Detect the Wayland compositor from environment variables.
#[cfg(target_os = "linux")]
fn detect_wayland_compositor() -> Option<String> {
    // XDG_CURRENT_DESKTOP is commonly set
    if let Ok(desktop) = env::var("XDG_CURRENT_DESKTOP") {
        return Some(desktop);
    }

    // DESKTOP_SESSION as fallback
    if let Ok(session) = env::var("DESKTOP_SESSION") {
        return Some(session);
    }

    // Check for specific compositor environment variables
    if env::var("GNOME_DESKTOP_SESSION_ID").is_ok() {
        return Some("GNOME".to_string());
    }

    if env::var("KDE_FULL_SESSION").is_ok() {
        return Some("KDE".to_string());
    }

    None
}

/// Check if xdotool is available on the system.
#[cfg(target_os = "linux")]
fn check_xdotool_available() -> bool {
    std::process::Command::new("which")
        .arg("xdotool")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn check_xdotool_available() -> bool {
    false
}

/// Check if Wayland portal GlobalShortcuts is available.
///
/// This is a simplified check. A full implementation would use zbus
/// to query the portal service.
#[cfg(target_os = "linux")]
fn check_wayland_portal() -> bool {
    // Check if XDG_DESKTOP_PORTAL is available
    // This is a heuristic - actual portal availability requires D-Bus query
    env::var("XDG_RUNTIME_DIR").is_ok()
}

#[cfg(not(target_os = "linux"))]
fn check_wayland_portal() -> bool {
    false
}

/// Check macOS accessibility permission.
///
/// This is a placeholder - actual implementation would use objc or swift.
#[cfg(target_os = "macos")]
fn check_macos_accessibility() -> PermissionState {
    // In a real implementation, we would call AXIsProcessTrusted()
    // For now, return NotDetermined as we haven't implemented the check
    PermissionState::NotDetermined
}

#[cfg(not(target_os = "macos"))]
fn check_macos_accessibility() -> PermissionState {
    PermissionState::NotApplicable
}

// === Mode Computation ===

/// Compute the effective hotkey mode based on platform capabilities.
fn compute_effective_hotkey_mode(
    configured: ActivationMode,
    release_available: bool,
) -> EffectiveMode<ActivationMode> {
    if release_available {
        // Platform supports release events, use configured mode
        EffectiveMode {
            configured,
            effective: configured,
            reason: "Platform supports key release events".to_string(),
        }
    } else {
        // No release support, force toggle mode
        EffectiveMode {
            configured,
            effective: ActivationMode::Toggle,
            reason: "Key release events not available, using toggle mode".to_string(),
        }
    }
}

/// Compute the effective injection method based on platform capabilities.
fn compute_effective_injection_method(
    configured: InjectionMethod,
    keystroke_available: bool,
) -> EffectiveMode<InjectionMethod> {
    if keystroke_available {
        // Platform supports keystroke injection
        EffectiveMode {
            configured,
            effective: configured,
            reason: "Platform supports keystroke injection".to_string(),
        }
    } else {
        // No keystroke support, force clipboard only
        EffectiveMode {
            configured,
            effective: InjectionMethod::ClipboardOnly,
            reason: "Keystroke injection not available, using clipboard only".to_string(),
        }
    }
}

// === Permission Detection ===

/// Detect permission status for the platform.
fn detect_permissions(display_server: &DisplayServer) -> PermissionStatus {
    let accessibility = match display_server {
        DisplayServer::MacOS => check_macos_accessibility(),
        _ => PermissionState::NotApplicable,
    };

    // Microphone permission would need platform-specific checks
    // For now, return NotDetermined as we'll check on first use
    let microphone = PermissionState::NotDetermined;

    PermissionStatus {
        microphone,
        accessibility,
    }
}

// === Diagnostics Generation ===

/// Generate human-readable diagnostics text.
fn generate_diagnostics(
    display_server: &DisplayServer,
    hotkey_press: bool,
    hotkey_release: bool,
    keystroke_injection: bool,
    permissions: &PermissionStatus,
) -> String {
    let mut lines = Vec::new();

    // Platform header
    let platform_name = match display_server {
        DisplayServer::Windows => "Windows".to_string(),
        DisplayServer::MacOS => "macOS".to_string(),
        DisplayServer::X11 => "Linux X11".to_string(),
        DisplayServer::Wayland { compositor } => {
            if let Some(comp) = compositor {
                format!("Linux Wayland ({})", comp)
            } else {
                "Linux Wayland".to_string()
            }
        }
        DisplayServer::Unknown => "Unknown Platform".to_string(),
    };
    lines.push(format!("{} Platform Detected", platform_name));
    lines.push(String::new());

    // Hotkey capabilities
    lines.push("Global Hotkey Support:".to_string());
    if hotkey_press {
        lines.push("  ✓ Key press detection available".to_string());
    } else {
        lines.push("  ✗ Key press detection NOT available".to_string());
    }
    if hotkey_release {
        lines.push("  ✓ Key release detection available".to_string());
    } else {
        lines.push("  ⚠ Key release detection NOT available (toggle mode recommended)".to_string());
    }
    lines.push(String::new());

    // Text injection
    lines.push("Text Injection:".to_string());
    if keystroke_injection {
        lines.push("  ✓ Direct keystroke injection available".to_string());
    } else {
        lines.push("  ⚠ Direct injection NOT available (clipboard mode only)".to_string());
    }
    lines.push("  ✓ Clipboard access available".to_string());
    lines.push(String::new());

    // Permissions (if applicable)
    if matches!(display_server, DisplayServer::MacOS) {
        lines.push("Permissions:".to_string());
        match permissions.accessibility {
            PermissionState::Granted => {
                lines.push("  ✓ Accessibility permission granted".to_string())
            }
            PermissionState::Denied => {
                lines.push("  ✗ Accessibility permission DENIED".to_string())
            }
            PermissionState::NotDetermined => {
                lines.push("  ? Accessibility permission not yet requested".to_string())
            }
            PermissionState::NotApplicable => {}
        }
        match permissions.microphone {
            PermissionState::Granted => {
                lines.push("  ✓ Microphone permission granted".to_string())
            }
            PermissionState::Denied => lines.push("  ✗ Microphone permission DENIED".to_string()),
            PermissionState::NotDetermined => {
                lines.push("  ? Microphone permission not yet requested".to_string())
            }
            PermissionState::NotApplicable => {}
        }
    }

    // Wayland-specific notes
    if matches!(display_server, DisplayServer::Wayland { .. }) {
        lines.push("⚠ WAYLAND NOTES:".to_string());
        lines.push("  • Push-to-talk mode may not work reliably".to_string());
        lines.push("  • Direct text injection is not supported".to_string());
        lines.push("  • Text will be copied to clipboard; paste with Ctrl+V".to_string());
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_display_server_returns_valid() {
        let ds = detect_display_server();
        // Should always return something, not panic
        match ds {
            DisplayServer::Windows
            | DisplayServer::MacOS
            | DisplayServer::X11
            | DisplayServer::Wayland { .. }
            | DisplayServer::Unknown => {}
        }
    }

    #[test]
    fn test_capabilities_detect_returns_valid() {
        let caps = Capabilities::detect();
        // Should always succeed
        assert!(caps.clipboard_available); // Clipboard is always available
    }

    #[test]
    fn test_effective_hotkey_mode_with_release() {
        let mode = compute_effective_hotkey_mode(ActivationMode::PushToTalk, true);
        assert_eq!(mode.effective, ActivationMode::PushToTalk);
    }

    #[test]
    fn test_effective_hotkey_mode_without_release() {
        let mode = compute_effective_hotkey_mode(ActivationMode::PushToTalk, false);
        assert_eq!(mode.effective, ActivationMode::Toggle);
        assert!(mode.reason.contains("toggle"));
    }

    #[test]
    fn test_effective_injection_with_keystroke() {
        let method = compute_effective_injection_method(InjectionMethod::ClipboardPaste, true);
        assert_eq!(method.effective, InjectionMethod::ClipboardPaste);
    }

    #[test]
    fn test_effective_injection_without_keystroke() {
        let method = compute_effective_injection_method(InjectionMethod::ClipboardPaste, false);
        assert_eq!(method.effective, InjectionMethod::ClipboardOnly);
        assert!(method.reason.contains("clipboard"));
    }

    #[test]
    fn test_diagnostics_not_empty() {
        let caps = Capabilities::detect();
        assert!(!caps.diagnostics.is_empty());
        assert!(caps.diagnostics.contains("Platform Detected"));
    }

    #[test]
    fn test_issues_returns_list() {
        let caps = Capabilities::detect();
        let issues = caps.issues();
        // Should return a list (may be empty depending on platform)
        // Just verify it doesn't panic
        let _ = issues;
    }
}
