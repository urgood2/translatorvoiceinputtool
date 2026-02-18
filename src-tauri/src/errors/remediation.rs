//! Remediation types and actions for error recovery.
//!
//! This module defines the possible actions a user can take to recover
//! from an error. The UI layer uses these to render appropriate buttons
//! and handle user interactions.

use serde::{Deserialize, Serialize};

/// Settings pages that can be opened for remediation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsPage {
    /// General application settings.
    General,
    /// Hotkey configuration.
    Hotkey,
    /// Audio device selection.
    AudioDevice,
    /// Microphone permission (OS settings).
    MicrophonePermission,
    /// macOS Accessibility permission.
    MacOSAccessibility,
    /// macOS Security & Privacy settings.
    MacOSSecurity,
}

impl SettingsPage {
    /// Get the OS-specific deep link URL for this settings page.
    ///
    /// Returns `None` if no deep link is available for the current platform.
    pub fn deep_link(&self) -> Option<&'static str> {
        match self {
            // macOS deep links
            #[cfg(target_os = "macos")]
            Self::MicrophonePermission => {
                Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
            }
            #[cfg(target_os = "macos")]
            Self::MacOSAccessibility => Some(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            ),
            #[cfg(target_os = "macos")]
            Self::MacOSSecurity => {
                Some("x-apple.systempreferences:com.apple.preference.security?General")
            }

            // Windows deep links
            #[cfg(target_os = "windows")]
            Self::MicrophonePermission => Some("ms-settings:privacy-microphone"),

            // In-app settings (no deep link needed)
            Self::General | Self::Hotkey | Self::AudioDevice => None,

            // Platform-specific settings on other platforms
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            _ => None,
        }
    }

    /// Get a human-readable description of how to access this setting.
    pub fn instructions(&self) -> &'static str {
        match self {
            Self::General => "Open Voice Input Tool Settings",
            Self::Hotkey => "Open Voice Input Tool Settings → Hotkey",
            Self::AudioDevice => "Open Voice Input Tool Settings → Audio",
            #[cfg(target_os = "macos")]
            Self::MicrophonePermission => {
                "Open System Preferences → Security & Privacy → Privacy → Microphone"
            }
            #[cfg(target_os = "macos")]
            Self::MacOSAccessibility => {
                "Open System Preferences → Security & Privacy → Privacy → Accessibility"
            }
            #[cfg(target_os = "macos")]
            Self::MacOSSecurity => "Open System Preferences → Security & Privacy → General",
            #[cfg(target_os = "windows")]
            Self::MicrophonePermission => "Open Settings → Privacy → Microphone",
            #[cfg(target_os = "linux")]
            Self::MicrophonePermission => "Check your desktop environment's privacy settings",
            #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
            Self::MicrophonePermission => "Check your system's privacy settings",
            #[cfg(not(target_os = "macos"))]
            Self::MacOSAccessibility | Self::MacOSSecurity => "N/A on this platform",
        }
    }
}

/// Suggested remediation action for an error.
///
/// The UI layer should render appropriate controls based on this enum:
/// - `OpenSettings`: Open a settings panel (in-app or system)
/// - `OpenUrl`: Open a URL in the browser
/// - `Retry`: Show a "Try Again" button
/// - `RestartSidecar`: Restart the background service
/// - `RestartApp`: Restart the entire application
/// - `Reinstall`: Prompt user to reinstall
/// - `None`: No specific action available
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum Remediation {
    /// Open a settings page (in-app or system).
    OpenSettings(SettingsPage),
    /// Open a URL in the default browser.
    OpenUrl(String),
    /// Retry the failed operation.
    Retry,
    /// Restart the sidecar/background service.
    RestartSidecar,
    /// Restart the entire application.
    RestartApp,
    /// Suggest reinstallation.
    Reinstall,
}

impl Remediation {
    /// Get a human-readable label for the action button.
    pub fn button_label(&self) -> &str {
        match self {
            Self::OpenSettings(_) => "Open Settings",
            Self::OpenUrl(_) => "Learn More",
            Self::Retry => "Try Again",
            Self::RestartSidecar => "Restart Service",
            Self::RestartApp => "Restart App",
            Self::Reinstall => "Get Help",
        }
    }

    /// Check if this remediation requires user interaction.
    ///
    /// `Retry` and `RestartSidecar` can be automated in some cases.
    pub fn requires_user_interaction(&self) -> bool {
        matches!(
            self,
            Self::OpenSettings(_) | Self::OpenUrl(_) | Self::RestartApp | Self::Reinstall
        )
    }

    /// Check if this remediation can be automatically attempted.
    pub fn can_auto_retry(&self) -> bool {
        matches!(self, Self::Retry | Self::RestartSidecar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_page_instructions_not_empty() {
        let pages = vec![
            SettingsPage::General,
            SettingsPage::Hotkey,
            SettingsPage::AudioDevice,
            SettingsPage::MicrophonePermission,
            SettingsPage::MacOSAccessibility,
            SettingsPage::MacOSSecurity,
        ];

        for page in pages {
            let instructions = page.instructions();
            assert!(
                !instructions.is_empty(),
                "Empty instructions for {:?}",
                page
            );
        }
    }

    #[test]
    fn test_remediation_button_labels() {
        let remediations = vec![
            Remediation::OpenSettings(SettingsPage::General),
            Remediation::OpenUrl("https://example.com".to_string()),
            Remediation::Retry,
            Remediation::RestartSidecar,
            Remediation::RestartApp,
            Remediation::Reinstall,
        ];

        for rem in remediations {
            let label = rem.button_label();
            assert!(!label.is_empty(), "Empty button label for {:?}", rem);
        }
    }

    #[test]
    fn test_remediation_serialization() {
        let rem = Remediation::OpenSettings(SettingsPage::Hotkey);
        let json = serde_json::to_string(&rem).unwrap();
        assert!(json.contains("open_settings"));
        assert!(json.contains("hotkey"));

        let rem = Remediation::Retry;
        let json = serde_json::to_string(&rem).unwrap();
        assert!(json.contains("retry"));
    }

    #[test]
    fn test_can_auto_retry() {
        assert!(Remediation::Retry.can_auto_retry());
        assert!(Remediation::RestartSidecar.can_auto_retry());
        assert!(!Remediation::OpenSettings(SettingsPage::General).can_auto_retry());
        assert!(!Remediation::RestartApp.can_auto_retry());
    }

    #[test]
    fn test_requires_user_interaction() {
        assert!(Remediation::OpenSettings(SettingsPage::General).requires_user_interaction());
        assert!(Remediation::OpenUrl("https://example.com".to_string()).requires_user_interaction());
        assert!(Remediation::RestartApp.requires_user_interaction());
        assert!(!Remediation::Retry.requires_user_interaction());
        assert!(!Remediation::RestartSidecar.requires_user_interaction());
    }
}
