//! Focus capture and validation for injection safety.
//!
//! This module provides Focus Guard functionality to prevent mis-injection
//! when the user switches applications between recording and transcription.
//!
//! # How Focus Guard Works
//!
//! 1. When recording stops, capture the current focus signature
//! 2. When transcription completes, validate focus before injecting
//! 3. If focus changed or OpenVoicy is focused, use clipboard-only mode

#![allow(dead_code)] // Module under construction

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::time::Instant;

/// Focus signature capturing foreground window information.
#[derive(Debug, Clone, Serialize)]
pub struct FocusSignature {
    /// Platform-specific window identifier.
    pub window_id: String,
    /// Process name (for self-injection detection).
    pub process_name: String,
    /// Human-readable application name.
    pub app_name: String,
    /// When this signature was captured.
    #[serde(skip)]
    pub captured_at: Instant,
    /// Timestamp for serialization.
    pub timestamp: DateTime<Utc>,
}

/// Result of focus validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum FocusValidation {
    /// Focus is the same window.
    Same,
    /// Focus changed to a different window.
    Changed { from_app: String, to_app: String },
    /// OpenVoicy itself is focused (self-injection prevention).
    SelfFocused,
    /// Focus capture not available on this platform.
    Unavailable,
}

impl FocusValidation {
    /// Check if injection should proceed.
    pub fn should_inject(&self) -> bool {
        matches!(self, FocusValidation::Same)
    }

    /// Get a human-readable reason for clipboard-only mode.
    pub fn clipboard_only_reason(&self) -> Option<String> {
        match self {
            FocusValidation::Same => None,
            FocusValidation::Changed { from_app, to_app } => {
                Some(format!("Focus changed from {} to {}", from_app, to_app))
            }
            FocusValidation::SelfFocused => Some("OpenVoicy settings window focused".to_string()),
            FocusValidation::Unavailable => {
                Some("Focus detection unavailable on this platform".to_string())
            }
        }
    }
}

/// Capture the current focus signature.
pub fn capture_focus() -> FocusSignature {
    #[cfg(target_os = "linux")]
    {
        capture_focus_linux()
    }

    #[cfg(target_os = "macos")]
    {
        capture_focus_macos()
    }

    #[cfg(target_os = "windows")]
    {
        capture_focus_windows()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        FocusSignature {
            window_id: "unknown".to_string(),
            process_name: "unknown".to_string(),
            app_name: "Unknown".to_string(),
            captured_at: Instant::now(),
            timestamp: Utc::now(),
        }
    }
}

/// Validate that focus matches the expected signature.
pub fn validate_focus(expected: &FocusSignature) -> FocusValidation {
    let current = capture_focus();

    // Check for self-injection first
    if is_self_focused(&current) {
        return FocusValidation::SelfFocused;
    }

    // Check if window ID matches
    if current.window_id == expected.window_id {
        FocusValidation::Same
    } else {
        FocusValidation::Changed {
            from_app: expected.app_name.clone(),
            to_app: current.app_name.clone(),
        }
    }
}

/// Check if OpenVoicy itself is focused.
pub fn is_self_focused(sig: &FocusSignature) -> bool {
    let process_lower = sig.process_name.to_lowercase();

    // Check various patterns for our app name
    process_lower.contains("openvoicy")
        || process_lower.contains("voice-input-tool")
        || process_lower.contains("translator-voice-input-tool")
        || process_lower.contains("voiceinputtool")
}

/// Normalize an app identifier for config matching.
///
/// This allows case-insensitive matching and tolerates minor formatting
/// differences (spaces/underscores/hyphens).
pub fn normalize_app_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_lowercase();
    if trimmed.is_empty() {
        return None;
    }

    // Windows process names commonly include .exe; treat it as equivalent.
    let without_ext = trimmed.strip_suffix(".exe").unwrap_or(&trimmed);
    let normalized: String = without_ext
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch
            } else if ch == ' ' || ch == '_' || ch == '-' {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|ch| *ch != '\0')
        .collect();

    let mut compact = String::with_capacity(normalized.len());
    let mut last_dash = false;
    for ch in normalized.trim_matches('-').chars() {
        if ch == '-' {
            if !last_dash {
                compact.push(ch);
                last_dash = true;
            }
        } else {
            compact.push(ch);
            last_dash = false;
        }
    }
    if compact.is_empty() {
        None
    } else {
        Some(compact)
    }
}

/// Candidate app identifiers for per-app override matching.
///
/// Returns normalized keys derived from both process name and app/window title.
pub fn app_override_candidates(sig: &FocusSignature) -> Vec<String> {
    let mut out = Vec::new();

    if let Some(process) = normalize_app_id(&sig.process_name) {
        out.push(process);
    }

    if let Some(app) = normalize_app_id(&sig.app_name) {
        if !out.contains(&app) {
            out.push(app);
        }
    }

    // Add token-level matches for window titles like "channel - Slack".
    for token in sig.app_name.split_whitespace() {
        if let Some(candidate) = normalize_app_id(token) {
            if !out.contains(&candidate) {
                out.push(candidate);
            }
        }
    }

    out
}

// === Linux Implementation ===

#[cfg(target_os = "linux")]
fn capture_focus_linux() -> FocusSignature {
    // Try using xdotool to get active window info
    let window_id = get_active_window_id_linux();
    let (process_name, app_name) = if window_id != "unknown" {
        get_window_info_linux(&window_id)
    } else {
        ("unknown".to_string(), "Unknown".to_string())
    };

    FocusSignature {
        window_id,
        process_name,
        app_name,
        captured_at: Instant::now(),
        timestamp: Utc::now(),
    }
}

#[cfg(target_os = "linux")]
fn get_active_window_id_linux() -> String {
    use std::process::Command;

    // Check if we're on Wayland (can't get window info)
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        return "wayland-unavailable".to_string();
    }

    // Use xdotool to get active window
    let output = Command::new("xdotool").args(["getactivewindow"]).output();

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => "unknown".to_string(),
    }
}

#[cfg(target_os = "linux")]
fn get_window_info_linux(window_id: &str) -> (String, String) {
    use std::process::Command;

    // Get process ID for the window
    let pid = Command::new("xdotool")
        .args(["getwindowpid", window_id])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    // Get process name from PID
    let process_name = pid
        .as_ref()
        .and_then(|p| {
            std::fs::read_to_string(format!("/proc/{}/comm", p))
                .ok()
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    // Get window name
    let app_name = Command::new("xdotool")
        .args(["getwindowname", window_id])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
            // Truncate long window names
            if name.len() > 50 {
                format!("{}...", &name[..47])
            } else if name.is_empty() {
                process_name.clone()
            } else {
                name
            }
        })
        .unwrap_or_else(|| process_name.clone());

    (process_name, app_name)
}

// === macOS Implementation (placeholder) ===

#[cfg(target_os = "macos")]
fn capture_focus_macos() -> FocusSignature {
    // TODO: Implement using NSWorkspace and CGWindowListCopyWindowInfo
    // For now, return a placeholder
    FocusSignature {
        window_id: "macos-todo".to_string(),
        process_name: "unknown".to_string(),
        app_name: "Unknown (macOS)".to_string(),
        captured_at: Instant::now(),
        timestamp: Utc::now(),
    }
}

// === Windows Implementation (placeholder) ===

#[cfg(target_os = "windows")]
fn capture_focus_windows() -> FocusSignature {
    // TODO: Implement using GetForegroundWindow and GetWindowThreadProcessId
    // For now, return a placeholder
    FocusSignature {
        window_id: "windows-todo".to_string(),
        process_name: "unknown".to_string(),
        app_name: "Unknown (Windows)".to_string(),
        captured_at: Instant::now(),
        timestamp: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_self_focused_detection() {
        // Test various patterns
        assert!(is_self_focused(&FocusSignature {
            window_id: "123".to_string(),
            process_name: "openvoicy".to_string(),
            app_name: "OpenVoicy".to_string(),
            captured_at: Instant::now(),
            timestamp: Utc::now(),
        }));

        assert!(is_self_focused(&FocusSignature {
            window_id: "123".to_string(),
            process_name: "voice-input-tool".to_string(),
            app_name: "Voice Input Tool".to_string(),
            captured_at: Instant::now(),
            timestamp: Utc::now(),
        }));

        assert!(is_self_focused(&FocusSignature {
            window_id: "123".to_string(),
            process_name: "translator-voice-input-tool".to_string(),
            app_name: "App".to_string(),
            captured_at: Instant::now(),
            timestamp: Utc::now(),
        }));

        // Case insensitive
        assert!(is_self_focused(&FocusSignature {
            window_id: "123".to_string(),
            process_name: "OpenVoicy".to_string(),
            app_name: "App".to_string(),
            captured_at: Instant::now(),
            timestamp: Utc::now(),
        }));
    }

    #[test]
    fn test_not_self_focused() {
        assert!(!is_self_focused(&FocusSignature {
            window_id: "123".to_string(),
            process_name: "firefox".to_string(),
            app_name: "Firefox".to_string(),
            captured_at: Instant::now(),
            timestamp: Utc::now(),
        }));

        assert!(!is_self_focused(&FocusSignature {
            window_id: "123".to_string(),
            process_name: "code".to_string(),
            app_name: "Visual Studio Code".to_string(),
            captured_at: Instant::now(),
            timestamp: Utc::now(),
        }));
    }

    #[test]
    fn test_normalize_app_id() {
        assert_eq!(normalize_app_id(" Slack "), Some("slack".to_string()));
        assert_eq!(normalize_app_id("Code.exe"), Some("code".to_string()));
        assert_eq!(
            normalize_app_id("Visual Studio Code"),
            Some("visual-studio-code".to_string())
        );
        assert_eq!(normalize_app_id(""), None);
    }

    #[test]
    fn test_app_override_candidates() {
        let sig = FocusSignature {
            window_id: "1".to_string(),
            process_name: "slack".to_string(),
            app_name: "general - Slack".to_string(),
            captured_at: Instant::now(),
            timestamp: Utc::now(),
        };

        let candidates = app_override_candidates(&sig);
        assert!(candidates.contains(&"slack".to_string()));
        assert!(candidates.contains(&"general-slack".to_string()));
    }

    #[test]
    fn test_focus_validation_same() {
        let sig = FocusSignature {
            window_id: "12345".to_string(),
            process_name: "firefox".to_string(),
            app_name: "Firefox".to_string(),
            captured_at: Instant::now(),
            timestamp: Utc::now(),
        };

        // Create a validation result for same focus
        let validation = FocusValidation::Same;
        assert!(validation.should_inject());
        assert!(validation.clipboard_only_reason().is_none());
    }

    #[test]
    fn test_focus_validation_changed() {
        let validation = FocusValidation::Changed {
            from_app: "Firefox".to_string(),
            to_app: "Terminal".to_string(),
        };

        assert!(!validation.should_inject());
        assert!(validation
            .clipboard_only_reason()
            .unwrap()
            .contains("Focus changed"));
    }

    #[test]
    fn test_focus_validation_self_focused() {
        let validation = FocusValidation::SelfFocused;

        assert!(!validation.should_inject());
        assert!(validation
            .clipboard_only_reason()
            .unwrap()
            .contains("OpenVoicy"));
    }

    #[test]
    fn test_capture_focus_does_not_panic() {
        // Should not panic even if xdotool isn't available
        let sig = capture_focus();
        assert!(!sig.window_id.is_empty());
    }
}
