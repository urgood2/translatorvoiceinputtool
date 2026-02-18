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

fn normalize_focus_component(raw: &str) -> Option<String> {
    let normalized: String = raw
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
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
        Some(compact.chars().take(64).collect())
    }
}

fn compose_window_id(
    platform: &str,
    pid: Option<&str>,
    title: Option<&str>,
    app_name: &str,
) -> String {
    let pid = pid.map(str::trim).filter(|value| !value.is_empty());
    let title_key = title.and_then(normalize_focus_component);
    let app_key = normalize_focus_component(app_name);

    match (pid, title_key, app_key) {
        (Some(pid), Some(title_key), _) => format!("{platform}-{pid}-{title_key}"),
        (Some(pid), None, _) => format!("{platform}-{pid}"),
        (None, Some(title_key), _) => format!("{platform}-{title_key}"),
        (None, None, Some(app_key)) => format!("{platform}-{app_key}"),
        _ => "unknown".to_string(),
    }
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
    let (pid, app_name_raw) = get_frontmost_app_macos();
    let window_title = get_front_window_title_macos();

    let app_name = if app_name_raw.trim().is_empty() {
        "Unknown (macOS)".to_string()
    } else {
        app_name_raw
    };
    let process_name = pid
        .as_deref()
        .and_then(get_process_name_macos)
        .unwrap_or_else(|| app_name.clone());
    let display_name = window_title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| app_name.clone());

    FocusSignature {
        window_id: compose_window_id("macos", pid.as_deref(), window_title.as_deref(), &app_name),
        process_name,
        app_name: display_name,
        captured_at: Instant::now(),
        timestamp: Utc::now(),
    }
}

#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Option<String> {
    use std::process::Command;

    let output = Command::new("osascript")
        .args(["-e", script])
        .output()
        .ok()
        .filter(|result| result.status.success())?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

#[cfg(target_os = "macos")]
fn get_frontmost_app_macos() -> (Option<String>, String) {
    let script = r#"tell application "System Events" to tell first application process whose frontmost is true to return (unix id as string) & tab & name"#;
    if let Some(output) = run_osascript(script) {
        let mut parts = output.splitn(2, '\t');
        let pid = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let app_name = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Unknown (macOS)");
        return (pid.map(ToString::to_string), app_name.to_string());
    }
    (None, "Unknown (macOS)".to_string())
}

#[cfg(target_os = "macos")]
fn get_front_window_title_macos() -> Option<String> {
    let script = r#"tell application "System Events" to tell first application process whose frontmost is true to if (count of windows) > 0 then return name of front window else return """#;
    run_osascript(script)
}

#[cfg(target_os = "macos")]
fn get_process_name_macos(pid: &str) -> Option<String> {
    use std::{path::Path, process::Command};

    let output = Command::new("ps")
        .args(["-p", pid, "-o", "comm="])
        .output()
        .ok()
        .filter(|result| result.status.success())?;
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        None
    } else {
        Some(
            Path::new(&raw)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(&raw)
                .to_string(),
        )
    }
}

// === Windows Implementation ===

#[cfg(target_os = "windows")]
fn capture_focus_windows() -> FocusSignature {
    let (pid, process_name_raw, window_title) = get_foreground_window_info_windows();
    let process_name = if process_name_raw.trim().is_empty() {
        "unknown".to_string()
    } else {
        process_name_raw
    };
    let display_name = window_title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| process_name.clone());

    FocusSignature {
        window_id: compose_window_id(
            "windows",
            pid.as_deref(),
            window_title.as_deref(),
            &display_name,
        ),
        process_name,
        app_name: display_name,
        captured_at: Instant::now(),
        timestamp: Utc::now(),
    }
}

#[cfg(target_os = "windows")]
fn run_powershell(script: &str) -> Option<String> {
    use std::process::Command;

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .ok()
        .filter(|result| result.status.success())?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

#[cfg(target_os = "windows")]
fn get_foreground_window_info_windows() -> (Option<String>, String, Option<String>) {
    let script = r#"$ErrorActionPreference='SilentlyContinue'; Add-Type -Namespace Win32 -Name User32 -MemberDefinition '[DllImport("user32.dll")] public static extern System.IntPtr GetForegroundWindow(); [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint lpdwProcessId); [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(System.IntPtr hWnd, System.Text.StringBuilder text, int count);'; $h=[Win32.User32]::GetForegroundWindow(); if ($h -eq [System.IntPtr]::Zero) { return }; $pid=0; [Win32.User32]::GetWindowThreadProcessId($h, [ref]$pid) | Out-Null; $p=Get-Process -Id $pid -ErrorAction SilentlyContinue; $name=if ($p) { $p.ProcessName } else { 'unknown' }; $sb=New-Object System.Text.StringBuilder 1024; [Win32.User32]::GetWindowText($h, $sb, $sb.Capacity) | Out-Null; $title=$sb.ToString(); Write-Output ($pid.ToString() + \"`t\" + $name + \"`t\" + $title)"#;

    let output = match run_powershell(script) {
        Some(output) => output,
        None => return (None, "unknown".to_string(), None),
    };

    let mut parts = output.splitn(3, '\t');
    let pid = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let process_name = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown")
        .to_string();
    let title = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    (pid.map(ToString::to_string), process_name, title)
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

    #[test]
    fn test_compose_window_id_prefers_pid_and_title() {
        assert_eq!(
            compose_window_id("windows", Some("1234"), Some("Visual Studio Code"), "Code"),
            "windows-1234-visual-studio-code"
        );
    }

    #[test]
    fn test_compose_window_id_falls_back_to_app_name() {
        assert_eq!(
            compose_window_id("macos", None, None, "Finder"),
            "macos-finder"
        );
    }
}
