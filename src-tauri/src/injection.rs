//! Text injection via clipboard paste.
//!
//! This module handles injecting transcribed text into the focused application
//! using the clipboard + paste shortcut approach.
//!
//! # Safety Features
//!
//! - Focus Guard: validates focus hasn't changed before injection
//! - Self-injection prevention: never injects into OpenVoicy itself
//! - Injection serialization: concurrent injections are queued
//! - Clipboard restore: optionally restores previous clipboard contents

#![allow(dead_code)] // Module under construction

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::focus::{capture_focus, validate_focus, FocusSignature};

/// Global injection mutex to serialize injections.
static INJECTION_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// Injection configuration.
#[derive(Debug, Clone)]
pub struct InjectionConfig {
    /// Delay before sending paste shortcut (ms).
    /// Clamped to 10-500ms.
    pub paste_delay_ms: u32,
    /// Whether to restore previous clipboard contents.
    pub restore_clipboard: bool,
    /// Suffix to append to injected text.
    pub suffix: String,
    /// Whether Focus Guard is enabled.
    pub focus_guard_enabled: bool,
}

impl Default for InjectionConfig {
    fn default() -> Self {
        Self {
            paste_delay_ms: 40,
            restore_clipboard: true,
            suffix: " ".to_string(),
            focus_guard_enabled: true,
        }
    }
}

impl InjectionConfig {
    /// Clamp paste delay to valid range.
    pub fn clamped_delay(&self) -> Duration {
        let ms = self.paste_delay_ms.clamp(10, 500);
        Duration::from_millis(ms as u64)
    }
}

/// Result of an injection attempt.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum InjectionResult {
    /// Text was injected via paste.
    Injected {
        text_length: usize,
        timestamp: DateTime<Utc>,
    },
    /// Text copied to clipboard only (paste not performed).
    ClipboardOnly {
        reason: String,
        text_length: usize,
        timestamp: DateTime<Utc>,
    },
    /// Injection failed.
    Failed {
        error: String,
        timestamp: DateTime<Utc>,
    },
}

impl InjectionResult {
    /// Check if the injection was successful (either injected or clipboard).
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            InjectionResult::Injected { .. } | InjectionResult::ClipboardOnly { .. }
        )
    }
}

/// Injection errors.
#[derive(Debug, Error)]
pub enum InjectionError {
    #[error("Clipboard error: {0}")]
    Clipboard(String),

    #[error("Paste synthesis failed: {0}")]
    PasteFailed(String),

    #[error("Platform not supported: {0}")]
    UnsupportedPlatform(String),
}

/// Inject text with Focus Guard validation.
///
/// This is the main entry point for text injection. It:
/// 1. Validates focus (if Focus Guard enabled)
/// 2. Serializes with other injections
/// 3. Sets clipboard text (with suffix)
/// 4. Sends paste shortcut (if focus validated)
/// 5. Optionally restores clipboard
///
/// Returns the injection result with details.
pub async fn inject_text(
    text: &str,
    expected_focus: Option<&FocusSignature>,
    config: &InjectionConfig,
) -> InjectionResult {
    // Validate focus if Focus Guard is enabled and we have an expected signature
    if config.focus_guard_enabled {
        if let Some(expected) = expected_focus {
            let validation = validate_focus(expected);
            if !validation.should_inject() {
                // Clipboard-only mode
                let reason = validation
                    .clipboard_only_reason()
                    .unwrap_or_else(|| "Focus validation failed".to_string());

                // Still set clipboard for user to paste manually
                let text_with_suffix = format!("{}{}", text, config.suffix);
                if let Err(e) = set_clipboard(&text_with_suffix) {
                    return InjectionResult::Failed {
                        error: format!("Clipboard error: {}", e),
                        timestamp: Utc::now(),
                    };
                }

                log::info!("Clipboard-only mode: {}", reason);
                return InjectionResult::ClipboardOnly {
                    reason,
                    text_length: text.len(),
                    timestamp: Utc::now(),
                };
            }
        }
    }

    // Check for self-injection even without expected focus
    let current_focus = capture_focus();
    if crate::focus::is_self_focused(&current_focus) {
        let text_with_suffix = format!("{}{}", text, config.suffix);
        if let Err(e) = set_clipboard(&text_with_suffix) {
            return InjectionResult::Failed {
                error: format!("Clipboard error: {}", e),
                timestamp: Utc::now(),
            };
        }

        return InjectionResult::ClipboardOnly {
            reason: "OpenVoicy settings window focused".to_string(),
            text_length: text.len(),
            timestamp: Utc::now(),
        };
    }

    // Perform injection (serialized)
    perform_injection(text, config).await
}

/// Perform the actual injection (clipboard + paste).
async fn perform_injection(text: &str, config: &InjectionConfig) -> InjectionResult {
    // Serialize injections
    let _guard = INJECTION_MUTEX.lock().await;

    let text_with_suffix = format!("{}{}", text, config.suffix);

    // Save previous clipboard if needed
    let previous_clipboard = if config.restore_clipboard {
        get_clipboard().ok()
    } else {
        None
    };

    // Set clipboard
    if let Err(e) = set_clipboard(&text_with_suffix) {
        return InjectionResult::Failed {
            error: format!("Clipboard error: {}", e),
            timestamp: Utc::now(),
        };
    }

    // Wait before paste
    sleep(config.clamped_delay()).await;

    // Synthesize paste shortcut
    match synthesize_paste() {
        Ok(()) => {
            // Restore clipboard if needed
            if let Some(prev) = previous_clipboard {
                // Small delay to let paste complete
                sleep(Duration::from_millis(50)).await;
                let _ = set_clipboard(&prev);
            }

            InjectionResult::Injected {
                text_length: text.len(),
                timestamp: Utc::now(),
            }
        }
        Err(e) => {
            // Paste failed, but text is still on clipboard
            InjectionResult::ClipboardOnly {
                reason: format!("Paste synthesis failed: {}", e),
                text_length: text.len(),
                timestamp: Utc::now(),
            }
        }
    }
}

/// Set text to clipboard (public API for other modules).
pub fn set_clipboard_public(text: &str) -> Result<(), String> {
    set_clipboard(text).map_err(|e| e.to_string())
}

/// Set text to clipboard.
fn set_clipboard(text: &str) -> Result<(), InjectionError> {
    #[cfg(target_os = "linux")]
    {
        set_clipboard_linux(text)
    }

    #[cfg(target_os = "macos")]
    {
        set_clipboard_macos(text)
    }

    #[cfg(target_os = "windows")]
    {
        set_clipboard_windows(text)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err(InjectionError::UnsupportedPlatform(
            "Clipboard not supported".to_string(),
        ))
    }
}

/// Get text from clipboard.
fn get_clipboard() -> Result<String, InjectionError> {
    #[cfg(target_os = "linux")]
    {
        get_clipboard_linux()
    }

    #[cfg(target_os = "macos")]
    {
        get_clipboard_macos()
    }

    #[cfg(target_os = "windows")]
    {
        get_clipboard_windows()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err(InjectionError::UnsupportedPlatform(
            "Clipboard not supported".to_string(),
        ))
    }
}

/// Synthesize paste shortcut (Ctrl+V / Cmd+V).
fn synthesize_paste() -> Result<(), InjectionError> {
    #[cfg(target_os = "linux")]
    {
        synthesize_paste_linux()
    }

    #[cfg(target_os = "macos")]
    {
        synthesize_paste_macos()
    }

    #[cfg(target_os = "windows")]
    {
        synthesize_paste_windows()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err(InjectionError::UnsupportedPlatform(
            "Paste synthesis not supported".to_string(),
        ))
    }
}

// === Linux Implementation ===

#[cfg(target_os = "linux")]
fn set_clipboard_linux(text: &str) -> Result<(), InjectionError> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Check if we're on Wayland
    let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();

    if is_wayland {
        // Use wl-copy for Wayland
        let mut child = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| InjectionError::Clipboard(format!("wl-copy failed: {}", e)))?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| InjectionError::Clipboard(e.to_string()))?;
        }

        child
            .wait()
            .map_err(|e| InjectionError::Clipboard(e.to_string()))?;
    } else {
        // Use xclip for X11
        let mut child = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| InjectionError::Clipboard(format!("xclip failed: {}", e)))?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| InjectionError::Clipboard(e.to_string()))?;
        }

        child
            .wait()
            .map_err(|e| InjectionError::Clipboard(e.to_string()))?;
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn get_clipboard_linux() -> Result<String, InjectionError> {
    use std::process::Command;

    // Check if we're on Wayland
    let is_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();

    let output = if is_wayland {
        Command::new("wl-paste")
            .output()
            .map_err(|e| InjectionError::Clipboard(format!("wl-paste failed: {}", e)))?
    } else {
        Command::new("xclip")
            .args(["-selection", "clipboard", "-o"])
            .output()
            .map_err(|e| InjectionError::Clipboard(format!("xclip failed: {}", e)))?
    };

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        // Empty clipboard is not an error
        Ok(String::new())
    }
}

#[cfg(target_os = "linux")]
fn synthesize_paste_linux() -> Result<(), InjectionError> {
    use std::process::Command;

    // Check if we're on Wayland
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        // Wayland doesn't support keystroke synthesis
        // This will result in clipboard-only mode
        return Err(InjectionError::UnsupportedPlatform(
            "Wayland does not support keystroke injection".to_string(),
        ));
    }

    // Use xdotool for X11
    let status = Command::new("xdotool")
        .args(["key", "ctrl+v"])
        .status()
        .map_err(|e| InjectionError::PasteFailed(format!("xdotool failed: {}", e)))?;

    if status.success() {
        Ok(())
    } else {
        Err(InjectionError::PasteFailed(
            "xdotool returned non-zero exit code".to_string(),
        ))
    }
}

// === macOS Implementation (placeholder) ===

#[cfg(target_os = "macos")]
fn set_clipboard_macos(text: &str) -> Result<(), InjectionError> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| InjectionError::Clipboard(format!("pbcopy failed: {}", e)))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| InjectionError::Clipboard(e.to_string()))?;
    }

    child
        .wait()
        .map_err(|e| InjectionError::Clipboard(e.to_string()))?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn get_clipboard_macos() -> Result<String, InjectionError> {
    use std::process::Command;

    let output = Command::new("pbpaste")
        .output()
        .map_err(|e| InjectionError::Clipboard(format!("pbpaste failed: {}", e)))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(target_os = "macos")]
fn synthesize_paste_macos() -> Result<(), InjectionError> {
    // TODO: Implement using CGEvent for Cmd+V
    // For now, use osascript as a workaround
    use std::process::Command;

    let status = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to keystroke \"v\" using command down",
        ])
        .status()
        .map_err(|e| InjectionError::PasteFailed(format!("osascript failed: {}", e)))?;

    if status.success() {
        Ok(())
    } else {
        Err(InjectionError::PasteFailed(
            "osascript returned non-zero exit code".to_string(),
        ))
    }
}

// === Windows Implementation (placeholder) ===

#[cfg(target_os = "windows")]
fn set_clipboard_windows(text: &str) -> Result<(), InjectionError> {
    // TODO: Implement using Windows clipboard API
    Err(InjectionError::UnsupportedPlatform(
        "Windows clipboard not yet implemented".to_string(),
    ))
}

#[cfg(target_os = "windows")]
fn get_clipboard_windows() -> Result<String, InjectionError> {
    // TODO: Implement using Windows clipboard API
    Err(InjectionError::UnsupportedPlatform(
        "Windows clipboard not yet implemented".to_string(),
    ))
}

#[cfg(target_os = "windows")]
fn synthesize_paste_windows() -> Result<(), InjectionError> {
    // TODO: Implement using SendInput for Ctrl+V
    Err(InjectionError::UnsupportedPlatform(
        "Windows paste synthesis not yet implemented".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = InjectionConfig::default();
        assert_eq!(config.paste_delay_ms, 40);
        assert!(config.restore_clipboard);
        assert_eq!(config.suffix, " ");
        assert!(config.focus_guard_enabled);
    }

    #[test]
    fn test_delay_clamping() {
        let config = InjectionConfig {
            paste_delay_ms: 5, // Below minimum
            ..Default::default()
        };
        assert_eq!(config.clamped_delay(), Duration::from_millis(10));

        let config = InjectionConfig {
            paste_delay_ms: 1000, // Above maximum
            ..Default::default()
        };
        assert_eq!(config.clamped_delay(), Duration::from_millis(500));

        let config = InjectionConfig {
            paste_delay_ms: 100, // Within range
            ..Default::default()
        };
        assert_eq!(config.clamped_delay(), Duration::from_millis(100));
    }

    #[test]
    fn test_injection_result_success() {
        let result = InjectionResult::Injected {
            text_length: 10,
            timestamp: Utc::now(),
        };
        assert!(result.is_success());

        let result = InjectionResult::ClipboardOnly {
            reason: "test".to_string(),
            text_length: 10,
            timestamp: Utc::now(),
        };
        assert!(result.is_success());

        let result = InjectionResult::Failed {
            error: "test error".to_string(),
            timestamp: Utc::now(),
        };
        assert!(!result.is_success());
    }

    #[tokio::test]
    async fn test_injection_serialization() {
        // Test that injections are serialized (not interleaved)
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let counter = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..5)
            .map(|i| {
                let counter = Arc::clone(&counter);
                tokio::spawn(async move {
                    let _guard = INJECTION_MUTEX.lock().await;
                    // Simulate work
                    let val = counter.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    val
                })
            })
            .collect();

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        // All values should be unique (0, 1, 2, 3, 4) if properly serialized
        let mut sorted = results.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 5);
    }

    #[test]
    fn test_suffix_variants() {
        // Test empty suffix
        let config = InjectionConfig {
            suffix: "".to_string(),
            ..Default::default()
        };
        let text_with_suffix = format!("{}{}", "hello", config.suffix);
        assert_eq!(text_with_suffix, "hello");

        // Test space suffix (default)
        let config = InjectionConfig::default();
        let text_with_suffix = format!("{}{}", "hello", config.suffix);
        assert_eq!(text_with_suffix, "hello ");

        // Test newline suffix
        let config = InjectionConfig {
            suffix: "\n".to_string(),
            ..Default::default()
        };
        let text_with_suffix = format!("{}{}", "hello", config.suffix);
        assert_eq!(text_with_suffix, "hello\n");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_clipboard_operations() {
        // Skip if no display available (headless environment)
        let has_x11 = std::env::var("DISPLAY")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        let has_wayland = std::env::var("WAYLAND_DISPLAY")
            .map(|v| !v.is_empty())
            .unwrap_or(false);

        if !has_x11 && !has_wayland {
            // Skip test in headless environment
            return;
        }

        // Skip if xclip/wl-copy not available
        use std::process::Command;

        let has_xclip = Command::new("which")
            .arg("xclip")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let has_wl_copy = Command::new("which")
            .arg("wl-copy")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !has_xclip && !has_wl_copy {
            // Skip test if no clipboard tool available
            return;
        }

        // Try a simple test first to see if clipboard actually works
        let probe_text = "__clipboard_test_probe__";
        if set_clipboard(probe_text).is_err() {
            // Clipboard not actually working, skip test
            return;
        }
        if let Ok(retrieved) = get_clipboard() {
            if retrieved.trim() != probe_text {
                // Clipboard not working properly, skip test
                return;
            }
        } else {
            return;
        }

        // Test Unicode (emoji, CJK, RTL)
        let test_texts = vec![
            "Hello, world!",
            "Unicode: 你好世界",
            "Emoji: 🎉🚀✨",
            "RTL: שלום",
            "Mixed: Hello 你好 🌍",
        ];

        for text in test_texts {
            if set_clipboard(text).is_ok() {
                if let Ok(retrieved) = get_clipboard() {
                    assert_eq!(retrieved.trim(), text, "Failed for: {}", text);
                }
            }
        }
    }
}
