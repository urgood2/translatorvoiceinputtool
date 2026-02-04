//! Comprehensive error handling for the Voice Input Tool.
//!
//! This module provides:
//! - Strongly-typed error kinds matching the sidecar protocol
//! - User-facing error messages with actionable remediation
//! - Mapping from internal errors to user-friendly messages
//!
//! # Error Categories
//!
//! | Category       | Error Kinds                        | Typical Remediation       |
//! |----------------|-----------------------------------|---------------------------|
//! | Hardware       | E_DEVICE_NOT_FOUND, E_AUDIO_IO    | Check connections         |
//! | Permissions    | E_MIC_PERMISSION                  | Open system settings      |
//! | Model          | E_MODEL_*, E_DISK_FULL, E_NETWORK | Retry, free disk space    |
//! | Recording      | E_ALREADY_RECORDING, E_NOT_RECORDING | Wait, retry            |
//! | Sidecar        | E_INTERNAL, crashes               | Restart sidecar           |

mod kinds;
mod remediation;

pub use kinds::ErrorKind;
pub use remediation::{Remediation, SettingsPage};

use serde::Serialize;

/// User-facing error with actionable information.
///
/// This struct is designed to be directly useful to the UI layer:
/// - `title` and `message` are human-readable
/// - `error_kind` links to the technical error for logging
/// - `remediation` tells the UI what action button to show
/// - `details` contains technical info for diagnostics (not shown to user)
#[derive(Debug, Clone, Serialize)]
pub struct UserError {
    /// Short error title (for notification headers, dialogs).
    pub title: String,
    /// User-friendly message explaining what happened.
    pub message: String,
    /// Technical error kind (for logging/diagnostics).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<ErrorKind>,
    /// Suggested remediation action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<Remediation>,
    /// Technical details (not shown to user, for diagnostics).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl UserError {
    /// Create a new user error with all fields.
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        error_kind: Option<ErrorKind>,
        remediation: Option<Remediation>,
        details: Option<String>,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            error_kind,
            remediation,
            details,
        }
    }

    /// Create a simple user error with just title and message.
    pub fn simple(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            error_kind: None,
            remediation: None,
            details: None,
        }
    }

    /// Add remediation to an existing error.
    pub fn with_remediation(mut self, remediation: Remediation) -> Self {
        self.remediation = Some(remediation);
        self
    }

    /// Add technical details to an existing error.
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Add error kind to an existing error.
    pub fn with_kind(mut self, kind: ErrorKind) -> Self {
        self.error_kind = Some(kind);
        self
    }
}

/// Application-level errors that can be converted to user errors.
#[derive(Debug, Clone)]
pub enum AppError {
    // === Hardware/Device Errors ===
    /// No microphone available on the system.
    NoMicrophone,
    /// Microphone permission denied by OS.
    MicrophonePermissionDenied,
    /// Audio device disconnected during recording.
    DeviceDisconnected { during_recording: bool },
    /// Selected audio device not found.
    DeviceNotFound { device_uid: Option<String> },
    /// Audio I/O error.
    AudioIO { message: String },

    // === Sidecar Errors ===
    /// Sidecar process crashed.
    SidecarCrash { restart_count: u32 },
    /// Sidecar not responding (watchdog timeout).
    SidecarHang,
    /// Sidecar failed to start after max retries.
    SidecarMaxRetries { retry_count: u32 },
    /// Sidecar executable not found.
    SidecarNotFound,
    /// Sidecar blocked by OS (macOS quarantine).
    SidecarQuarantined,

    // === Model Errors ===
    /// Model download failed due to network error.
    ModelDownloadNetwork { url: Option<String> },
    /// Model download failed due to disk full.
    ModelDownloadDiskFull { required_bytes: u64, available_bytes: u64 },
    /// Model cache corrupted.
    ModelCacheCorrupt,
    /// Model failed to load.
    ModelLoadFailed { model_id: String },
    /// Model not found.
    ModelNotFound { model_id: String },
    /// Model purge rejected (in use).
    ModelPurgeRejected,
    /// Model not initialized.
    ModelNotInitialized,

    // === Recording Errors ===
    /// Recording too short to process.
    RecordingTooShort { duration_ms: u32 },
    /// Already recording (duplicate start).
    AlreadyRecording,
    /// Not recording (stop called when not recording).
    NotRecording,
    /// Invalid session ID.
    InvalidSession { expected: String, actual: String },
    /// Recording max duration reached.
    RecordingMaxDuration { duration_secs: u32 },

    // === Transcription Errors ===
    /// Transcription timed out.
    TranscriptionTimeout { timeout_secs: u32 },
    /// Transcription failed.
    TranscriptionFailed { message: String },

    // === Hotkey Errors ===
    /// Hotkey conflict with another application.
    HotkeyConflict { hotkey: String },
    /// Wayland portal unavailable.
    WaylandPortalUnavailable,

    // === Injection Errors ===
    /// Focus changed during transcription.
    FocusChanged,
    /// Self-injection prevented (own window focused).
    SelfInjectionPrevented,
    /// Accessibility permission denied (macOS).
    AccessibilityPermissionDenied,
    /// Clipboard-only mode active (not an error, informational).
    ClipboardOnlyMode,

    // === Internal Errors ===
    /// Generic internal error.
    Internal { message: String },
    /// Configuration error.
    Config { message: String },
}

impl AppError {
    /// Convert to a user-facing error.
    pub fn to_user_error(&self) -> UserError {
        map_error_to_user_message(self)
    }
}

/// Map an application error to a user-friendly message.
///
/// This is the central mapping function that converts internal errors
/// to user-facing messages. All remediation text is defined here for
/// easy maintenance and testing.
fn map_error_to_user_message(error: &AppError) -> UserError {
    match error {
        // === Hardware/Device Errors ===
        AppError::NoMicrophone => UserError::new(
            "No Microphone",
            "No microphone found. Connect a microphone and restart the application.",
            Some(ErrorKind::DeviceNotFound),
            Some(Remediation::OpenUrl(
                "https://docs.openvoicy.app/troubleshooting/no-mic".to_string(),
            )),
            None,
        ),

        AppError::MicrophonePermissionDenied => UserError::new(
            "Microphone Permission Required",
            "Microphone permission is required for voice input. Click to open settings.",
            Some(ErrorKind::MicPermission),
            Some(Remediation::OpenSettings(SettingsPage::MicrophonePermission)),
            None,
        ),

        AppError::DeviceDisconnected { during_recording } => {
            if *during_recording {
                UserError::new(
                    "Microphone Disconnected",
                    "Microphone was disconnected during recording. Recording cancelled.",
                    Some(ErrorKind::AudioIO),
                    None,
                    None,
                )
            } else {
                UserError::new(
                    "Microphone Disconnected",
                    "The selected microphone was disconnected.",
                    Some(ErrorKind::DeviceNotFound),
                    None,
                    None,
                )
            }
        }

        AppError::DeviceNotFound { device_uid } => {
            let message = if let Some(uid) = device_uid {
                format!(
                    "Previously selected microphone '{}' not found. Using default device.",
                    uid
                )
            } else {
                "Selected microphone not found. Using default device.".to_string()
            };
            UserError::new(
                "Microphone Not Found",
                message,
                Some(ErrorKind::DeviceNotFound),
                Some(Remediation::OpenSettings(SettingsPage::AudioDevice)),
                device_uid.clone(),
            )
        }

        AppError::AudioIO { message } => UserError::new(
            "Audio Error",
            "Failed to access audio device. Check connections and try again.",
            Some(ErrorKind::AudioIO),
            Some(Remediation::Retry),
            Some(message.clone()),
        ),

        // === Sidecar Errors ===
        AppError::SidecarCrash { restart_count } => UserError::new(
            "Background Service Crashed",
            if *restart_count > 0 {
                format!("Background service crashed. Restarting... (attempt {})", restart_count + 1)
            } else {
                "Background service crashed. Restarting...".to_string()
            },
            Some(ErrorKind::Internal),
            Some(Remediation::RestartSidecar),
            Some(format!("restart_count={}", restart_count)),
        ),

        AppError::SidecarHang => UserError::new(
            "Background Service Not Responding",
            "Background service is not responding. Restarting...",
            Some(ErrorKind::Internal),
            Some(Remediation::RestartSidecar),
            None,
        ),

        AppError::SidecarMaxRetries { retry_count } => UserError::new(
            "Background Service Failed",
            "Background service failed repeatedly. Click to retry or restart the application.",
            Some(ErrorKind::Internal),
            Some(Remediation::RestartApp),
            Some(format!("retry_count={}", retry_count)),
        ),

        AppError::SidecarNotFound => UserError::new(
            "Application Files Missing",
            "Required application files are missing. Please reinstall the application.",
            Some(ErrorKind::Internal),
            Some(Remediation::Reinstall),
            None,
        ),

        AppError::SidecarQuarantined => UserError::new(
            "Background Service Blocked",
            "Background service is blocked by macOS Gatekeeper. Open Security settings to allow.",
            Some(ErrorKind::Internal),
            Some(Remediation::OpenSettings(SettingsPage::MacOSSecurity)),
            None,
        ),

        // === Model Errors ===
        AppError::ModelDownloadNetwork { url } => UserError::new(
            "Download Failed",
            "Model download failed. Check your internet connection and try again.",
            Some(ErrorKind::Network),
            Some(Remediation::Retry),
            url.clone(),
        ),

        AppError::ModelDownloadDiskFull {
            required_bytes,
            available_bytes,
        } => {
            let required_gb = *required_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            let available_gb = *available_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            UserError::new(
                "Not Enough Disk Space",
                format!(
                    "Not enough disk space to download the model. Need {:.1} GB, have {:.1} GB available.",
                    required_gb, available_gb
                ),
                Some(ErrorKind::DiskFull),
                None,
                Some(format!(
                    "required_bytes={}, available_bytes={}",
                    required_bytes, available_bytes
                )),
            )
        }

        AppError::ModelCacheCorrupt => UserError::new(
            "Model Files Corrupted",
            "Model files are corrupted. Re-downloading...",
            Some(ErrorKind::CacheCorrupt),
            Some(Remediation::Retry),
            None,
        ),

        AppError::ModelLoadFailed { model_id } => UserError::new(
            "Model Load Failed",
            "Failed to load the speech recognition model. Click to re-download.",
            Some(ErrorKind::ModelLoad),
            Some(Remediation::Retry),
            Some(model_id.clone()),
        ),

        AppError::ModelNotFound { model_id } => UserError::new(
            "Model Not Found",
            "Speech recognition model not found. Downloading...",
            Some(ErrorKind::ModelNotFound),
            Some(Remediation::Retry),
            Some(model_id.clone()),
        ),

        AppError::ModelPurgeRejected => UserError::new(
            "Cannot Clear Model Cache",
            "Cannot clear model cache while transcription is in progress. Try again later.",
            Some(ErrorKind::NotReady),
            None,
            None,
        ),

        AppError::ModelNotInitialized => UserError::new(
            "Model Not Ready",
            "Speech recognition model is still loading. Please wait.",
            Some(ErrorKind::NotInitialized),
            None,
            None,
        ),

        // === Recording Errors ===
        AppError::RecordingTooShort { duration_ms } => UserError::new(
            "Recording Too Short",
            "Recording was too short to process. Hold the hotkey longer or use toggle mode.",
            None, // Not an error, just informational
            None,
            Some(format!("duration_ms={}", duration_ms)),
        ),

        AppError::AlreadyRecording => UserError::new(
            "Already Recording",
            "Recording is already in progress.",
            Some(ErrorKind::AlreadyRecording),
            None,
            None,
        ),

        AppError::NotRecording => UserError::new(
            "Not Recording",
            "No recording in progress.",
            Some(ErrorKind::NotRecording),
            None,
            None,
        ),

        AppError::InvalidSession { expected, actual } => UserError::new(
            "Session Mismatch",
            "Recording session mismatch. Please try again.",
            Some(ErrorKind::InvalidSession),
            Some(Remediation::Retry),
            Some(format!("expected={}, actual={}", expected, actual)),
        ),

        AppError::RecordingMaxDuration { duration_secs } => UserError::new(
            "Maximum Recording Length",
            format!(
                "Maximum recording length of {} seconds reached. Processing...",
                duration_secs
            ),
            None, // Not an error, auto-behavior
            None,
            None,
        ),

        // === Transcription Errors ===
        AppError::TranscriptionTimeout { timeout_secs } => UserError::new(
            "Transcription Timeout",
            format!(
                "Transcription timed out after {} seconds. Click to restart the service.",
                timeout_secs
            ),
            Some(ErrorKind::Internal),
            Some(Remediation::RestartSidecar),
            None,
        ),

        AppError::TranscriptionFailed { message } => UserError::new(
            "Transcription Failed",
            "Transcription failed. Recording discarded.",
            Some(ErrorKind::Transcription),
            Some(Remediation::Retry),
            Some(message.clone()),
        ),

        // === Hotkey Errors ===
        AppError::HotkeyConflict { hotkey } => UserError::new(
            "Hotkey Conflict",
            format!(
                "Hotkey [{}] is in use by another application. Click to change.",
                hotkey
            ),
            None, // Rust-side, no sidecar error kind
            Some(Remediation::OpenSettings(SettingsPage::Hotkey)),
            None,
        ),

        AppError::WaylandPortalUnavailable => UserError::new(
            "Global Shortcuts Unavailable",
            "Global shortcuts are not available on this Wayland compositor. The app will run in limited mode.",
            None,
            None,
            Some("Wayland GlobalShortcuts portal not available".to_string()),
        ),

        // === Injection Errors ===
        AppError::FocusChanged => UserError::new(
            "Window Changed",
            "Window changed during transcription. Text copied to clipboard instead.",
            None, // Not an error, Focus Guard behavior
            None,
            None,
        ),

        AppError::SelfInjectionPrevented => UserError::new(
            "Settings Window Focused",
            "Settings window was focused. Text copied to clipboard instead.",
            None, // Not an error, safety behavior
            None,
            None,
        ),

        AppError::AccessibilityPermissionDenied => UserError::new(
            "Accessibility Permission Required",
            "Accessibility permission is required for text injection. Click to open settings.",
            None, // Rust-side detection
            Some(Remediation::OpenSettings(SettingsPage::MacOSAccessibility)),
            None,
        ),

        AppError::ClipboardOnlyMode => UserError::new(
            "Clipboard Mode",
            "Direct text injection is not available. Text will be copied to clipboard.",
            None,
            None,
            None,
        ),

        // === Internal Errors ===
        AppError::Internal { message } => UserError::new(
            "Internal Error",
            "An unexpected error occurred. Please try again or restart the application.",
            Some(ErrorKind::Internal),
            Some(Remediation::RestartApp),
            Some(message.clone()),
        ),

        AppError::Config { message } => UserError::new(
            "Configuration Error",
            "Failed to load or save configuration. Settings may be reset to defaults.",
            None,
            Some(Remediation::OpenSettings(SettingsPage::General)),
            Some(message.clone()),
        ),
    }
}

/// Map a sidecar error kind string to an AppError.
///
/// This function is used when receiving errors from the sidecar via JSON-RPC.
pub fn from_sidecar_error(kind: &str, message: &str, details: Option<&str>) -> AppError {
    match kind {
        "E_MIC_PERMISSION" => AppError::MicrophonePermissionDenied,
        "E_DEVICE_NOT_FOUND" => AppError::DeviceNotFound {
            device_uid: details.map(|s| s.to_string()),
        },
        "E_AUDIO_IO" => AppError::AudioIO {
            message: message.to_string(),
        },
        "E_ALREADY_RECORDING" => AppError::AlreadyRecording,
        "E_NOT_RECORDING" => AppError::NotRecording,
        "E_INVALID_SESSION" => AppError::InvalidSession {
            expected: "unknown".to_string(),
            actual: details.unwrap_or("unknown").to_string(),
        },
        "E_DISK_FULL" => {
            // Parse bytes from details if available
            AppError::ModelDownloadDiskFull {
                required_bytes: 0,
                available_bytes: 0,
            }
        }
        "E_NETWORK" => AppError::ModelDownloadNetwork {
            url: details.map(|s| s.to_string()),
        },
        "E_CACHE_CORRUPT" => AppError::ModelCacheCorrupt,
        "E_NOT_READY" => AppError::ModelPurgeRejected,
        "E_MODEL_LOAD" => AppError::ModelLoadFailed {
            model_id: details.unwrap_or("unknown").to_string(),
        },
        "E_MODEL_NOT_FOUND" => AppError::ModelNotFound {
            model_id: details.unwrap_or("unknown").to_string(),
        },
        "E_NOT_INITIALIZED" => AppError::ModelNotInitialized,
        "E_TRANSCRIPTION" | "E_TRANSCRIBE" => AppError::TranscriptionFailed {
            message: message.to_string(),
        },
        "E_INTERNAL" | "E_MODEL" | "E_ASR" | "E_METER" => AppError::Internal {
            message: message.to_string(),
        },
        _ => AppError::Internal {
            message: format!("{}: {}", kind, message),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_app_errors_map_to_user_errors() {
        // Exhaustive test: every AppError variant must produce a valid UserError
        let errors = vec![
            AppError::NoMicrophone,
            AppError::MicrophonePermissionDenied,
            AppError::DeviceDisconnected {
                during_recording: true,
            },
            AppError::DeviceDisconnected {
                during_recording: false,
            },
            AppError::DeviceNotFound {
                device_uid: Some("test".to_string()),
            },
            AppError::DeviceNotFound { device_uid: None },
            AppError::AudioIO {
                message: "test".to_string(),
            },
            AppError::SidecarCrash { restart_count: 0 },
            AppError::SidecarCrash { restart_count: 3 },
            AppError::SidecarHang,
            AppError::SidecarMaxRetries { retry_count: 5 },
            AppError::SidecarNotFound,
            AppError::SidecarQuarantined,
            AppError::ModelDownloadNetwork { url: None },
            AppError::ModelDownloadDiskFull {
                required_bytes: 1000,
                available_bytes: 500,
            },
            AppError::ModelCacheCorrupt,
            AppError::ModelLoadFailed {
                model_id: "test".to_string(),
            },
            AppError::ModelNotFound {
                model_id: "test".to_string(),
            },
            AppError::ModelPurgeRejected,
            AppError::ModelNotInitialized,
            AppError::RecordingTooShort { duration_ms: 100 },
            AppError::AlreadyRecording,
            AppError::NotRecording,
            AppError::InvalidSession {
                expected: "a".to_string(),
                actual: "b".to_string(),
            },
            AppError::RecordingMaxDuration { duration_secs: 60 },
            AppError::TranscriptionTimeout { timeout_secs: 30 },
            AppError::TranscriptionFailed {
                message: "test".to_string(),
            },
            AppError::HotkeyConflict {
                hotkey: "Ctrl+Space".to_string(),
            },
            AppError::WaylandPortalUnavailable,
            AppError::FocusChanged,
            AppError::SelfInjectionPrevented,
            AppError::AccessibilityPermissionDenied,
            AppError::ClipboardOnlyMode,
            AppError::Internal {
                message: "test".to_string(),
            },
            AppError::Config {
                message: "test".to_string(),
            },
        ];

        for error in errors {
            let user_error = error.to_user_error();
            // Every error must have a non-empty title and message
            assert!(!user_error.title.is_empty(), "Missing title for {:?}", error);
            assert!(
                !user_error.message.is_empty(),
                "Missing message for {:?}",
                error
            );
            // Messages should not contain raw exception text or paths
            assert!(
                !user_error.message.contains("Traceback"),
                "Raw traceback in message for {:?}",
                error
            );
            assert!(
                !user_error.message.contains("/home/"),
                "Path in message for {:?}",
                error
            );
        }
    }

    #[test]
    fn test_user_error_serialization() {
        let error = UserError::new(
            "Test Error",
            "This is a test",
            Some(ErrorKind::Internal),
            Some(Remediation::Retry),
            Some("details".to_string()),
        );

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("Test Error"));
        assert!(json.contains("This is a test"));
        assert!(json.contains("internal"));
        assert!(json.contains("retry"));
    }

    #[test]
    fn test_from_sidecar_error() {
        let error = from_sidecar_error("E_MIC_PERMISSION", "Permission denied", None);
        assert!(matches!(error, AppError::MicrophonePermissionDenied));

        let error = from_sidecar_error("E_DEVICE_NOT_FOUND", "Not found", Some("usb-mic-1"));
        match error {
            AppError::DeviceNotFound { device_uid } => {
                assert_eq!(device_uid, Some("usb-mic-1".to_string()));
            }
            _ => panic!("Wrong error type"),
        }

        let error = from_sidecar_error("E_UNKNOWN", "Something went wrong", None);
        assert!(matches!(error, AppError::Internal { .. }));
    }

    #[test]
    fn test_error_kinds_have_remediation_where_applicable() {
        // Errors that should have remediation
        let errors_with_remediation = vec![
            AppError::NoMicrophone,
            AppError::MicrophonePermissionDenied,
            AppError::DeviceNotFound { device_uid: None },
            AppError::SidecarCrash { restart_count: 0 },
            AppError::SidecarMaxRetries { retry_count: 5 },
            AppError::ModelDownloadNetwork { url: None },
            AppError::AccessibilityPermissionDenied,
        ];

        for error in errors_with_remediation {
            let user_error = error.to_user_error();
            assert!(
                user_error.remediation.is_some(),
                "Missing remediation for {:?}",
                error
            );
        }
    }
}
