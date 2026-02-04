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
use std::fs;
use std::io;
use std::path::PathBuf;

/// Current schema version.
const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Default config directory name.
const CONFIG_DIR_NAME: &str = "OpenVoicy";

/// Config file name.
const CONFIG_FILE_NAME: &str = "config.json";

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

    /// Text replacement rules.
    #[serde(default)]
    pub replacements: Vec<ReplacementRule>,

    /// UI settings.
    #[serde(default)]
    pub ui: UiConfig,

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
            replacements: Vec::new(),
            ui: UiConfig::default(),
            presets: PresetsConfig::default(),
        }
    }
}

impl AppConfig {
    /// Validate and clamp config values to valid ranges.
    pub fn validate_and_clamp(&mut self) {
        // Clamp paste delay
        self.injection.paste_delay_ms = self.injection.paste_delay_ms.clamp(10, 500);

        // Validate hotkey format (basic check - ensure non-empty)
        if self.hotkeys.primary.is_empty() {
            self.hotkeys.primary = HotkeyConfig::default().primary;
        }
        if self.hotkeys.copy_last.is_empty() {
            self.hotkeys.copy_last = HotkeyConfig::default().copy_last;
        }

        // Validate window dimensions (minimum 200x200)
        self.ui.window_width = self.ui.window_width.max(200);
        self.ui.window_height = self.ui.window_height.max(200);
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
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device_uid: None, // Use system default
            audio_cues_enabled: true,
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
}

impl Default for InjectionConfig {
    fn default() -> Self {
        Self {
            paste_delay_ms: 40,
            restore_clipboard: true,
            suffix: " ".to_string(), // Single space
            focus_guard_enabled: true,
        }
    }
}

/// Text replacement rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplacementRule {
    /// Pattern to match.
    pub pattern: String,
    /// Replacement text.
    pub replacement: String,
    /// Whether this rule is enabled.
    pub enabled: bool,
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
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_on_startup: true, // First run shows settings
            window_width: 600,
            window_height: 500,
        }
    }
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

    // Future migrations go here:
    // if version < 2 { ... }

    serde_json::from_value(config).unwrap_or_else(|e| {
        log::error!("Config migration failed, using defaults: {}", e);
        AppConfig::default()
    })
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
        assert_eq!(config.hotkeys.primary, "Ctrl+Shift+Space");
        assert_eq!(config.hotkeys.mode, HotkeyMode::Hold);
        assert_eq!(config.injection.paste_delay_ms, 40);
        assert!(config.injection.restore_clipboard);
        assert_eq!(config.injection.suffix, " ");
        assert!(config.injection.focus_guard_enabled);
        assert!(config.replacements.is_empty());
        assert!(config.ui.show_on_startup);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");

        let mut config = AppConfig::default();
        config.audio.device_uid = Some("test-device-uid".to_string());
        config.hotkeys.primary = "Ctrl+Space".to_string();
        config.injection.paste_delay_ms = 100;

        // Save
        save_config_to_path(&config, &config_path).unwrap();

        // Verify file exists
        assert!(config_path.exists());

        // Load
        let loaded = load_config_from_path(&config_path);
        assert_eq!(loaded.audio.device_uid, Some("test-device-uid".to_string()));
        assert_eq!(loaded.hotkeys.primary, "Ctrl+Space");
        assert_eq!(loaded.injection.paste_delay_ms, 100);
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
        assert!(config.injection.focus_guard_enabled); // Should be added by migration
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
        assert_eq!(config.hotkeys.primary, "Ctrl+Shift+Space");
        assert_eq!(config.injection.paste_delay_ms, 40);
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
        assert!(json.contains("hotkeys"));
        assert!(json.contains("injection"));
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
}
