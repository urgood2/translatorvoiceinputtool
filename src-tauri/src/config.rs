//! Configuration persistence with atomic writes and migrations.
//!
//! Stores application configuration in a JSON file with:
//! - Atomic writes (write temp, rename)
//! - Corruption fallback (regenerate defaults if parse fails)
//! - Schema versioning with migration support
//! - Platform-specific config paths

#![allow(dead_code)] // Module under construction

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use uuid::Uuid;

/// Current schema version.
const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Default config directory name.
const CONFIG_DIR_NAME: &str = "OpenVoicy";

/// Config file name.
const CONFIG_FILE_NAME: &str = "config.json";
const SENSITIVE_FIELD_KEYWORDS: [&str; 4] = ["token", "key", "secret", "password"];

const ROOT_CONFIG_FIELDS: [&str; 9] = [
    "schema_version",
    "audio",
    "hotkeys",
    "injection",
    "model",
    "replacements",
    "ui",
    "history",
    "presets",
];

const AUDIO_CONFIG_FIELDS: [&str; 6] = [
    "device_uid",
    "audio_cues_enabled",
    "trim_silence",
    "vad_enabled",
    "vad_silence_ms",
    "vad_min_speech_ms",
];

const HOTKEY_CONFIG_FIELDS: [&str; 3] = ["primary", "copy_last", "mode"];

const INJECTION_CONFIG_FIELDS: [&str; 5] = [
    "paste_delay_ms",
    "restore_clipboard",
    "suffix",
    "focus_guard_enabled",
    "app_overrides",
];

const APP_OVERRIDE_FIELDS: [&str; 2] = ["paste_delay_ms", "use_clipboard_only"];

const MODEL_CONFIG_FIELDS: [&str; 4] = ["model_id", "device", "preferred_device", "language"];

const REPLACEMENT_RULE_FIELDS: [&str; 9] = [
    "id",
    "kind",
    "pattern",
    "replacement",
    "enabled",
    "word_boundary",
    "case_sensitive",
    "description",
    "origin",
];

const UI_CONFIG_FIELDS: [&str; 8] = [
    "show_on_startup",
    "window_width",
    "window_height",
    "theme",
    "onboarding_completed",
    "overlay_enabled",
    "locale",
    "reduce_motion",
];

const HISTORY_CONFIG_FIELDS: [&str; 3] = ["persistence_mode", "max_entries", "encrypt_at_rest"];

const PRESETS_CONFIG_FIELDS: [&str; 1] = ["enabled_presets"];

/// Root application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Schema version for migrations.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,

    /// Audio settings.
    #[serde(default)]
    pub audio: AudioConfig,

    /// Hotkey bindings.
    #[serde(default)]
    pub hotkeys: HotkeyConfig,

    /// Text injection settings.
    #[serde(default)]
    pub injection: InjectionConfig,

    /// Model settings.
    #[serde(default)]
    pub model: Option<ModelConfig>,

    /// Text replacement rules.
    #[serde(default)]
    pub replacements: Vec<ReplacementRule>,

    /// UI settings.
    #[serde(default)]
    pub ui: UiConfig,

    /// History settings.
    #[serde(default)]
    pub history: HistoryConfig,

    /// Preset configurations.
    #[serde(default)]
    pub presets: PresetsConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            audio: AudioConfig::default(),
            hotkeys: HotkeyConfig::default(),
            injection: InjectionConfig::default(),
            model: None, // Use defaults from sidecar
            replacements: Vec::new(),
            ui: UiConfig::default(),
            history: HistoryConfig::default(),
            presets: PresetsConfig::default(),
        }
    }
}

/// Model configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model ID (e.g., "nvidia/parakeet-tdt-0.6b-v2").
    #[serde(default)]
    pub model_id: Option<String>,
    /// Device preference ("auto", "cpu", "cuda", "mps").
    #[serde(default)]
    pub device: Option<String>,
    /// Preferred compute tier from UI ("auto", "cpu", "gpu").
    #[serde(default = "default_preferred_device")]
    pub preferred_device: String,
    /// ASR language hint: None for sidecar default, "auto" for autodetect, or ISO 639-1 code.
    #[serde(default)]
    pub language: Option<String>,
}

impl ModelConfig {
    /// Resolve effective sidecar device preference from legacy and new fields.
    pub fn effective_device_pref(&self) -> String {
        if let Some(device) = self.device.as_deref() {
            if matches!(device, "cuda" | "mps") {
                return device.to_string();
            }
        }

        map_preferred_device_to_backend(&self.preferred_device)
    }
}

impl AppConfig {
    /// Resolve effective sidecar device preference for model initialization.
    pub fn effective_model_device_pref(&self) -> String {
        self.model
            .as_ref()
            .map(ModelConfig::effective_device_pref)
            .unwrap_or_else(|| "auto".to_string())
    }

    /// Validate and clamp config values to valid ranges.
    pub fn validate_and_clamp(&mut self) {
        let original_paste_delay_ms = self.injection.paste_delay_ms;
        self.injection.paste_delay_ms = self.injection.paste_delay_ms.clamp(10, 500);
        if self.injection.paste_delay_ms != original_paste_delay_ms {
            log::warn!(
                "injection.paste_delay_ms clamped from {} to {}",
                original_paste_delay_ms,
                self.injection.paste_delay_ms
            );
        }

        let invalid_override_keys: Vec<String> = self
            .injection
            .app_overrides
            .keys()
            .filter(|app_id| app_id.trim().is_empty())
            .cloned()
            .collect();
        for invalid_key in invalid_override_keys {
            self.injection.app_overrides.remove(&invalid_key);
            log::warn!("Removed invalid injection.app_overrides entry with empty app id");
        }

        for (app_id, override_config) in &mut self.injection.app_overrides {
            if let Some(delay) = override_config.paste_delay_ms {
                let clamped = delay.clamp(10, 500);
                if clamped != delay {
                    log::warn!(
                        "injection.app_overrides['{}'].paste_delay_ms clamped from {} to {}",
                        app_id,
                        delay,
                        clamped
                    );
                    override_config.paste_delay_ms = Some(clamped);
                }
            }
        }

        let original_vad_silence_ms = self.audio.vad_silence_ms;
        self.audio.vad_silence_ms = self.audio.vad_silence_ms.clamp(400, 5000);
        if self.audio.vad_silence_ms != original_vad_silence_ms {
            log::warn!(
                "audio.vad_silence_ms clamped from {} to {}",
                original_vad_silence_ms,
                self.audio.vad_silence_ms
            );
        }

        let original_vad_min_speech_ms = self.audio.vad_min_speech_ms;
        self.audio.vad_min_speech_ms = self.audio.vad_min_speech_ms.clamp(100, 2000);
        if self.audio.vad_min_speech_ms != original_vad_min_speech_ms {
            log::warn!(
                "audio.vad_min_speech_ms clamped from {} to {}",
                original_vad_min_speech_ms,
                self.audio.vad_min_speech_ms
            );
        }

        // Validate hotkey format (basic check - ensure non-empty)
        if self.hotkeys.primary.is_empty() {
            log::warn!(
                "hotkeys.primary is empty; resetting to '{}'",
                HotkeyConfig::default().primary
            );
            self.hotkeys.primary = HotkeyConfig::default().primary;
        }
        if self.hotkeys.copy_last.is_empty() {
            log::warn!(
                "hotkeys.copy_last is empty; resetting to '{}'",
                HotkeyConfig::default().copy_last
            );
            self.hotkeys.copy_last = HotkeyConfig::default().copy_last;
        }

        // Validate window dimensions (minimum 200x200)
        let original_window_width = self.ui.window_width;
        let original_window_height = self.ui.window_height;
        self.ui.window_width = self.ui.window_width.max(200);
        self.ui.window_height = self.ui.window_height.max(200);
        if self.ui.window_width != original_window_width {
            log::warn!(
                "ui.window_width clamped from {} to {}",
                original_window_width,
                self.ui.window_width
            );
        }
        if self.ui.window_height != original_window_height {
            log::warn!(
                "ui.window_height clamped from {} to {}",
                original_window_height,
                self.ui.window_height
            );
        }

        if let Some(model) = self.model.as_mut() {
            if !matches!(model.preferred_device.as_str(), "auto" | "cpu" | "gpu") {
                log::warn!(
                    "Invalid model.preferred_device value '{}', resetting to '{}'",
                    model.preferred_device,
                    default_preferred_device()
                );
                model.preferred_device = default_preferred_device();
            }

            // Validate model.language format
            if let Some(language) = model.language.as_deref() {
                if language != "auto" && !is_iso_639_1_code(language) {
                    log::warn!(
                        "Invalid model.language value '{}'; resetting to null",
                        language
                    );
                    model.language = None;
                }
            }
        }

        let effective_model_device_pref = self.effective_model_device_pref();
        log::info!(
            "Effective model device preference resolved to '{}'",
            effective_model_device_pref
        );

        for rule in &mut self.replacements {
            if !matches!(rule.kind.as_str(), "literal" | "regex") {
                log::warn!(
                    "replacement rule '{}' has invalid kind '{}'; resetting to '{}'",
                    rule.id,
                    rule.kind,
                    default_replacement_kind()
                );
                rule.kind = default_replacement_kind();
            }

            if let Some(origin) = rule.origin.as_deref() {
                if !is_valid_replacement_origin(origin) {
                    log::warn!(
                        "replacement rule '{}' has invalid origin '{}'; clearing origin",
                        rule.id,
                        origin
                    );
                    rule.origin = None;
                }
            }
        }

        // Validate theme selection
        if !matches!(self.ui.theme.as_str(), "system" | "light" | "dark") {
            log::warn!(
                "Invalid ui.theme value '{}', resetting to '{}'",
                self.ui.theme,
                default_theme()
            );
            self.ui.theme = default_theme();
        }

        // Validate history persistence mode
        if !matches!(self.history.persistence_mode.as_str(), "memory" | "disk") {
            log::warn!(
                "Invalid history.persistence_mode value '{}', resetting to '{}'",
                self.history.persistence_mode,
                default_persistence_mode()
            );
            self.history.persistence_mode = default_persistence_mode();
        }

        let original_history_max_entries = self.history.max_entries;
        self.history.max_entries = self.history.max_entries.clamp(10, 2000);
        if self.history.max_entries != original_history_max_entries {
            log::warn!(
                "history.max_entries clamped from {} to {}",
                original_history_max_entries,
                self.history.max_entries
            );
        }

        if self.history.persistence_mode == "disk" && !self.history.encrypt_at_rest {
            log::warn!(
                "history.encrypt_at_rest is disabled while persistence_mode is 'disk'; leaving explicit user setting"
            );
        }
    }
}

/// Audio configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    /// Stable device UID (not index!). None = system default.
    pub device_uid: Option<String>,
    /// Whether to play audio cues.
    pub audio_cues_enabled: bool,
    /// Whether to trim leading/trailing silence before ASR.
    #[serde(default = "default_true")]
    pub trim_silence: bool,
    /// Whether Voice Activity Detection auto-stop is enabled.
    #[serde(default)]
    pub vad_enabled: bool,
    /// Silence duration threshold before VAD auto-stop triggers.
    #[serde(default = "default_vad_silence_ms")]
    pub vad_silence_ms: u32,
    /// Minimum speech duration before VAD can auto-stop.
    #[serde(default = "default_vad_min_speech_ms")]
    pub vad_min_speech_ms: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device_uid: None, // Use system default
            audio_cues_enabled: true,
            trim_silence: true,
            vad_enabled: false,
            vad_silence_ms: default_vad_silence_ms(),
            vad_min_speech_ms: default_vad_min_speech_ms(),
        }
    }
}

/// Hotkey mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyMode {
    /// Press and hold to record, release to stop.
    Hold,
    /// Press to toggle recording on/off.
    Toggle,
}

impl Default for HotkeyMode {
    fn default() -> Self {
        Self::Hold
    }
}

/// Hotkey configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    /// Primary hotkey for recording.
    pub primary: String,
    /// Hotkey to copy last transcript.
    pub copy_last: String,
    /// Hotkey mode (hold vs toggle).
    pub mode: HotkeyMode,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            primary: "Ctrl+Shift+Space".to_string(),
            copy_last: "Ctrl+Shift+V".to_string(),
            mode: HotkeyMode::Hold,
        }
    }
}

/// Text injection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InjectionConfig {
    /// Delay before paste shortcut (ms). Clamped to 10-500.
    pub paste_delay_ms: u32,
    /// Whether to restore previous clipboard after injection.
    pub restore_clipboard: bool,
    /// Suffix to append to injected text.
    pub suffix: String,
    /// Whether Focus Guard is enabled.
    pub focus_guard_enabled: bool,
    /// Per-application overrides keyed by app identifier.
    #[serde(default)]
    pub app_overrides: HashMap<String, AppOverride>,
}

/// Per-application injection override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppOverride {
    /// Optional paste delay override for this app.
    #[serde(default)]
    pub paste_delay_ms: Option<u32>,
    /// Whether clipboard-only injection should be used for this app.
    #[serde(default)]
    pub use_clipboard_only: Option<bool>,
}

impl Default for InjectionConfig {
    fn default() -> Self {
        Self {
            paste_delay_ms: 40,
            restore_clipboard: true,
            suffix: " ".to_string(), // Single space
            focus_guard_enabled: true,
            app_overrides: HashMap::new(),
        }
    }
}

/// Text replacement rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplacementRule {
    /// Stable rule identifier (UUID for user-created rules).
    #[serde(default = "generate_replacement_rule_id")]
    pub id: String,
    /// Rule type: literal text or regex pattern.
    #[serde(default = "default_replacement_kind")]
    pub kind: String,
    /// Pattern to match.
    pub pattern: String,
    /// Replacement text.
    pub replacement: String,
    /// Whether this rule is enabled.
    pub enabled: bool,
    /// Whether matching is restricted to word boundaries.
    #[serde(default)]
    pub word_boundary: bool,
    /// Whether matching is case-sensitive.
    #[serde(default = "default_true")]
    pub case_sensitive: bool,
    /// Optional human-readable rule description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional origin identifier (e.g., "user" or "preset:punctuation").
    #[serde(default)]
    pub origin: Option<String>,
}

/// UI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Show settings window on startup.
    pub show_on_startup: bool,
    /// Window width.
    pub window_width: u32,
    /// Window height.
    pub window_height: u32,
    /// Theme preference ("system", "light", "dark").
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Whether onboarding has been completed.
    #[serde(default = "default_onboarding_completed")]
    pub onboarding_completed: bool,
    /// Whether overlay UI is enabled.
    #[serde(default = "default_overlay_enabled")]
    pub overlay_enabled: bool,
    /// Preferred UI locale (e.g., "en-US"), or None for system locale.
    #[serde(default)]
    pub locale: Option<String>,
    /// Whether reduced-motion mode is enabled.
    #[serde(default)]
    pub reduce_motion: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_on_startup: true, // First run shows settings
            window_width: 600,
            window_height: 500,
            theme: default_theme(),
            onboarding_completed: default_onboarding_completed(),
            overlay_enabled: default_overlay_enabled(),
            locale: None,
            reduce_motion: false,
        }
    }
}

/// History configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    /// Transcript history persistence mode: "memory" or "disk".
    #[serde(default = "default_persistence_mode")]
    pub persistence_mode: String,
    /// Maximum number of history entries retained in memory.
    #[serde(default = "default_history_max_entries")]
    pub max_entries: u32,
    /// Whether disk-persisted history is encrypted at rest.
    #[serde(default = "default_true")]
    pub encrypt_at_rest: bool,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            persistence_mode: default_persistence_mode(),
            max_entries: default_history_max_entries(),
            encrypt_at_rest: default_true(),
        }
    }
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_onboarding_completed() -> bool {
    false
}

fn default_overlay_enabled() -> bool {
    true
}

fn default_persistence_mode() -> String {
    "memory".to_string()
}

fn default_history_max_entries() -> u32 {
    100
}

fn default_true() -> bool {
    true
}

fn default_vad_silence_ms() -> u32 {
    1200
}

fn default_vad_min_speech_ms() -> u32 {
    250
}

fn default_preferred_device() -> String {
    "auto".to_string()
}

fn preferred_gpu_backend() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "mps"
    }

    #[cfg(not(target_os = "macos"))]
    {
        "cuda"
    }
}

fn map_preferred_device_to_backend(preferred_device: &str) -> String {
    match preferred_device {
        "cpu" => "cpu".to_string(),
        "gpu" => preferred_gpu_backend().to_string(),
        _ => "auto".to_string(),
    }
}

fn is_iso_639_1_code(language: &str) -> bool {
    language.len() == 2 && language.chars().all(|ch| ch.is_ascii_alphabetic())
}

fn is_valid_replacement_origin(origin: &str) -> bool {
    origin == "user"
        || origin == "preset"
        || origin
            .strip_prefix("preset:")
            .is_some_and(|preset_name| !preset_name.trim().is_empty())
}

fn generate_replacement_rule_id() -> String {
    Uuid::new_v4().to_string()
}

fn default_replacement_kind() -> String {
    "literal".to_string()
}

/// Preset configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetsConfig {
    /// IDs of enabled preset rule sets.
    pub enabled_presets: Vec<String>,
}

impl Default for PresetsConfig {
    fn default() -> Self {
        Self {
            enabled_presets: Vec::new(),
        }
    }
}

fn default_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

/// Get the platform-specific config directory path.
pub fn config_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(CONFIG_DIR_NAME)
    }

    #[cfg(target_os = "windows")]
    {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(CONFIG_DIR_NAME)
    }

    #[cfg(target_os = "linux")]
    {
        dirs::config_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".config")
            })
            .join(CONFIG_DIR_NAME)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        PathBuf::from(".").join(CONFIG_DIR_NAME)
    }
}

/// Get the full config file path.
pub fn config_path() -> PathBuf {
    config_dir().join(CONFIG_FILE_NAME)
}

/// Load configuration from disk.
///
/// If the config file doesn't exist or is corrupted, returns defaults.
/// Corrupted files are backed up for debugging.
pub fn load_config() -> AppConfig {
    load_config_from_path(&config_path())
}

/// Load configuration from a specific path (for testing).
pub fn load_config_from_path(path: &PathBuf) -> AppConfig {
    match fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<Value>(&content) {
            Ok(value) => {
                warn_on_sensitive_unknown_fields(&value);
                let mut config = migrate_config(value);
                config.validate_and_clamp();
                config
            }
            Err(e) => {
                log::error!("Config parse error, using defaults: {}", e);
                // Backup corrupt file for debugging
                let backup = path.with_extension("json.corrupt");
                if let Err(backup_err) = fs::rename(path, &backup) {
                    log::warn!("Failed to backup corrupt config: {}", backup_err);
                }
                AppConfig::default()
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            log::info!("No config file found, using defaults");
            AppConfig::default()
        }
        Err(e) => {
            log::error!("Config read error, using defaults: {}", e);
            AppConfig::default()
        }
    }
}

/// Save configuration to disk atomically.
///
/// Writes to a temp file first, then renames to the final path.
pub fn save_config(config: &AppConfig) -> Result<(), ConfigError> {
    save_config_to_path(config, &config_path())
}

/// Save configuration to a specific path (for testing).
pub fn save_config_to_path(config: &AppConfig, path: &PathBuf) -> Result<(), ConfigError> {
    let temp = path.with_extension("json.tmp");

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write to temp file
    let json = serde_json::to_string_pretty(config)?;
    fs::write(&temp, &json)?;

    // Atomic rename
    fs::rename(&temp, path)?;

    Ok(())
}

/// Migrate configuration from older schema versions.
fn migrate_config(mut config: Value) -> AppConfig {
    let version = config["schema_version"].as_u64().unwrap_or(0) as u32;

    // Migration v0 → v1: add focus_guard_enabled
    if version < 1 {
        if let Some(injection) = config.get_mut("injection") {
            if injection.get("focus_guard_enabled").is_none() {
                injection["focus_guard_enabled"] = serde_json::json!(true);
            }
        }
        config["schema_version"] = serde_json::json!(1);
        log::info!("Migrated config v0 → v1: added focus_guard_enabled");
    }

    // Existing config migration: missing onboarding_completed means "already onboarded".
    let needs_onboarding_migration = match config.get("ui") {
        Some(Value::Object(ui)) => !ui.contains_key("onboarding_completed"),
        None => true,
        _ => false,
    };
    if needs_onboarding_migration {
        match config.get_mut("ui") {
            Some(Value::Object(ui)) => {
                ui.insert("onboarding_completed".to_string(), serde_json::json!(true));
                log::info!("Existing user detected, skipping onboarding");
            }
            None => {
                config["ui"] = serde_json::json!({ "onboarding_completed": true });
                log::info!("Existing user detected, skipping onboarding");
            }
            Some(_) => {}
        }
    }

    ensure_replacement_rule_ids(&mut config);

    // Future migrations go here:
    // if version < 2 { ... }

    serde_json::from_value(config).unwrap_or_else(|e| {
        log::error!("Config migration failed, using defaults: {}", e);
        AppConfig::default()
    })
}

fn ensure_replacement_rule_ids(config: &mut Value) {
    let mut migrated = 0usize;

    if let Some(Value::Array(rules)) = config.get_mut("replacements") {
        for rule in rules {
            if let Value::Object(rule_obj) = rule {
                let needs_id = !matches!(
                    rule_obj.get("id"),
                    Some(Value::String(id)) if !id.trim().is_empty()
                );

                if needs_id {
                    rule_obj.insert(
                        "id".to_string(),
                        Value::String(generate_replacement_rule_id()),
                    );
                    migrated += 1;
                }
            }
        }
    }

    if migrated > 0 {
        log::info!(
            "Migrated {} replacement rules to include generated IDs",
            migrated
        );
    }
}

fn warn_on_sensitive_unknown_fields(config: &Value) {
    for path in sensitive_unknown_config_fields(config) {
        log::warn!(
            "Ignoring unknown sensitive config field '{}'; tokens and secrets must not be stored in config",
            path
        );
    }
}

fn sensitive_unknown_config_fields(config: &Value) -> Vec<String> {
    let mut fields = Vec::new();

    let Some(root) = config.as_object() else {
        return fields;
    };

    collect_sensitive_unknown_keys(root, "", &ROOT_CONFIG_FIELDS, &mut fields);

    if let Some(audio) = root.get("audio").and_then(Value::as_object) {
        collect_sensitive_unknown_keys(audio, "audio", &AUDIO_CONFIG_FIELDS, &mut fields);
    }
    if let Some(hotkeys) = root.get("hotkeys").and_then(Value::as_object) {
        collect_sensitive_unknown_keys(hotkeys, "hotkeys", &HOTKEY_CONFIG_FIELDS, &mut fields);
    }
    if let Some(injection) = root.get("injection").and_then(Value::as_object) {
        collect_sensitive_unknown_keys(
            injection,
            "injection",
            &INJECTION_CONFIG_FIELDS,
            &mut fields,
        );
        if let Some(app_overrides) = injection.get("app_overrides").and_then(Value::as_object) {
            for (app_id, override_cfg) in app_overrides {
                if let Some(override_obj) = override_cfg.as_object() {
                    let path_prefix = format!("injection.app_overrides.{}", app_id);
                    collect_sensitive_unknown_keys(
                        override_obj,
                        &path_prefix,
                        &APP_OVERRIDE_FIELDS,
                        &mut fields,
                    );
                }
            }
        }
    }
    if let Some(model) = root.get("model").and_then(Value::as_object) {
        collect_sensitive_unknown_keys(model, "model", &MODEL_CONFIG_FIELDS, &mut fields);
    }
    if let Some(replacements) = root.get("replacements").and_then(Value::as_array) {
        for (idx, replacement) in replacements.iter().enumerate() {
            if let Some(rule_obj) = replacement.as_object() {
                let path_prefix = format!("replacements[{}]", idx);
                collect_sensitive_unknown_keys(
                    rule_obj,
                    &path_prefix,
                    &REPLACEMENT_RULE_FIELDS,
                    &mut fields,
                );
            }
        }
    }
    if let Some(ui) = root.get("ui").and_then(Value::as_object) {
        collect_sensitive_unknown_keys(ui, "ui", &UI_CONFIG_FIELDS, &mut fields);
    }
    if let Some(history) = root.get("history").and_then(Value::as_object) {
        collect_sensitive_unknown_keys(history, "history", &HISTORY_CONFIG_FIELDS, &mut fields);
    }
    if let Some(presets) = root.get("presets").and_then(Value::as_object) {
        collect_sensitive_unknown_keys(presets, "presets", &PRESETS_CONFIG_FIELDS, &mut fields);
    }

    fields.sort();
    fields.dedup();
    fields
}

fn collect_sensitive_unknown_keys(
    object: &serde_json::Map<String, Value>,
    prefix: &str,
    known_keys: &[&str],
    fields: &mut Vec<String>,
) {
    for key in object.keys() {
        let is_known = known_keys.contains(&key.as_str());
        if !is_known && contains_sensitive_field_keyword(key) {
            fields.push(path_with_key(prefix, key));
        }
    }
}

fn contains_sensitive_field_keyword(key: &str) -> bool {
    let key_lower = key.to_ascii_lowercase();
    SENSITIVE_FIELD_KEYWORDS
        .iter()
        .any(|keyword| key_lower.contains(keyword))
}

fn path_with_key(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{}.{}", prefix, key)
    }
}

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(config.audio.device_uid.is_none());
        assert!(config.audio.audio_cues_enabled);
        assert!(config.audio.trim_silence);
        assert!(!config.audio.vad_enabled);
        assert_eq!(config.audio.vad_silence_ms, 1200);
        assert_eq!(config.audio.vad_min_speech_ms, 250);
        assert_eq!(config.hotkeys.primary, "Ctrl+Shift+Space");
        assert_eq!(config.hotkeys.mode, HotkeyMode::Hold);
        assert_eq!(config.injection.paste_delay_ms, 40);
        assert!(config.injection.restore_clipboard);
        assert_eq!(config.injection.suffix, " ");
        assert!(config.injection.focus_guard_enabled);
        assert!(config.injection.app_overrides.is_empty());
        assert!(config.model.is_none());
        assert!(config.replacements.is_empty());
        assert!(config.ui.show_on_startup);
        assert_eq!(config.ui.theme, "system");
        assert!(!config.ui.onboarding_completed);
        assert!(config.ui.overlay_enabled);
        assert_eq!(config.ui.locale, None);
        assert!(!config.ui.reduce_motion);
        assert_eq!(config.history.persistence_mode, "memory");
        assert_eq!(config.history.max_entries, 100);
        assert!(config.history.encrypt_at_rest);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let mut config = AppConfig::default();
        config.audio.device_uid = Some("test-device-uid".to_string());
        config.audio.trim_silence = false;
        config.audio.vad_enabled = true;
        config.audio.vad_silence_ms = 1500;
        config.audio.vad_min_speech_ms = 300;
        config.hotkeys.primary = "Ctrl+Space".to_string();
        config.injection.paste_delay_ms = 100;
        config.injection.app_overrides.insert(
            "slack".to_string(),
            AppOverride {
                paste_delay_ms: Some(120),
                use_clipboard_only: Some(true),
            },
        );
        config.replacements = vec![ReplacementRule {
            id: "f06abed8-b4b4-4fc2-ac96-c7a418084f39".to_string(),
            kind: "literal".to_string(),
            pattern: "BTW".to_string(),
            replacement: "by the way".to_string(),
            enabled: true,
            word_boundary: true,
            case_sensitive: false,
            description: Some("Expand common abbreviation".to_string()),
            origin: Some("user".to_string()),
        }];
        config.model = Some(ModelConfig {
            model_id: Some("nvidia/parakeet-tdt-0.6b-v2".to_string()),
            device: Some("cuda".to_string()),
            preferred_device: "gpu".to_string(),
            language: Some("auto".to_string()),
        });
        config.ui.locale = Some("en-US".to_string());
        config.ui.reduce_motion = true;
        config.history.persistence_mode = "disk".to_string();
        config.history.max_entries = 300;
        config.history.encrypt_at_rest = false;

        // Save
        save_config_to_path(&config, &config_path).unwrap();

        // Verify file exists
        assert!(config_path.exists());

        // Load
        let loaded = load_config_from_path(&config_path);
        assert_eq!(loaded.audio.device_uid, Some("test-device-uid".to_string()));
        assert!(!loaded.audio.trim_silence);
        assert!(loaded.audio.vad_enabled);
        assert_eq!(loaded.audio.vad_silence_ms, 1500);
        assert_eq!(loaded.audio.vad_min_speech_ms, 300);
        assert_eq!(loaded.hotkeys.primary, "Ctrl+Space");
        assert_eq!(loaded.injection.paste_delay_ms, 100);
        assert_eq!(
            loaded
                .injection
                .app_overrides
                .get("slack")
                .and_then(|ov| ov.paste_delay_ms),
            Some(120)
        );
        assert_eq!(
            loaded
                .injection
                .app_overrides
                .get("slack")
                .and_then(|ov| ov.use_clipboard_only),
            Some(true)
        );
        assert_eq!(loaded.replacements.len(), 1);
        assert_eq!(
            loaded.replacements[0].id,
            "f06abed8-b4b4-4fc2-ac96-c7a418084f39"
        );
        assert_eq!(loaded.replacements[0].kind, "literal");
        assert_eq!(loaded.replacements[0].pattern, "BTW");
        assert_eq!(loaded.replacements[0].replacement, "by the way");
        assert!(loaded.replacements[0].enabled);
        assert!(loaded.replacements[0].word_boundary);
        assert!(!loaded.replacements[0].case_sensitive);
        assert_eq!(
            loaded.replacements[0].description.as_deref(),
            Some("Expand common abbreviation")
        );
        assert_eq!(loaded.replacements[0].origin.as_deref(), Some("user"));
        assert_eq!(
            loaded
                .model
                .as_ref()
                .and_then(|model| model.language.as_deref()),
            Some("auto")
        );
        assert_eq!(loaded.ui.locale, Some("en-US".to_string()));
        assert!(loaded.ui.reduce_motion);
        assert_eq!(loaded.history.persistence_mode, "disk");
        assert_eq!(loaded.history.max_entries, 300);
        assert!(!loaded.history.encrypt_at_rest);
    }

    #[test]
    fn test_atomic_write_creates_temp() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let config = AppConfig::default();
        save_config_to_path(&config, &config_path).unwrap();

        // Temp file should not exist after successful save
        let temp_path = config_path.with_extension("json.tmp");
        assert!(!temp_path.exists());

        // Final file should exist
        assert!(config_path.exists());
    }

    #[test]
    fn test_corrupt_json_fallback() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Write invalid JSON
        fs::write(&config_path, "{ invalid json }").unwrap();

        // Load should return defaults
        let config = load_config_from_path(&config_path);
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);

        // Corrupt file should be backed up
        let backup_path = config_path.with_extension("json.corrupt");
        assert!(backup_path.exists());
        assert!(!config_path.exists());
    }

    #[test]
    fn test_valid_json_wrong_schema_fallback() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Write valid JSON with wrong types
        fs::write(
            &config_path,
            r#"{"schema_version": "not_a_number", "audio": "wrong"}"#,
        )
        .unwrap();

        // Load should return defaults after migration fails
        let config = load_config_from_path(&config_path);
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_missing_file_returns_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent.json");

        let config = load_config_from_path(&config_path);
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(!config.ui.onboarding_completed);
        assert!(config.ui.overlay_enabled);
        assert_eq!(config.ui.locale, None);
        assert!(!config.ui.reduce_motion);
        assert_eq!(config.history.persistence_mode, "memory");
    }

    #[test]
    fn test_migration_v0_to_v1() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Write v0 config (no schema_version, no focus_guard_enabled)
        fs::write(
            &config_path,
            r#"{
                "audio": {"device_uid": "my-device", "audio_cues_enabled": true},
                "injection": {"paste_delay_ms": 50}
            }"#,
        )
        .unwrap();

        let config = load_config_from_path(&config_path);
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.audio.device_uid, Some("my-device".to_string()));
        assert!(config.audio.trim_silence);
        assert!(!config.audio.vad_enabled);
        assert_eq!(config.audio.vad_silence_ms, 1200);
        assert_eq!(config.audio.vad_min_speech_ms, 250);
        assert!(config.injection.focus_guard_enabled); // Should be added by migration
        assert!(config.ui.onboarding_completed); // Existing users should skip onboarding
        assert_eq!(config.history.persistence_mode, "memory");
        assert_eq!(config.history.max_entries, 100);
        assert!(config.history.encrypt_at_rest);
    }

    #[test]
    fn test_legacy_config_roundtrip_applies_defaults_and_stays_stable() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Legacy/pre-phase-0 style config with old fields only.
        fs::write(
            &config_path,
            r#"{
                "audio": {
                    "device_uid": "legacy-mic",
                    "audio_cues_enabled": false
                },
                "hotkeys": {
                    "primary": "Ctrl+Space",
                    "copy_last": "Ctrl+Shift+V",
                    "mode": "toggle"
                },
                "injection": {
                    "paste_delay_ms": 80,
                    "restore_clipboard": false,
                    "suffix": "\n"
                },
                "model": {
                    "model_id": "nvidia/parakeet-tdt-0.6b-v2",
                    "device": "auto"
                },
                "replacements": [
                    {
                        "pattern": "BTW",
                        "replacement": "by the way",
                        "enabled": true
                    }
                ],
                "ui": {
                    "show_on_startup": false,
                    "window_width": 800,
                    "window_height": 640
                }
            }"#,
        )
        .unwrap();

        let loaded = load_config_from_path(&config_path);

        // Existing values are preserved.
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.audio.device_uid.as_deref(), Some("legacy-mic"));
        assert!(!loaded.audio.audio_cues_enabled);
        assert_eq!(loaded.hotkeys.primary, "Ctrl+Space");
        assert_eq!(loaded.hotkeys.copy_last, "Ctrl+Shift+V");
        assert_eq!(loaded.hotkeys.mode, HotkeyMode::Toggle);
        assert_eq!(loaded.injection.paste_delay_ms, 80);
        assert!(!loaded.injection.restore_clipboard);
        assert_eq!(loaded.injection.suffix, "\n");
        assert_eq!(
            loaded.model.as_ref().and_then(|m| m.model_id.as_deref()),
            Some("nvidia/parakeet-tdt-0.6b-v2")
        );
        assert_eq!(
            loaded.model.as_ref().and_then(|m| m.device.as_deref()),
            Some("auto")
        );
        assert!(!loaded.ui.show_on_startup);
        assert_eq!(loaded.ui.window_width, 800);
        assert_eq!(loaded.ui.window_height, 640);

        // New defaults are applied.
        assert!(loaded.audio.trim_silence);
        assert!(!loaded.audio.vad_enabled);
        assert_eq!(loaded.audio.vad_silence_ms, 1200);
        assert_eq!(loaded.audio.vad_min_speech_ms, 250);
        assert!(loaded.injection.focus_guard_enabled);
        assert!(loaded.injection.app_overrides.is_empty());
        assert_eq!(
            loaded.model.as_ref().map(|m| m.preferred_device.as_str()),
            Some("auto")
        );
        assert_eq!(
            loaded.model.as_ref().and_then(|m| m.language.as_deref()),
            None
        );
        assert_eq!(loaded.ui.theme, "system");
        assert!(loaded.ui.onboarding_completed);
        assert!(loaded.ui.overlay_enabled);
        assert_eq!(loaded.ui.locale, None);
        assert!(!loaded.ui.reduce_motion);
        assert_eq!(loaded.history.persistence_mode, "memory");
        assert_eq!(loaded.history.max_entries, 100);
        assert!(loaded.history.encrypt_at_rest);
        assert_eq!(loaded.presets.enabled_presets, Vec::<String>::new());

        assert_eq!(loaded.replacements.len(), 1);
        assert!(Uuid::parse_str(&loaded.replacements[0].id).is_ok());
        assert_eq!(loaded.replacements[0].kind, "literal");
        assert!(!loaded.replacements[0].word_boundary);
        assert!(loaded.replacements[0].case_sensitive);
        assert!(loaded.replacements[0].description.is_none());
        assert!(loaded.replacements[0].origin.is_none());

        // Save and verify defaults were materialized.
        save_config_to_path(&loaded, &config_path).unwrap();
        let saved_json = fs::read_to_string(&config_path).unwrap();
        let saved_value: Value = serde_json::from_str(&saved_json).unwrap();
        assert!(saved_value["audio"].get("trim_silence").is_some());
        assert!(saved_value["audio"].get("vad_enabled").is_some());
        assert!(saved_value["audio"].get("vad_silence_ms").is_some());
        assert!(saved_value["audio"].get("vad_min_speech_ms").is_some());
        assert!(saved_value["injection"].get("app_overrides").is_some());
        assert!(saved_value["model"].get("preferred_device").is_some());
        assert!(saved_value["history"].get("persistence_mode").is_some());
        assert!(saved_value["history"].get("max_entries").is_some());
        assert!(saved_value["history"].get("encrypt_at_rest").is_some());
        assert!(saved_value["ui"].get("theme").is_some());
        assert!(saved_value["ui"].get("onboarding_completed").is_some());
        assert!(saved_value["ui"].get("overlay_enabled").is_some());
        assert!(saved_value["replacements"][0].get("id").is_some());
        assert!(saved_value["replacements"][0].get("kind").is_some());
        assert!(saved_value["replacements"][0]
            .get("word_boundary")
            .is_some());
        assert!(saved_value["replacements"][0]
            .get("case_sensitive")
            .is_some());

        // Load again and ensure stable round-trip.
        let reloaded = load_config_from_path(&config_path);
        let loaded_value = serde_json::to_value(&loaded).unwrap();
        let reloaded_value = serde_json::to_value(&reloaded).unwrap();
        assert_eq!(loaded_value, reloaded_value);
    }

    #[test]
    fn test_invalid_new_fields_in_file_fall_back_to_safe_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "schema_version": 1,
                "audio": {
                    "vad_silence_ms": 50,
                    "vad_min_speech_ms": 50000
                },
                "model": {
                    "model_id": "nvidia/parakeet-tdt-0.6b-v2",
                    "device": "auto",
                    "preferred_device": "tpu",
                    "language": "english"
                },
                "ui": {
                    "theme": "invalid-theme"
                },
                "history": {
                    "persistence_mode": "remote",
                    "max_entries": 50000
                }
            }"#,
        )
        .unwrap();

        let loaded = load_config_from_path(&config_path);
        assert_eq!(loaded.audio.vad_silence_ms, 400);
        assert_eq!(loaded.audio.vad_min_speech_ms, 2000);
        assert_eq!(
            loaded.model.as_ref().map(|m| m.preferred_device.as_str()),
            Some("auto")
        );
        assert_eq!(
            loaded.model.as_ref().and_then(|m| m.language.as_deref()),
            None
        );
        assert_eq!(loaded.ui.theme, "system");
        assert_eq!(loaded.history.persistence_mode, "memory");
        assert_eq!(loaded.history.max_entries, 2000);
    }

    #[test]
    fn test_replacement_rules_without_id_get_generated_and_stable_ids() {
        use std::collections::HashSet;

        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "schema_version": 1,
                "replacements": [
                    {"pattern": "BTW", "replacement": "by the way", "enabled": true},
                    {"pattern": "ASAP", "replacement": "as soon as possible", "enabled": false}
                ]
            }"#,
        )
        .unwrap();

        let loaded = load_config_from_path(&config_path);
        assert_eq!(loaded.replacements.len(), 2);
        assert_eq!(loaded.replacements[0].kind, "literal");
        assert!(!loaded.replacements[0].word_boundary);
        assert!(loaded.replacements[0].case_sensitive);
        assert!(loaded.replacements[0].description.is_none());
        assert!(loaded.replacements[0].origin.is_none());
        assert_eq!(loaded.replacements[0].pattern, "BTW");
        assert_eq!(loaded.replacements[1].kind, "literal");
        assert!(!loaded.replacements[1].word_boundary);
        assert!(loaded.replacements[1].case_sensitive);
        assert!(loaded.replacements[1].description.is_none());
        assert!(loaded.replacements[1].origin.is_none());
        assert_eq!(loaded.replacements[1].pattern, "ASAP");
        assert_ne!(loaded.replacements[0].id, loaded.replacements[1].id);
        assert!(Uuid::parse_str(&loaded.replacements[0].id).is_ok());
        assert!(Uuid::parse_str(&loaded.replacements[1].id).is_ok());

        let initial_ids: HashSet<String> =
            loaded.replacements.iter().map(|r| r.id.clone()).collect();
        save_config_to_path(&loaded, &config_path).unwrap();
        let reloaded = load_config_from_path(&config_path);
        let reloaded_ids: HashSet<String> =
            reloaded.replacements.iter().map(|r| r.id.clone()).collect();
        assert_eq!(initial_ids, reloaded_ids);
    }

    #[test]
    fn test_missing_optional_fields_get_defaults() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Write minimal valid config
        fs::write(&config_path, r#"{"schema_version": 1}"#).unwrap();

        let config = load_config_from_path(&config_path);
        assert_eq!(config.schema_version, 1);
        // All optional fields should have defaults
        assert!(config.audio.device_uid.is_none());
        assert!(config.audio.audio_cues_enabled);
        assert!(config.audio.trim_silence);
        assert!(!config.audio.vad_enabled);
        assert_eq!(config.audio.vad_silence_ms, 1200);
        assert_eq!(config.audio.vad_min_speech_ms, 250);
        assert_eq!(config.hotkeys.primary, "Ctrl+Shift+Space");
        assert_eq!(config.injection.paste_delay_ms, 40);
        assert!(config.injection.app_overrides.is_empty());
        assert_eq!(config.ui.theme, "system");
        assert!(config.ui.onboarding_completed); // Existing config file should skip onboarding
        assert!(config.ui.overlay_enabled);
        assert_eq!(config.ui.locale, None);
        assert!(!config.ui.reduce_motion);
        assert_eq!(config.history.persistence_mode, "memory");
        assert_eq!(config.history.max_entries, 100);
        assert!(config.history.encrypt_at_rest);
    }

    #[test]
    fn test_paste_delay_clamping() {
        let mut config = AppConfig::default();

        // Test below minimum
        config.injection.paste_delay_ms = 5;
        config.validate_and_clamp();
        assert_eq!(config.injection.paste_delay_ms, 10);

        // Test above maximum
        config.injection.paste_delay_ms = 1000;
        config.validate_and_clamp();
        assert_eq!(config.injection.paste_delay_ms, 500);

        // Test within range
        config.injection.paste_delay_ms = 200;
        config.validate_and_clamp();
        assert_eq!(config.injection.paste_delay_ms, 200);
    }

    #[test]
    fn test_app_override_paste_delay_clamping() {
        let mut config = AppConfig::default();
        config.injection.app_overrides.insert(
            "slack".to_string(),
            AppOverride {
                paste_delay_ms: Some(700),
                use_clipboard_only: Some(false),
            },
        );

        config.validate_and_clamp();
        assert_eq!(
            config
                .injection
                .app_overrides
                .get("slack")
                .and_then(|ov| ov.paste_delay_ms),
            Some(500)
        );
    }

    #[test]
    fn test_invalid_app_override_key_is_removed() {
        let mut config = AppConfig::default();
        config.injection.app_overrides.insert(
            "".to_string(),
            AppOverride {
                paste_delay_ms: Some(120),
                use_clipboard_only: Some(true),
            },
        );

        config.validate_and_clamp();
        assert!(config.injection.app_overrides.is_empty());
    }

    #[test]
    fn test_vad_timing_clamping() {
        let mut config = AppConfig::default();

        config.audio.vad_silence_ms = 100;
        config.audio.vad_min_speech_ms = 50;
        config.validate_and_clamp();
        assert_eq!(config.audio.vad_silence_ms, 400);
        assert_eq!(config.audio.vad_min_speech_ms, 100);

        config.audio.vad_silence_ms = 10000;
        config.audio.vad_min_speech_ms = 5000;
        config.validate_and_clamp();
        assert_eq!(config.audio.vad_silence_ms, 5000);
        assert_eq!(config.audio.vad_min_speech_ms, 2000);
    }

    #[test]
    fn test_device_uid_persists() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        // Save with device UID
        let mut config = AppConfig::default();
        config.audio.device_uid = Some("usb:abc123def456".to_string());
        save_config_to_path(&config, &config_path).unwrap();

        // Load and verify
        let loaded = load_config_from_path(&config_path);
        assert_eq!(
            loaded.audio.device_uid,
            Some("usb:abc123def456".to_string())
        );
        assert!(loaded.audio.trim_silence);
        assert!(!loaded.audio.vad_enabled);
        assert_eq!(loaded.audio.vad_silence_ms, 1200);
        assert_eq!(loaded.audio.vad_min_speech_ms, 250);
    }

    #[test]
    fn test_empty_hotkey_gets_default() {
        let mut config = AppConfig::default();
        config.hotkeys.primary = "".to_string();
        config.hotkeys.copy_last = "".to_string();

        config.validate_and_clamp();

        assert_eq!(config.hotkeys.primary, "Ctrl+Shift+Space");
        assert_eq!(config.hotkeys.copy_last, "Ctrl+Shift+V");
    }

    #[test]
    fn test_config_serialization() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();

        // Verify key fields are present
        assert!(json.contains("schema_version"));
        assert!(json.contains("audio"));
        assert!(json.contains("trim_silence"));
        assert!(json.contains("vad_enabled"));
        assert!(json.contains("vad_silence_ms"));
        assert!(json.contains("vad_min_speech_ms"));
        assert!(json.contains("hotkeys"));
        assert!(json.contains("injection"));
        assert!(json.contains("app_overrides"));
        assert!(json.contains("onboarding_completed"));
        assert!(json.contains("overlay_enabled"));
        assert!(json.contains("locale"));
        assert!(json.contains("reduce_motion"));
        assert!(json.contains("history"));
        assert!(json.contains("persistence_mode"));
        assert!(json.contains("max_entries"));
        assert!(json.contains("encrypt_at_rest"));
    }

    #[test]
    fn test_hotkey_mode_serialization() {
        let hold = HotkeyMode::Hold;
        let toggle = HotkeyMode::Toggle;

        assert_eq!(serde_json::to_string(&hold).unwrap(), "\"hold\"");
        assert_eq!(serde_json::to_string(&toggle).unwrap(), "\"toggle\"");

        let parsed: HotkeyMode = serde_json::from_str("\"toggle\"").unwrap();
        assert_eq!(parsed, HotkeyMode::Toggle);
    }

    #[test]
    fn test_creates_parent_directories() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir
            .path()
            .join("nested")
            .join("dirs")
            .join("config.json");

        let config = AppConfig::default();
        save_config_to_path(&config, &config_path).unwrap();

        assert!(config_path.exists());
    }

    #[test]
    fn test_window_dimension_validation() {
        let mut config = AppConfig::default();
        config.ui.window_width = 50; // Too small
        config.ui.window_height = 100; // Too small

        config.validate_and_clamp();

        assert_eq!(config.ui.window_width, 200);
        assert_eq!(config.ui.window_height, 200);
    }

    #[test]
    fn test_invalid_theme_gets_default() {
        let mut config = AppConfig::default();
        config.ui.theme = "ultra-dark".to_string();

        config.validate_and_clamp();

        assert_eq!(config.ui.theme, "system");
    }

    #[test]
    fn test_invalid_history_persistence_mode_gets_default() {
        let mut config = AppConfig::default();
        config.history.persistence_mode = "remote".to_string();

        config.validate_and_clamp();

        assert_eq!(config.history.persistence_mode, "memory");
    }

    #[test]
    fn test_history_max_entries_clamping() {
        let mut config = AppConfig::default();
        config.history.max_entries = 5;
        config.validate_and_clamp();
        assert_eq!(config.history.max_entries, 10);

        config.history.max_entries = 5000;
        config.validate_and_clamp();
        assert_eq!(config.history.max_entries, 2000);
    }

    #[test]
    fn test_missing_theme_in_ui_object_defaults_to_system() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "schema_version": 1,
                "ui": {
                    "show_on_startup": true,
                    "window_width": 640,
                    "window_height": 480
                }
            }"#,
        )
        .unwrap();

        let config = load_config_from_path(&config_path);
        assert_eq!(config.ui.theme, "system");
        assert!(config.ui.onboarding_completed);
        assert!(config.ui.overlay_enabled);
        assert_eq!(config.ui.locale, None);
        assert!(!config.ui.reduce_motion);
    }

    #[test]
    fn test_existing_config_preserves_explicit_onboarding_false() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "schema_version": 1,
                "ui": {
                    "onboarding_completed": false
                }
            }"#,
        )
        .unwrap();

        let config = load_config_from_path(&config_path);
        assert!(!config.ui.onboarding_completed);
        assert!(config.ui.overlay_enabled);
        assert_eq!(config.ui.locale, None);
        assert!(!config.ui.reduce_motion);
    }

    #[test]
    fn test_existing_config_preserves_explicit_overlay_false() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "schema_version": 1,
                "ui": {
                    "overlay_enabled": false
                }
            }"#,
        )
        .unwrap();

        let config = load_config_from_path(&config_path);
        assert!(!config.ui.overlay_enabled);
        assert_eq!(config.ui.locale, None);
        assert!(!config.ui.reduce_motion);
    }

    #[test]
    fn test_existing_config_preserves_explicit_locale_string() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "schema_version": 1,
                "ui": {
                    "locale": "ja-JP"
                }
            }"#,
        )
        .unwrap();

        let config = load_config_from_path(&config_path);
        assert_eq!(config.ui.locale, Some("ja-JP".to_string()));
        assert!(!config.ui.reduce_motion);
    }

    #[test]
    fn test_existing_config_preserves_explicit_locale_null() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "schema_version": 1,
                "ui": {
                    "locale": null
                }
            }"#,
        )
        .unwrap();

        let config = load_config_from_path(&config_path);
        assert_eq!(config.ui.locale, None);
        assert!(!config.ui.reduce_motion);
    }

    #[test]
    fn test_existing_config_preserves_explicit_reduce_motion_true() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "schema_version": 1,
                "ui": {
                    "reduce_motion": true
                }
            }"#,
        )
        .unwrap();

        let config = load_config_from_path(&config_path);
        assert!(config.ui.reduce_motion);
    }

    #[test]
    fn test_existing_config_preserves_explicit_history_persistence_mode_disk() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "schema_version": 1,
                "history": {
                    "persistence_mode": "disk"
                }
            }"#,
        )
        .unwrap();

        let config = load_config_from_path(&config_path);
        assert_eq!(config.history.persistence_mode, "disk");
        assert_eq!(config.history.max_entries, 100);
        assert!(config.history.encrypt_at_rest);
    }

    #[test]
    fn test_model_preferred_device_defaults_to_auto_when_missing() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        fs::write(
            &config_path,
            r#"{
                "schema_version": 1,
                "model": {
                    "model_id": "nvidia/parakeet-tdt-0.6b-v2"
                }
            }"#,
        )
        .unwrap();

        let config = load_config_from_path(&config_path);
        assert_eq!(
            config.model.as_ref().map(|m| m.preferred_device.as_str()),
            Some("auto")
        );
    }

    #[test]
    fn test_effective_model_device_pref_uses_concrete_model_device() {
        let mut config = AppConfig::default();
        config.model = Some(ModelConfig {
            model_id: None,
            device: Some("cuda".to_string()),
            preferred_device: "cpu".to_string(),
            language: None,
        });

        assert_eq!(config.effective_model_device_pref(), "cuda");
    }

    #[test]
    fn test_effective_model_device_pref_maps_preferred_device_for_non_concrete_device() {
        let mut config = AppConfig::default();
        config.model = Some(ModelConfig {
            model_id: None,
            device: Some("auto".to_string()),
            preferred_device: "cpu".to_string(),
            language: None,
        });
        assert_eq!(config.effective_model_device_pref(), "cpu");

        config.model = Some(ModelConfig {
            model_id: None,
            device: Some("cpu".to_string()),
            preferred_device: "gpu".to_string(),
            language: None,
        });
        assert_eq!(
            config.effective_model_device_pref(),
            preferred_gpu_backend()
        );
    }

    #[test]
    fn test_invalid_model_preferred_device_gets_reset_to_auto() {
        let mut config = AppConfig::default();
        config.model = Some(ModelConfig {
            model_id: None,
            device: Some("auto".to_string()),
            preferred_device: "tpu".to_string(),
            language: None,
        });

        config.validate_and_clamp();

        assert_eq!(
            config.model.as_ref().map(|m| m.preferred_device.as_str()),
            Some("auto")
        );
    }

    #[test]
    fn test_model_language_roundtrip_null_auto_iso() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        for language in [None, Some("auto"), Some("ja")] {
            let mut config = AppConfig::default();
            config.model = Some(ModelConfig {
                model_id: Some("nvidia/parakeet-tdt-0.6b-v2".to_string()),
                device: Some("auto".to_string()),
                preferred_device: "auto".to_string(),
                language: language.map(std::string::ToString::to_string),
            });

            save_config_to_path(&config, &config_path).unwrap();
            let loaded = load_config_from_path(&config_path);

            assert_eq!(
                loaded
                    .model
                    .as_ref()
                    .and_then(|model| model.language.as_deref()),
                language
            );
        }
    }

    #[test]
    fn test_validate_and_clamp_resets_unknown_model_language_value_to_null() {
        let mut config = AppConfig::default();
        config.model = Some(ModelConfig {
            model_id: None,
            device: Some("auto".to_string()),
            preferred_device: "auto".to_string(),
            language: Some("english".to_string()),
        });

        config.validate_and_clamp();

        assert_eq!(
            config
                .model
                .as_ref()
                .and_then(|model| model.language.as_deref()),
            None
        );
    }

    #[test]
    fn test_invalid_replacement_kind_gets_defaulted() {
        let mut config = AppConfig::default();
        config.replacements = vec![ReplacementRule {
            id: "rule-invalid-kind".to_string(),
            kind: "wildcard".to_string(),
            pattern: "abc".to_string(),
            replacement: "def".to_string(),
            enabled: true,
            word_boundary: false,
            case_sensitive: true,
            description: None,
            origin: None,
        }];

        config.validate_and_clamp();
        assert_eq!(config.replacements[0].kind, "literal");
    }

    #[test]
    fn test_invalid_replacement_origin_gets_cleared() {
        let mut config = AppConfig::default();
        config.replacements = vec![ReplacementRule {
            id: "rule-invalid-origin".to_string(),
            kind: "literal".to_string(),
            pattern: "abc".to_string(),
            replacement: "def".to_string(),
            enabled: true,
            word_boundary: false,
            case_sensitive: true,
            description: None,
            origin: Some("system".to_string()),
        }];

        config.validate_and_clamp();
        assert_eq!(config.replacements[0].origin, None);
    }

    #[test]
    fn test_sensitive_unknown_config_fields_detected_for_nested_objects() {
        let config = serde_json::json!({
            "schema_version": 1,
            "auth_token": "dont-store-me",
            "audio": {
                "device_uid": "mic-1",
                "api_key": "also-bad"
            },
            "injection": {
                "app_overrides": {
                    "slack": {
                        "paste_delay_ms": 40,
                        "session_secret": "bad"
                    }
                }
            },
            "replacements": [
                {
                    "pattern": "foo",
                    "replacement": "bar",
                    "enabled": true,
                    "token_value": "bad"
                }
            ],
            "ui": {
                "password_hint": "bad"
            }
        });

        let unknown = sensitive_unknown_config_fields(&config);
        assert!(unknown.contains(&"auth_token".to_string()));
        assert!(unknown.contains(&"audio.api_key".to_string()));
        assert!(unknown.contains(&"injection.app_overrides.slack.session_secret".to_string()));
        assert!(unknown.contains(&"replacements[0].token_value".to_string()));
        assert!(unknown.contains(&"ui.password_hint".to_string()));
    }

    #[test]
    fn test_sensitive_unknown_config_fields_ignore_known_sensitive_named_fields() {
        let config = serde_json::json!({
            "schema_version": 1,
            "hotkeys": {
                "primary": "Ctrl+Shift+Space",
                "copy_last": "Ctrl+Shift+V",
                "mode": "hold"
            }
        });

        let unknown = sensitive_unknown_config_fields(&config);
        assert!(unknown.is_empty());
    }
}
