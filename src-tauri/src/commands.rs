//! Tauri commands exposed to the UI.
//!
//! This module provides the complete API surface between the React UI
//! and the Rust backend via Tauri commands.

use regex::{NoExpand, RegexBuilder};
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::capabilities::{Capabilities, CapabilityIssue};
use crate::config::{self, AppConfig, ReplacementRule};
use crate::history::{TranscriptEntry, TranscriptHistory};
use crate::integration::{SidecarAudioDevice, SidecarModelStatus, SidecarPresetInfo};
use crate::state::{AppStateManager, CannotRecordReason, StateEvent};
use crate::IntegrationState;

/// Command error types.
#[derive(Debug, Error, Serialize)]
#[serde(rename_all = "snake_case", tag = "code")]
pub enum CommandError {
    #[error("Config error: {message}")]
    Config { message: String },

    #[error("Audio error: {message}")]
    Audio { message: String },

    #[error("Sidecar IPC error: {message}")]
    #[serde(rename = "E_SIDECAR_IPC")]
    SidecarIpc { message: String },

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
pub fn update_config(
    config: AppConfig,
    history: tauri::State<TranscriptHistory>,
) -> Result<(), CommandError> {
    let mut config = config;
    config.validate_and_clamp();
    config::save_config(&config)?;
    history.resize(config.history.max_entries as usize);
    Ok(())
}

/// Reset configuration to defaults.
#[tauri::command]
pub fn reset_config_to_defaults(
    history: tauri::State<TranscriptHistory>,
) -> Result<AppConfig, CommandError> {
    let config = AppConfig::default();
    config::save_config(&config)?;
    history.resize(config.history.max_entries as usize);
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
pub async fn list_audio_devices(
    integration_state: tauri::State<'_, IntegrationState>,
) -> Result<Vec<AudioDevice>, CommandError> {
    let manager = integration_state.0.read().await;
    let devices = manager
        .list_audio_devices()
        .await
        .map_err(|message| CommandError::Audio { message })?;

    Ok(devices
        .into_iter()
        .map(
            |SidecarAudioDevice {
                 uid,
                 name,
                 is_default,
                 default_sample_rate,
                 channels,
             }| AudioDevice {
                uid,
                name,
                is_default,
                sample_rate: default_sample_rate,
                channels,
            },
        )
        .collect())
}

/// Set the audio input device.
#[tauri::command]
pub async fn set_audio_device(
    integration_state: tauri::State<'_, IntegrationState>,
    device_uid: Option<String>,
) -> Result<String, CommandError> {
    let manager = integration_state.0.read().await;
    let active_device_uid = manager
        .set_audio_device(device_uid.clone())
        .await
        .map_err(|message| CommandError::Audio { message })?;

    // Persist configured device selection once sidecar accepts the change.
    let mut app_config = config::load_config();
    app_config.audio.device_uid = device_uid.clone();
    config::save_config(&app_config)?;

    Ok(active_device_uid
        .or(device_uid)
        .unwrap_or_else(|| "default".to_string()))
}

/// Start microphone test (for level visualization).
#[tauri::command]
pub async fn start_mic_test(
    integration_state: tauri::State<'_, IntegrationState>,
) -> Result<(), CommandError> {
    let device_uid = config::load_config().audio.device_uid;
    let manager = integration_state.0.read().await;
    manager
        .start_mic_test(device_uid)
        .await
        .map_err(|message| CommandError::Audio { message })
}

/// Stop microphone test.
#[tauri::command]
pub async fn stop_mic_test(
    integration_state: tauri::State<'_, IntegrationState>,
) -> Result<(), CommandError> {
    let manager = integration_state.0.read().await;
    manager
        .stop_mic_test()
        .await
        .map_err(|message| CommandError::Audio { message })
}

// ============================================================================
// MODEL COMMANDS
// ============================================================================

/// Model status information.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub model_id: String,
    pub status: ModelState,
    pub revision: Option<String>,
    pub cache_path: Option<String>,
    pub progress: Option<Progress>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    Missing,
    Downloading,
    Loading,
    Verifying,
    Ready,
    Error,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub current: u64,
    pub total: Option<u64>,
    pub unit: String,
}

/// Get model status.
#[tauri::command]
pub async fn get_model_status(
    integration_state: tauri::State<'_, IntegrationState>,
    model_id: Option<String>,
) -> Result<ModelStatus, CommandError> {
    let manager = integration_state.0.read().await;
    let status = manager
        .query_model_status(model_id)
        .await
        .map_err(|message| CommandError::SidecarIpc { message })?;

    Ok(map_sidecar_model_status(status))
}

fn map_sidecar_model_status(status: SidecarModelStatus) -> ModelStatus {
    let model_state = match status.status.as_str() {
        "missing" => ModelState::Missing,
        "downloading" => ModelState::Downloading,
        "loading" => ModelState::Loading,
        "verifying" => ModelState::Verifying,
        "ready" => ModelState::Ready,
        "error" => ModelState::Error,
        _ => ModelState::Unknown,
    };

    let progress = status.progress.map(|p| Progress {
        current: p.current,
        total: p.total,
        unit: p.unit.unwrap_or_else(|| "bytes".to_string()),
    });

    ModelStatus {
        model_id: status.model_id,
        status: model_state,
        revision: status.revision,
        cache_path: status.cache_path,
        progress,
        error: status.error.or(status.error_message),
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
            let pattern = if rule.word_boundary {
                if rule.kind == "regex" {
                    format!(r"\b(?:{})\b", rule.pattern)
                } else {
                    format!(r"\b{}\b", regex::escape(&rule.pattern))
                }
            } else if rule.kind == "regex" {
                rule.pattern.clone()
            } else {
                regex::escape(&rule.pattern)
            };

            match RegexBuilder::new(&pattern)
                .case_insensitive(!rule.case_sensitive)
                .build()
            {
                Ok(compiled) => {
                    result = if rule.kind == "regex" {
                        compiled
                            .replace_all(&result, rule.replacement.as_str())
                            .into_owned()
                    } else {
                        compiled
                            .replace_all(&result, NoExpand(rule.replacement.as_str()))
                            .into_owned()
                    };
                }
                Err(error) => {
                    log::warn!(
                        "Skipping replacement rule '{}' due to invalid pattern '{}': {}",
                        rule.id,
                        rule.pattern,
                        error
                    );
                }
            }
        }
    }
    result
}

/// Get available presets.
#[tauri::command]
pub async fn get_available_presets(
    integration_state: tauri::State<'_, IntegrationState>,
) -> Result<Vec<PresetInfo>, CommandError> {
    let manager = integration_state.0.read().await;
    let presets = manager
        .list_replacement_presets()
        .await
        .map_err(|message| CommandError::SidecarIpc { message })?;

    Ok(presets
        .into_iter()
        .map(
            |SidecarPresetInfo {
                 id,
                 name,
                 description,
                 rule_count,
             }| PresetInfo {
                id,
                name,
                description,
                rule_count,
            },
        )
        .collect())
}

fn rule_belongs_to_preset(rule: &ReplacementRule, preset_id: &str) -> bool {
    if rule.id.starts_with(&format!("{preset_id}:")) {
        return true;
    }

    rule.origin
        .as_deref()
        .is_some_and(|origin| origin == format!("preset:{preset_id}"))
}

fn merge_preset_rules(
    active_rules: Vec<ReplacementRule>,
    preset_rules: Vec<ReplacementRule>,
    preset_id: &str,
) -> Vec<ReplacementRule> {
    let mut merged_rules = active_rules
        .into_iter()
        .filter(|rule| !rule_belongs_to_preset(rule, preset_id))
        .collect::<Vec<_>>();

    merged_rules.extend(preset_rules);
    merged_rules
}

/// Load a preset's rules.
#[tauri::command]
pub async fn load_preset(
    integration_state: tauri::State<'_, IntegrationState>,
    preset_id: String,
) -> Result<Vec<ReplacementRule>, CommandError> {
    let manager = integration_state.0.read().await;

    let preset_rules = match manager
        .get_preset_replacement_rules(preset_id.clone())
        .await
        .map_err(|message| CommandError::SidecarIpc { message })?
    {
        Some(rules) => rules,
        None => {
            // Missing preset should be a no-op for UI consumers (no thrown error).
            return Ok(Vec::new());
        }
    };

    let active_rules = manager
        .get_active_replacement_rules()
        .await
        .map_err(|message| CommandError::SidecarIpc { message })?;

    let merged_rules = merge_preset_rules(active_rules, preset_rules.clone(), &preset_id);

    manager
        .set_active_replacement_rules(merged_rules.clone())
        .await
        .map_err(|message| CommandError::SidecarIpc { message })?;

    let mut app_config = config::load_config();
    app_config.replacements = merged_rules;
    if !app_config
        .presets
        .enabled_presets
        .iter()
        .any(|enabled| enabled == &preset_id)
    {
        app_config.presets.enabled_presets.push(preset_id);
    }
    config::save_config(&app_config)?;

    Ok(preset_rules)
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
    pub recent_logs: Vec<LogEntry>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub environment: BTreeMap<String, String>,
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
        recent_logs: get_recent_logs(100),
        environment: diagnostics_environment(),
    }
}

fn diagnostics_environment() -> BTreeMap<String, String> {
    diagnostics_environment_from_iter(std::env::vars())
}

fn diagnostics_environment_from_iter<I>(vars: I) -> BTreeMap<String, String>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut env = BTreeMap::new();
    for (key, value) in vars {
        if should_include_diagnostics_env_var(&key) {
            env.insert(key.clone(), redact_diagnostics_env_value(&key, &value));
        }
    }
    env
}

fn should_include_diagnostics_env_var(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    upper.starts_with("OPENVOICY_")
        || upper.starts_with("HF_")
        || upper.starts_with("TRANSLATORVOICEINPUTTOOL_")
        || is_sensitive_env_key(&upper)
}

fn redact_diagnostics_env_value(key: &str, value: &str) -> String {
    if is_sensitive_env_key(&key.to_ascii_uppercase()) {
        "[REDACTED]".to_string()
    } else {
        value.to_string()
    }
}

fn is_sensitive_env_key(upper_key: &str) -> bool {
    upper_key.contains("TOKEN")
        || upper_key.contains("SECRET")
        || upper_key.contains("PASSWORD")
        || upper_key.contains("API_KEY")
        || upper_key.ends_with("_KEY")
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
                id: "rule-1".to_string(),
                kind: "literal".to_string(),
                pattern: " period".to_string(),
                replacement: ".".to_string(),
                enabled: true,
                word_boundary: false,
                case_sensitive: false,
                description: None,
                origin: None,
            },
            ReplacementRule {
                id: "rule-2".to_string(),
                kind: "literal".to_string(),
                pattern: " comma".to_string(),
                replacement: ",".to_string(),
                enabled: true,
                word_boundary: false,
                case_sensitive: false,
                description: None,
                origin: None,
            },
            ReplacementRule {
                id: "rule-3".to_string(),
                kind: "literal".to_string(),
                pattern: " disabled".to_string(),
                replacement: "XXX".to_string(),
                enabled: false,
                word_boundary: false,
                case_sensitive: false,
                description: None,
                origin: None,
            },
        ];

        let result = preview_replacement(
            "Hello period how are you comma I am fine".to_string(),
            rules,
        );
        assert_eq!(result, "Hello. how are you, I am fine");
    }

    #[test]
    fn test_preview_replacement_word_boundary_and_case_sensitivity() {
        let rules = vec![ReplacementRule {
            id: "rule-word-boundary".to_string(),
            kind: "literal".to_string(),
            pattern: "asap".to_string(),
            replacement: "as soon as possible".to_string(),
            enabled: true,
            word_boundary: true,
            case_sensitive: false,
            description: None,
            origin: None,
        }];

        let result = preview_replacement("ASAPly ASAP".to_string(), rules);
        assert_eq!(result, "ASAPly as soon as possible");
    }

    #[test]
    fn test_preview_replacement_regex_kind() {
        let rules = vec![ReplacementRule {
            id: "rule-regex".to_string(),
            kind: "regex".to_string(),
            pattern: "\\$\\d+(\\.\\d{2})?".to_string(),
            replacement: "[PRICE]".to_string(),
            enabled: true,
            word_boundary: false,
            case_sensitive: true,
            description: None,
            origin: None,
        }];

        let result = preview_replacement("Total: $42.50".to_string(), rules);
        assert_eq!(result, "Total: [PRICE]");
    }

    #[test]
    fn test_merge_preset_rules_replaces_existing_rules_for_same_preset() {
        let active_rules = vec![
            ReplacementRule {
                id: "user-rule-1".to_string(),
                kind: "literal".to_string(),
                pattern: "btw".to_string(),
                replacement: "by the way".to_string(),
                enabled: true,
                word_boundary: true,
                case_sensitive: false,
                description: None,
                origin: Some("user".to_string()),
            },
            ReplacementRule {
                id: "punctuation:period".to_string(),
                kind: "literal".to_string(),
                pattern: " period".to_string(),
                replacement: ".".to_string(),
                enabled: true,
                word_boundary: false,
                case_sensitive: false,
                description: None,
                origin: Some("preset".to_string()),
            },
        ];

        let preset_rules = vec![ReplacementRule {
            id: "punctuation:comma".to_string(),
            kind: "literal".to_string(),
            pattern: " comma".to_string(),
            replacement: ",".to_string(),
            enabled: true,
            word_boundary: false,
            case_sensitive: false,
            description: None,
            origin: Some("preset".to_string()),
        }];

        let merged = merge_preset_rules(active_rules, preset_rules, "punctuation");
        let merged_ids = merged.into_iter().map(|rule| rule.id).collect::<Vec<_>>();

        assert_eq!(merged_ids, vec!["user-rule-1", "punctuation:comma"]);
    }

    #[test]
    fn test_merge_preset_rules_keeps_other_preset_rules() {
        let active_rules = vec![
            ReplacementRule {
                id: "coding-terms:semicolon".to_string(),
                kind: "literal".to_string(),
                pattern: " semicolon".to_string(),
                replacement: ";".to_string(),
                enabled: true,
                word_boundary: false,
                case_sensitive: false,
                description: None,
                origin: Some("preset".to_string()),
            },
            ReplacementRule {
                id: "punctuation:period".to_string(),
                kind: "literal".to_string(),
                pattern: " period".to_string(),
                replacement: ".".to_string(),
                enabled: true,
                word_boundary: false,
                case_sensitive: false,
                description: None,
                origin: Some("preset".to_string()),
            },
        ];

        let preset_rules = vec![ReplacementRule {
            id: "punctuation:question-mark".to_string(),
            kind: "literal".to_string(),
            pattern: " question mark".to_string(),
            replacement: "?".to_string(),
            enabled: true,
            word_boundary: false,
            case_sensitive: false,
            description: None,
            origin: Some("preset".to_string()),
        }];

        let merged = merge_preset_rules(active_rules, preset_rules, "punctuation");
        let merged_ids = merged.into_iter().map(|rule| rule.id).collect::<Vec<_>>();

        assert_eq!(
            merged_ids,
            vec!["coding-terms:semicolon", "punctuation:question-mark"]
        );
    }

    #[test]
    fn test_diagnostics_report_serialization() {
        let report = generate_diagnostics();
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("version"));
        assert!(json.contains("platform"));
        assert!(json.contains("capabilities"));
        assert!(json.contains("recent_logs"));
    }

    #[test]
    fn test_generate_diagnostics_includes_recent_logs() {
        let buffer = crate::log_buffer::global_buffer();
        buffer.clear();
        crate::log_buffer::log_to_buffer(log::Level::Info, "commands::tests", "diagnostics-log");

        let report = generate_diagnostics();
        assert!(report
            .recent_logs
            .iter()
            .any(|entry| entry.target == "commands::tests"
                && entry.message.contains("diagnostics-log")));
    }

    #[test]
    fn test_diagnostics_environment_redacts_sensitive_values() {
        let env = diagnostics_environment_from_iter(vec![
            ("HF_TOKEN".to_string(), "hf_secret_token".to_string()),
            ("SERVICE_API_KEY".to_string(), "api_secret".to_string()),
            ("OPENVOICY_LOG_LEVEL".to_string(), "debug".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
        ]);

        assert_eq!(env.get("HF_TOKEN").map(String::as_str), Some("[REDACTED]"));
        assert_eq!(
            env.get("SERVICE_API_KEY").map(String::as_str),
            Some("[REDACTED]")
        );
        assert_eq!(
            env.get("OPENVOICY_LOG_LEVEL").map(String::as_str),
            Some("debug")
        );
        assert!(!env.contains_key("PATH"));
    }
}
