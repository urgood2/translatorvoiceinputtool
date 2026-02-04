//! Error kind definitions matching the sidecar protocol.
//!
//! These error kinds correspond to the `E_*` codes returned by the
//! Python sidecar in JSON-RPC error responses.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Error kinds that can occur in the application.
///
/// These are stable identifiers that can be used for:
/// - Logging and diagnostics
/// - Error tracking/analytics
/// - Matching errors to remediation text
///
/// The string representation matches the sidecar's `E_*` codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    // === Hardware/Device Errors ===
    /// Microphone permission denied by OS.
    MicPermission,
    /// Audio device not found.
    DeviceNotFound,
    /// Audio I/O error.
    AudioIO,
    /// Audio device unavailable (in use, etc.).
    DeviceUnavailable,

    // === Recording Errors ===
    /// Already recording.
    AlreadyRecording,
    /// Not currently recording.
    NotRecording,
    /// Invalid session ID.
    InvalidSession,
    /// Recording error (generic).
    Recording,

    // === Audio Meter Errors ===
    /// Meter already running.
    MeterRunning,
    /// Meter not running.
    MeterNotRunning,
    /// Meter error (generic).
    Meter,

    // === Model Errors ===
    /// Model not found.
    ModelNotFound,
    /// Model failed to load.
    ModelLoad,
    /// Model not initialized.
    NotInitialized,
    /// Model not ready (e.g., busy).
    NotReady,
    /// Model error (generic).
    Model,
    /// ASR error (generic).
    Asr,

    // === Cache/Storage Errors ===
    /// Not enough disk space.
    DiskFull,
    /// Cache corrupted.
    CacheCorrupt,
    /// Lock acquisition failed.
    Lock,

    // === Network Errors ===
    /// Network error.
    Network,

    // === Transcription Errors ===
    /// Transcription failed.
    Transcription,

    // === Protocol Errors ===
    /// Method not found.
    MethodNotFound,
    /// Invalid parameters.
    InvalidParams,
    /// Parse error.
    ParseError,

    // === Replacement Errors ===
    /// Replacement rule error.
    Replacement,
    /// Resource not found.
    NotFound,

    // === Internal Errors ===
    /// Internal error.
    Internal,
}

impl ErrorKind {
    /// Convert a sidecar error kind string to an ErrorKind.
    ///
    /// Returns `None` if the string is not a recognized error kind.
    pub fn from_sidecar(kind: &str) -> Option<Self> {
        match kind {
            "E_MIC_PERMISSION" => Some(Self::MicPermission),
            "E_DEVICE_NOT_FOUND" => Some(Self::DeviceNotFound),
            "E_AUDIO_IO" => Some(Self::AudioIO),
            "E_DEVICE_UNAVAILABLE" => Some(Self::DeviceUnavailable),
            "E_ALREADY_RECORDING" => Some(Self::AlreadyRecording),
            "E_NOT_RECORDING" => Some(Self::NotRecording),
            "E_INVALID_SESSION" => Some(Self::InvalidSession),
            "E_RECORDING" => Some(Self::Recording),
            "E_METER_RUNNING" => Some(Self::MeterRunning),
            "E_METER_NOT_RUNNING" => Some(Self::MeterNotRunning),
            "E_METER" => Some(Self::Meter),
            "E_MODEL_NOT_FOUND" => Some(Self::ModelNotFound),
            "E_MODEL_LOAD" => Some(Self::ModelLoad),
            "E_NOT_INITIALIZED" => Some(Self::NotInitialized),
            "E_NOT_READY" => Some(Self::NotReady),
            "E_MODEL" => Some(Self::Model),
            "E_ASR" => Some(Self::Asr),
            "E_DISK_FULL" => Some(Self::DiskFull),
            "E_CACHE_CORRUPT" => Some(Self::CacheCorrupt),
            "E_LOCK" => Some(Self::Lock),
            "E_NETWORK" => Some(Self::Network),
            "E_TRANSCRIPTION" | "E_TRANSCRIBE" => Some(Self::Transcription),
            "E_METHOD_NOT_FOUND" => Some(Self::MethodNotFound),
            "E_INVALID_PARAMS" => Some(Self::InvalidParams),
            "E_PARSE_ERROR" => Some(Self::ParseError),
            "E_REPLACEMENT" => Some(Self::Replacement),
            "E_NOT_FOUND" => Some(Self::NotFound),
            "E_INTERNAL" => Some(Self::Internal),
            _ => None,
        }
    }

    /// Convert to the sidecar error kind string (E_* format).
    pub fn to_sidecar(&self) -> &'static str {
        match self {
            Self::MicPermission => "E_MIC_PERMISSION",
            Self::DeviceNotFound => "E_DEVICE_NOT_FOUND",
            Self::AudioIO => "E_AUDIO_IO",
            Self::DeviceUnavailable => "E_DEVICE_UNAVAILABLE",
            Self::AlreadyRecording => "E_ALREADY_RECORDING",
            Self::NotRecording => "E_NOT_RECORDING",
            Self::InvalidSession => "E_INVALID_SESSION",
            Self::Recording => "E_RECORDING",
            Self::MeterRunning => "E_METER_RUNNING",
            Self::MeterNotRunning => "E_METER_NOT_RUNNING",
            Self::Meter => "E_METER",
            Self::ModelNotFound => "E_MODEL_NOT_FOUND",
            Self::ModelLoad => "E_MODEL_LOAD",
            Self::NotInitialized => "E_NOT_INITIALIZED",
            Self::NotReady => "E_NOT_READY",
            Self::Model => "E_MODEL",
            Self::Asr => "E_ASR",
            Self::DiskFull => "E_DISK_FULL",
            Self::CacheCorrupt => "E_CACHE_CORRUPT",
            Self::Lock => "E_LOCK",
            Self::Network => "E_NETWORK",
            Self::Transcription => "E_TRANSCRIPTION",
            Self::MethodNotFound => "E_METHOD_NOT_FOUND",
            Self::InvalidParams => "E_INVALID_PARAMS",
            Self::ParseError => "E_PARSE_ERROR",
            Self::Replacement => "E_REPLACEMENT",
            Self::NotFound => "E_NOT_FOUND",
            Self::Internal => "E_INTERNAL",
        }
    }

    /// Check if this error kind is recoverable (user can retry).
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::Network
                | Self::DeviceNotFound
                | Self::AudioIO
                | Self::CacheCorrupt
                | Self::AlreadyRecording
                | Self::NotRecording
                | Self::Transcription
        )
    }

    /// Check if this error kind requires user action (permissions, settings).
    pub fn requires_user_action(&self) -> bool {
        matches!(self, Self::MicPermission | Self::DiskFull)
    }

    /// Check if this error kind is internal (should be logged, not shown to user).
    pub fn is_internal(&self) -> bool {
        matches!(
            self,
            Self::Internal
                | Self::ParseError
                | Self::MethodNotFound
                | Self::InvalidParams
                | Self::Lock
        )
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_sidecar())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_sidecar_roundtrip() {
        // All variants should roundtrip through from_sidecar/to_sidecar
        let variants = vec![
            ErrorKind::MicPermission,
            ErrorKind::DeviceNotFound,
            ErrorKind::AudioIO,
            ErrorKind::DeviceUnavailable,
            ErrorKind::AlreadyRecording,
            ErrorKind::NotRecording,
            ErrorKind::InvalidSession,
            ErrorKind::Recording,
            ErrorKind::MeterRunning,
            ErrorKind::MeterNotRunning,
            ErrorKind::Meter,
            ErrorKind::ModelNotFound,
            ErrorKind::ModelLoad,
            ErrorKind::NotInitialized,
            ErrorKind::NotReady,
            ErrorKind::Model,
            ErrorKind::Asr,
            ErrorKind::DiskFull,
            ErrorKind::CacheCorrupt,
            ErrorKind::Lock,
            ErrorKind::Network,
            ErrorKind::Transcription,
            ErrorKind::MethodNotFound,
            ErrorKind::InvalidParams,
            ErrorKind::ParseError,
            ErrorKind::Replacement,
            ErrorKind::NotFound,
            ErrorKind::Internal,
        ];

        for variant in variants {
            let sidecar_str = variant.to_sidecar();
            let parsed = ErrorKind::from_sidecar(sidecar_str);
            assert_eq!(
                parsed,
                Some(variant),
                "Roundtrip failed for {:?} -> {} -> {:?}",
                variant,
                sidecar_str,
                parsed
            );
        }
    }

    #[test]
    fn test_unknown_sidecar_kind() {
        assert_eq!(ErrorKind::from_sidecar("E_UNKNOWN"), None);
        assert_eq!(ErrorKind::from_sidecar("not_an_error"), None);
    }

    #[test]
    fn test_serialization() {
        let kind = ErrorKind::MicPermission;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"mic_permission\"");

        let parsed: ErrorKind = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, kind);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", ErrorKind::MicPermission), "E_MIC_PERMISSION");
        assert_eq!(format!("{}", ErrorKind::Internal), "E_INTERNAL");
    }

    #[test]
    fn test_is_recoverable() {
        assert!(ErrorKind::Network.is_recoverable());
        assert!(ErrorKind::DeviceNotFound.is_recoverable());
        assert!(!ErrorKind::MicPermission.is_recoverable());
        assert!(!ErrorKind::DiskFull.is_recoverable());
    }

    #[test]
    fn test_requires_user_action() {
        assert!(ErrorKind::MicPermission.requires_user_action());
        assert!(ErrorKind::DiskFull.requires_user_action());
        assert!(!ErrorKind::Network.requires_user_action());
    }

    #[test]
    fn test_is_internal() {
        assert!(ErrorKind::Internal.is_internal());
        assert!(ErrorKind::ParseError.is_internal());
        assert!(!ErrorKind::MicPermission.is_internal());
        assert!(!ErrorKind::Network.is_internal());
    }
}
