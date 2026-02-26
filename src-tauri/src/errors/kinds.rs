//! Canonical app error code catalog.
//!
//! `ErrorKind` provides stable `E_*` identifiers used by the backend/frontend
//! contract. Legacy sidecar-specific codes are accepted as aliases by
//! [`ErrorKind::from_sidecar`] so older sidecars continue to interoperate.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const E_SIDECAR_SPAWN: &str = "E_SIDECAR_SPAWN";
pub const E_SIDECAR_IPC: &str = "E_SIDECAR_IPC";
pub const E_SIDECAR_CRASH: &str = "E_SIDECAR_CRASH";
pub const E_SIDECAR_CIRCUIT_BREAKER: &str = "E_SIDECAR_CIRCUIT_BREAKER";
pub const E_MIC_PERMISSION: &str = "E_MIC_PERMISSION";
pub const E_DEVICE_REMOVED: &str = "E_DEVICE_REMOVED";
pub const E_NO_AUDIO_DEVICE: &str = "E_NO_AUDIO_DEVICE";
pub const E_RECORDING_FAILED: &str = "E_RECORDING_FAILED";
pub const E_TRANSCRIPTION_FAILED: &str = "E_TRANSCRIPTION_FAILED";
pub const E_TRANSCRIPTION_TIMEOUT: &str = "E_TRANSCRIPTION_TIMEOUT";
pub const E_MODEL_NOT_READY: &str = "E_MODEL_NOT_READY";
pub const E_MODEL_DOWNLOAD: &str = "E_MODEL_DOWNLOAD";
pub const E_DISK_FULL: &str = "E_DISK_FULL";
pub const E_CACHE_CORRUPT: &str = "E_CACHE_CORRUPT";
pub const E_NETWORK: &str = "E_NETWORK";
pub const E_INJECTION_FAILED: &str = "E_INJECTION_FAILED";
pub const E_OVERLAY_FAILED: &str = "E_OVERLAY_FAILED";
pub const E_METHOD_NOT_FOUND: &str = "E_METHOD_NOT_FOUND";
pub const E_LANGUAGE_UNSUPPORTED: &str = "E_LANGUAGE_UNSUPPORTED";
pub const E_INTERNAL: &str = "E_INTERNAL";

/// Stable list of app error codes.
#[allow(dead_code)]
pub const ALL_ERROR_CODES: [&str; 20] = [
    E_SIDECAR_SPAWN,
    E_SIDECAR_IPC,
    E_SIDECAR_CRASH,
    E_SIDECAR_CIRCUIT_BREAKER,
    E_MIC_PERMISSION,
    E_DEVICE_REMOVED,
    E_NO_AUDIO_DEVICE,
    E_RECORDING_FAILED,
    E_TRANSCRIPTION_FAILED,
    E_TRANSCRIPTION_TIMEOUT,
    E_MODEL_NOT_READY,
    E_MODEL_DOWNLOAD,
    E_DISK_FULL,
    E_CACHE_CORRUPT,
    E_NETWORK,
    E_INJECTION_FAILED,
    E_OVERLAY_FAILED,
    E_METHOD_NOT_FOUND,
    E_LANGUAGE_UNSUPPORTED,
    E_INTERNAL,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    SidecarSpawn,
    SidecarIpc,
    SidecarCrash,
    SidecarCircuitBreaker,
    MicPermission,
    DeviceRemoved,
    NoAudioDevice,
    RecordingFailed,
    TranscriptionFailed,
    TranscriptionTimeout,
    ModelNotReady,
    ModelDownload,
    DiskFull,
    CacheCorrupt,
    Network,
    InjectionFailed,
    OverlayFailed,
    MethodNotFound,
    LanguageUnsupported,
    Internal,
}

impl ErrorKind {
    /// Parse canonical app error codes and legacy sidecar aliases.
    pub fn from_sidecar(kind: &str) -> Option<Self> {
        match kind {
            // Canonical catalog
            E_SIDECAR_SPAWN => Some(Self::SidecarSpawn),
            E_SIDECAR_IPC => Some(Self::SidecarIpc),
            E_SIDECAR_CRASH => Some(Self::SidecarCrash),
            E_SIDECAR_CIRCUIT_BREAKER => Some(Self::SidecarCircuitBreaker),
            E_MIC_PERMISSION => Some(Self::MicPermission),
            E_DEVICE_REMOVED => Some(Self::DeviceRemoved),
            E_NO_AUDIO_DEVICE => Some(Self::NoAudioDevice),
            E_RECORDING_FAILED => Some(Self::RecordingFailed),
            E_TRANSCRIPTION_FAILED => Some(Self::TranscriptionFailed),
            E_TRANSCRIPTION_TIMEOUT => Some(Self::TranscriptionTimeout),
            E_MODEL_NOT_READY => Some(Self::ModelNotReady),
            E_MODEL_DOWNLOAD => Some(Self::ModelDownload),
            E_DISK_FULL => Some(Self::DiskFull),
            E_CACHE_CORRUPT => Some(Self::CacheCorrupt),
            E_NETWORK => Some(Self::Network),
            E_INJECTION_FAILED => Some(Self::InjectionFailed),
            E_OVERLAY_FAILED => Some(Self::OverlayFailed),
            E_METHOD_NOT_FOUND => Some(Self::MethodNotFound),
            E_LANGUAGE_UNSUPPORTED => Some(Self::LanguageUnsupported),
            E_INTERNAL => Some(Self::Internal),

            // Backward-compatible aliases from older sidecars
            "E_SIDECAR_RESTARTING" => Some(Self::SidecarCrash),
            "E_SIDECAR_FAILED" => Some(Self::SidecarCircuitBreaker),
            "E_DEVICE_NOT_FOUND" | "E_DEVICE_UNAVAILABLE" => Some(Self::NoAudioDevice),
            "E_AUDIO_IO"
            | "E_RECORDING"
            | "E_ALREADY_RECORDING"
            | "E_NOT_RECORDING"
            | "E_INVALID_SESSION" => Some(Self::RecordingFailed),
            "E_MODEL_NOT_FOUND" | "E_MODEL_LOAD" | "E_NOT_INITIALIZED" | "E_NOT_READY"
            | "E_MODEL" => Some(Self::ModelNotReady),
            "E_TRANSCRIPTION" | "E_TRANSCRIBE" | "E_ASR" => Some(Self::TranscriptionFailed),
            "E_INVALID_PARAMS"
            | "E_PARSE_ERROR"
            | "E_METER"
            | "E_METER_RUNNING"
            | "E_METER_NOT_RUNNING"
            | "E_LOCK" => Some(Self::SidecarIpc),
            _ => None,
        }
    }

    /// Convert to canonical app error code.
    pub fn to_sidecar(self) -> &'static str {
        match self {
            Self::SidecarSpawn => E_SIDECAR_SPAWN,
            Self::SidecarIpc => E_SIDECAR_IPC,
            Self::SidecarCrash => E_SIDECAR_CRASH,
            Self::SidecarCircuitBreaker => E_SIDECAR_CIRCUIT_BREAKER,
            Self::MicPermission => E_MIC_PERMISSION,
            Self::DeviceRemoved => E_DEVICE_REMOVED,
            Self::NoAudioDevice => E_NO_AUDIO_DEVICE,
            Self::RecordingFailed => E_RECORDING_FAILED,
            Self::TranscriptionFailed => E_TRANSCRIPTION_FAILED,
            Self::TranscriptionTimeout => E_TRANSCRIPTION_TIMEOUT,
            Self::ModelNotReady => E_MODEL_NOT_READY,
            Self::ModelDownload => E_MODEL_DOWNLOAD,
            Self::DiskFull => E_DISK_FULL,
            Self::CacheCorrupt => E_CACHE_CORRUPT,
            Self::Network => E_NETWORK,
            Self::InjectionFailed => E_INJECTION_FAILED,
            Self::OverlayFailed => E_OVERLAY_FAILED,
            Self::MethodNotFound => E_METHOD_NOT_FOUND,
            Self::LanguageUnsupported => E_LANGUAGE_UNSUPPORTED,
            Self::Internal => E_INTERNAL,
        }
    }

    /// Whether user can usually recover with retry/degraded operation.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::SidecarIpc
                | Self::SidecarCrash
                | Self::DeviceRemoved
                | Self::NoAudioDevice
                | Self::RecordingFailed
                | Self::TranscriptionFailed
                | Self::TranscriptionTimeout
                | Self::ModelNotReady
                | Self::ModelDownload
                | Self::CacheCorrupt
                | Self::Network
                | Self::InjectionFailed
                | Self::OverlayFailed
                | Self::MethodNotFound
                | Self::LanguageUnsupported
        )
    }

    /// Whether user likely needs to change settings/environment.
    pub fn requires_user_action(&self) -> bool {
        matches!(
            self,
            Self::MicPermission
                | Self::NoAudioDevice
                | Self::SidecarSpawn
                | Self::SidecarCircuitBreaker
                | Self::DiskFull
                | Self::CacheCorrupt
        )
    }

    /// Whether this should mainly be treated as internal diagnostics.
    pub fn is_internal(&self) -> bool {
        matches!(
            self,
            Self::Internal | Self::SidecarIpc | Self::SidecarSpawn | Self::SidecarCircuitBreaker
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
        let variants = vec![
            ErrorKind::SidecarSpawn,
            ErrorKind::SidecarIpc,
            ErrorKind::SidecarCrash,
            ErrorKind::SidecarCircuitBreaker,
            ErrorKind::MicPermission,
            ErrorKind::DeviceRemoved,
            ErrorKind::NoAudioDevice,
            ErrorKind::RecordingFailed,
            ErrorKind::TranscriptionFailed,
            ErrorKind::TranscriptionTimeout,
            ErrorKind::ModelNotReady,
            ErrorKind::ModelDownload,
            ErrorKind::DiskFull,
            ErrorKind::CacheCorrupt,
            ErrorKind::Network,
            ErrorKind::InjectionFailed,
            ErrorKind::OverlayFailed,
            ErrorKind::MethodNotFound,
            ErrorKind::LanguageUnsupported,
            ErrorKind::Internal,
        ];

        for variant in variants {
            let code = variant.to_sidecar();
            let parsed = ErrorKind::from_sidecar(code);
            assert_eq!(parsed, Some(variant));
        }
    }

    #[test]
    fn test_legacy_aliases_remain_supported() {
        assert_eq!(
            ErrorKind::from_sidecar("E_TRANSCRIPTION"),
            Some(ErrorKind::TranscriptionFailed)
        );
        assert_eq!(
            ErrorKind::from_sidecar("E_AUDIO_IO"),
            Some(ErrorKind::RecordingFailed)
        );
        assert_eq!(
            ErrorKind::from_sidecar("E_DEVICE_NOT_FOUND"),
            Some(ErrorKind::NoAudioDevice)
        );
        assert_eq!(
            ErrorKind::from_sidecar("E_MODEL_LOAD"),
            Some(ErrorKind::ModelNotReady)
        );
        assert_eq!(
            ErrorKind::from_sidecar("E_SIDECAR_FAILED"),
            Some(ErrorKind::SidecarCircuitBreaker)
        );
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
        assert_eq!(format!("{}", ErrorKind::MicPermission), E_MIC_PERMISSION);
        assert_eq!(format!("{}", ErrorKind::Internal), E_INTERNAL);
    }

    #[test]
    fn test_is_recoverable() {
        assert!(ErrorKind::Network.is_recoverable());
        assert!(ErrorKind::NoAudioDevice.is_recoverable());
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
        assert!(ErrorKind::SidecarIpc.is_internal());
        assert!(!ErrorKind::MicPermission.is_internal());
        assert!(!ErrorKind::Network.is_internal());
    }

    #[test]
    fn test_all_error_codes_list_is_unique() {
        let mut unique = std::collections::HashSet::new();
        for code in ALL_ERROR_CODES {
            assert!(unique.insert(code), "duplicate error code: {}", code);
            assert!(code.starts_with("E_"));
        }
    }
}
