//! Tauri commands exposed to the UI.
//!
//! This module provides the complete API surface between the React UI
//! and the Rust backend via Tauri commands.

use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

use std::sync::Arc;

use crate::capabilities::{Capabilities, CapabilityIssue};
use crate::config::{self, AppConfig, ReplacementRule};
use crate::history::{TranscriptEntry, TranscriptHistory};
use crate::state::{AppStateManager, CannotRecordReason, StateEvent};

/// Command error types.
#[derive(Debug, Error, Serialize)]
#[serde(rename_all = "snake_case", tag = "code")]
pub enum CommandError {
    #[error("Config error: {message}")]
    Config { message: String },

    #[error("Audio error: {message}")]
    Audio { message: String },

    #[error("Model error: {message}")]
    Model { message: String },

    #[error("Clipboard error: {message}")]
    Clipboard { message: String },

    #[error("Hotkey error: {message}")]
    Hotkey { message: String },

    #[error("Not implemented: {message}")]
    NotImplemented { message: String },

    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl From<config::ConfigError> for CommandError {
    fn from(e: config::ConfigError) -> Self {
        CommandError::Config {
            message: e.to_string(),
        }
    }
}

// ============================================================================
// STATE COMMANDS
// ============================================================================

/// Get current application state.
#[tauri::command]
pub fn get_app_state(state_manager: tauri::State<Arc<AppStateManager>>) -> StateEvent {
    state_manager.get_event()
}

/// Get platform capabilities.
#[tauri::command]
pub fn get_capabilities() -> Capabilities {
    Capabilities::detect()
}

/// Get capability issues that need user attention.
#[tauri::command]
pub fn get_capability_issues() -> Vec<CapabilityIssue> {
    Capabilities::detect().issues()
}

/// Check if recording can start.
#[tauri::command]
pub fn can_start_recording(
    state_manager: tauri::State<Arc<AppStateManager>>,
) -> Result<(), CannotRecordReason> {
    state_manager.can_start_recording()
}

/// Self-check result for diagnostics.
#[derive(Debug, Clone, Serialize)]
pub struct SelfCheckResult {
    pub hotkey: CheckItem,
    pub injection: CheckItem,
    pub microphone: CheckItem,
    pub sidecar: CheckItem,
    pub model: CheckItem,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckItem {
    pub status: CheckStatus,
    pub message: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
}

/// Run self-check for all subsystems.
#[tauri::command]
pub fn run_self_check() -> SelfCheckResult {
    let caps = Capabilities::detect();

    // Check injection capability
    let injection = if caps.keystroke_injection_available {
        CheckItem {
            status: CheckStatus::Ok,
            message: "Text injection available".to_string(),
            detail: None,
        }
    } else {
        CheckItem {
            status: CheckStatus::Warning,
            message: "Clipboard-only mode".to_string(),
            detail: Some("Keystroke synthesis not available on this platform".to_string()),
        }
    };

    // Check hotkey capability
    let hotkey = if caps.hotkey_release_available {
        CheckItem {
            status: CheckStatus::Ok,
            message: "Hold-to-record available".to_string(),
            detail: None,
        }
    } else {
        CheckItem {
            status: CheckStatus::Warning,
            message: "Toggle mode only".to_string(),
            detail: Some("Key release detection not available".to_string()),
        }
    };

    SelfCheckResult {
        hotkey,
        injection,
        microphone: CheckItem {
            status: CheckStatus::Ok,
            message: "Not yet implemented".to_string(),
            detail: None,
        },
        sidecar: CheckItem {
            status: CheckStatus::Ok,
            message: "Not yet implemented".to_string(),
            detail: None,
        },
        model: CheckItem {
            status: CheckStatus::Ok,
            message: "Not yet implemented".to_string(),
            detail: None,
        },
    }
}

// ============================================================================
// CONFIG COMMANDS
// ============================================================================

/// Get current configuration.
#[tauri::command]
pub fn get_config() -> AppConfig {
    config::load_config()
}

/// Update configuration.
#[tauri::command]
pub fn update_config(config: AppConfig) -> Result<(), CommandError> {
    let mut config = config;
    config.validate_and_clamp();
    config::save_config(&config)?;
    Ok(())
}

/// Reset configuration to defaults.
#[tauri::command]
pub fn reset_config_to_defaults() -> Result<AppConfig, CommandError> {
    let config = AppConfig::default();
    config::save_config(&config)?;
    Ok(config)
}

// ============================================================================
// AUDIO COMMANDS
// ============================================================================

/// Audio device information.
#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub uid: String,
    pub name: String,
    pub is_default: bool,
    pub sample_rate: u32,
    pub channels: u32,
}

/// List available audio input devices.
#[tauri::command]
pub async fn list_audio_devices() -> Result<Vec<AudioDevice>, CommandError> {
    // TODO: Implement via sidecar RPC call to audio.list_devices
    Err(CommandError::NotImplemented {
        message: "Audio device listing requires sidecar connection".to_string(),
    })
}

/// Set the audio input device.
#[tauri::command]
pub async fn set_audio_device(device_uid: Option<String>) -> Result<String, CommandError> {
    // Update config
    let mut config = config::load_config();
    config.audio.device_uid = device_uid.clone();
    config::save_config(&config)?;

    // TODO: Notify sidecar of device change
    Ok(device_uid.unwrap_or_else(|| "default".to_string()))
}

/// Start microphone test (for level visualization).
#[tauri::command]
pub async fn start_mic_test() -> Result<(), CommandError> {
    // TODO: Implement via sidecar
    Err(CommandError::NotImplemented {
        message: "Microphone test requires sidecar connection".to_string(),
    })
}

/// Stop microphone test.
#[tauri::command]
pub async fn stop_mic_test() -> Result<(), CommandError> {
    // TODO: Implement via sidecar
    Err(CommandError::NotImplemented {
        message: "Microphone test requires sidecar connection".to_string(),
    })
}

// ============================================================================
// MODEL COMMANDS
// ============================================================================

/// Model status information.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub model_id: String,
    pub status: ModelState,
    pub progress: Option<Progress>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    Missing,
    Downloading,
    Verifying,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub current: u64,
    pub total: Option<u64>,
    pub unit: String,
}

/// Get model status.
#[tauri::command]
pub async fn get_model_status() -> ModelStatus {
    // TODO: Query sidecar for model status
    ModelStatus {
        model_id: "nvidia/parakeet-tdt-0.6b-v2".to_string(),
        status: ModelState::Missing, // Placeholder
        progress: None,
        error: None,
    }
}

/// Download the ASR model.
#[tauri::command]
pub async fn download_model() -> Result<(), CommandError> {
    // TODO: Implement via sidecar
    Err(CommandError::NotImplemented {
        message: "Model download requires sidecar connection".to_string(),
    })
}

/// Purge model cache.
#[tauri::command]
pub async fn purge_model_cache() -> Result<(), CommandError> {
    // TODO: Implement
    Err(CommandError::NotImplemented {
        message: "Model cache purge not yet implemented".to_string(),
    })
}

// ============================================================================
// HISTORY COMMANDS
// ============================================================================

/// Get transcript history (newest first).
#[tauri::command]
pub fn get_transcript_history(history: tauri::State<TranscriptHistory>) -> Vec<TranscriptEntry> {
    history.all()
}

/// Copy a specific transcript to clipboard by ID.
#[tauri::command]
pub fn copy_transcript(
    history: tauri::State<TranscriptHistory>,
    id: String,
) -> Result<(), CommandError> {
    let uuid = Uuid::parse_str(&id).map_err(|_| CommandError::Clipboard {
        message: "Invalid transcript ID".to_string(),
    })?;

    history
        .copy_by_id(uuid)
        .ok_or_else(|| CommandError::Clipboard {
            message: "Transcript not found or clipboard error".to_string(),
        })?;

    Ok(())
}

/// Copy the most recent transcript to clipboard.
#[tauri::command]
pub fn copy_last_transcript(
    history: tauri::State<TranscriptHistory>,
) -> Result<Option<String>, CommandError> {
    if history.is_empty() {
        return Ok(None);
    }

    history
        .copy_last()
        .map(Some)
        .ok_or_else(|| CommandError::Clipboard {
            message: "Clipboard error".to_string(),
        })
}

/// Clear transcript history.
#[tauri::command]
pub fn clear_history(history: tauri::State<TranscriptHistory>) {
    history.clear();
}

// ============================================================================
// HOTKEY COMMANDS
// ============================================================================

/// Hotkey status information.
#[derive(Debug, Clone, Serialize)]
pub struct HotkeyStatus {
    pub primary: String,
    pub copy_last: String,
    pub mode: String,
    pub registered: bool,
}

/// Get current hotkey status.
#[tauri::command]
pub fn get_hotkey_status() -> HotkeyStatus {
    let config = config::load_config();
    HotkeyStatus {
        primary: config.hotkeys.primary,
        copy_last: config.hotkeys.copy_last,
        mode: format!("{:?}", config.hotkeys.mode).to_lowercase(),
        registered: false, // TODO: Track actual registration state
    }
}

/// Set hotkey bindings.
#[tauri::command]
pub fn set_hotkey(primary: String, copy_last: String) -> Result<(), CommandError> {
    let mut config = config::load_config();
    config.hotkeys.primary = primary;
    config.hotkeys.copy_last = copy_last;
    config::save_config(&config)?;

    // TODO: Re-register hotkeys with the system
    Ok(())
}

// ============================================================================
// REPLACEMENT COMMANDS
// ============================================================================

/// Preset information.
#[derive(Debug, Clone, Serialize)]
pub struct PresetInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub rule_count: usize,
}

/// Get current replacement rules.
#[tauri::command]
pub fn get_replacement_rules() -> Vec<ReplacementRule> {
    let config = config::load_config();
    config.replacements
}

/// Set replacement rules.
#[tauri::command]
pub async fn set_replacement_rules(rules: Vec<ReplacementRule>) -> Result<(), CommandError> {
    let mut config = config::load_config();
    config.replacements = rules;
    config::save_config(&config)?;
    Ok(())
}

/// Preview replacement result without saving.
#[tauri::command]
pub fn preview_replacement(input: String, rules: Vec<ReplacementRule>) -> String {
    let mut result = input;
    for rule in rules {
        if rule.enabled {
            result = result.replace(&rule.pattern, &rule.replacement);
        }
    }
    result
}

/// Get available presets.
#[tauri::command]
pub fn get_available_presets() -> Vec<PresetInfo> {
    // TODO: Load presets from bundled files
    vec![
        PresetInfo {
            id: "punctuation".to_string(),
            name: "Punctuation".to_string(),
            description: "Convert spoken punctuation to symbols".to_string(),
            rule_count: 10,
        },
        PresetInfo {
            id: "common-corrections".to_string(),
            name: "Common Corrections".to_string(),
            description: "Fix common transcription errors".to_string(),
            rule_count: 25,
        },
    ]
}

/// Load a preset's rules.
#[tauri::command]
pub fn load_preset(preset_id: String) -> Vec<ReplacementRule> {
    // TODO: Load actual preset rules
    match preset_id.as_str() {
        "punctuation" => vec![
            ReplacementRule {
                pattern: " period".to_string(),
                replacement: ".".to_string(),
                enabled: true,
            },
            ReplacementRule {
                pattern: " comma".to_string(),
                replacement: ",".to_string(),
                enabled: true,
            },
            ReplacementRule {
                pattern: " question mark".to_string(),
                replacement: "?".to_string(),
                enabled: true,
            },
        ],
        _ => vec![],
    }
}

// ============================================================================
// CONTROL COMMANDS
// ============================================================================

/// Toggle enabled state.
#[tauri::command]
pub fn toggle_enabled(state_manager: tauri::State<Arc<AppStateManager>>) -> bool {
    let current = state_manager.is_enabled();
    state_manager.set_enabled(!current);
    !current
}

/// Check if enabled.
#[tauri::command]
pub fn is_enabled(state_manager: tauri::State<Arc<AppStateManager>>) -> bool {
    state_manager.is_enabled()
}

/// Set enabled state.
#[tauri::command]
pub fn set_enabled(state_manager: tauri::State<Arc<AppStateManager>>, enabled: bool) {
    state_manager.set_enabled(enabled);
}

// ============================================================================
// DIAGNOSTICS COMMANDS
// ============================================================================

/// Diagnostics report.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsReport {
    pub version: String,
    pub platform: String,
    pub capabilities: Capabilities,
    pub config: AppConfig,
    pub self_check: SelfCheckResult,
}

// Re-export LogEntry from log_buffer for IPC
pub use crate::log_buffer::LogEntry;

/// Generate diagnostics report.
#[tauri::command]
pub fn generate_diagnostics() -> DiagnosticsReport {
    DiagnosticsReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS.to_string(),
        capabilities: Capabilities::detect(),
        config: config::load_config(),
        self_check: run_self_check(),
    }
}

/// Get recent log entries from the ring buffer.
#[tauri::command]
pub fn get_recent_logs(count: usize) -> Vec<LogEntry> {
    let buffer = crate::log_buffer::global_buffer();
    let entries = buffer.entries();
    let len = entries.len();

    // Return the last `count` entries (or all if count is larger)
    if count >= len {
        entries
    } else {
        entries.into_iter().skip(len - count).collect()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_error_serialization() {
        let error = CommandError::Config {
            message: "Test error".to_string(),
        };
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("config"));
        assert!(json.contains("Test error"));
    }

    #[test]
    fn test_check_status_serialization() {
        let status = CheckStatus::Ok;
        assert_eq!(serde_json::to_string(&status).unwrap(), "\"ok\"");

        let status = CheckStatus::Warning;
        assert_eq!(serde_json::to_string(&status).unwrap(), "\"warning\"");

        let status = CheckStatus::Error;
        assert_eq!(serde_json::to_string(&status).unwrap(), "\"error\"");
    }

    #[test]
    fn test_model_state_serialization() {
        let state = ModelState::Ready;
        assert_eq!(serde_json::to_string(&state).unwrap(), "\"ready\"");
    }

    #[test]
    fn test_preview_replacement() {
        let rules = vec![
            ReplacementRule {
                pattern: " period".to_string(),
                replacement: ".".to_string(),
                enabled: true,
            },
            ReplacementRule {
                pattern: " comma".to_string(),
                replacement: ",".to_string(),
                enabled: true,
            },
            ReplacementRule {
                pattern: " disabled".to_string(),
                replacement: "XXX".to_string(),
                enabled: false,
            },
        ];

        let result = preview_replacement(
            "Hello period how are you comma I am fine".to_string(),
            rules,
        );
        assert_eq!(result, "Hello. how are you, I am fine");
    }

    #[test]
    fn test_diagnostics_report_serialization() {
        let report = generate_diagnostics();
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("version"));
        assert!(json.contains("platform"));
        assert!(json.contains("capabilities"));
    }
}
