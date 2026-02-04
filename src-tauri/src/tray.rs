//! System tray icon and menu implementation.
//!
//! This module provides:
//! - System tray icon that changes based on app state
//! - Context menu with app controls
//! - Tooltip showing current status

use std::sync::Arc;

use tauri::menu::{MenuBuilder, MenuEvent, MenuId, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{image::Image, AppHandle, Manager};
use tokio::sync::RwLock;

use crate::history::TranscriptHistory;
use crate::state::{AppState, AppStateManager};

/// Tray menu item IDs
mod menu_ids {
    pub const SHOW_SETTINGS: &str = "show_settings";
    pub const STATUS: &str = "status";
    pub const TOGGLE_ENABLED: &str = "toggle_enabled";
    pub const COPY_LAST: &str = "copy_last";
    pub const RESTART_SIDECAR: &str = "restart_sidecar";
    pub const QUIT: &str = "quit";
}

/// Tray icon file paths (embedded at compile time)
const ICON_IDLE: &[u8] = include_bytes!("../icons/tray-idle.png");
const ICON_RECORDING: &[u8] = include_bytes!("../icons/tray-recording.png");
const ICON_TRANSCRIBING: &[u8] = include_bytes!("../icons/tray-transcribing.png");
const ICON_LOADING: &[u8] = include_bytes!("../icons/tray-loading.png");
const ICON_ERROR: &[u8] = include_bytes!("../icons/tray-error.png");
const ICON_DISABLED: &[u8] = include_bytes!("../icons/tray-disabled.png");

/// Get the appropriate icon bytes for the given state.
fn get_icon_for_state(state: AppState, enabled: bool) -> &'static [u8] {
    if !enabled {
        return ICON_DISABLED;
    }

    match state {
        AppState::Idle => ICON_IDLE,
        AppState::Recording => ICON_RECORDING,
        AppState::Transcribing => ICON_TRANSCRIBING,
        AppState::LoadingModel => ICON_LOADING,
        AppState::Error => ICON_ERROR,
    }
}

/// Get the status text for the given state.
fn get_status_text(state: AppState, enabled: bool) -> &'static str {
    if !enabled {
        return "Status: Paused";
    }

    match state {
        AppState::Idle => "Status: Ready",
        AppState::Recording => "Status: Recording...",
        AppState::Transcribing => "Status: Transcribing...",
        AppState::LoadingModel => "Status: Loading model...",
        AppState::Error => "Status: Error",
    }
}

/// Get the tooltip text for the given state.
fn get_tooltip_text(state: AppState, enabled: bool) -> &'static str {
    if !enabled {
        return "OpenVoicy - Paused";
    }

    match state {
        AppState::Idle => "OpenVoicy - Ready",
        AppState::Recording => "OpenVoicy - Recording...",
        AppState::Transcribing => "OpenVoicy - Processing...",
        AppState::LoadingModel => "OpenVoicy - Loading model...",
        AppState::Error => "OpenVoicy - Error",
    }
}

/// Create and set up the system tray.
pub fn setup_tray(app: &AppHandle) -> Result<TrayIcon, tauri::Error> {
    // Build the menu
    let menu = MenuBuilder::new(app)
        // Header (app name)
        .item(
            &MenuItemBuilder::with_id(MenuId::new("header"), "OpenVoicy")
                .enabled(false)
                .build(app)?,
        )
        // Show Settings
        .item(
            &MenuItemBuilder::with_id(menu_ids::SHOW_SETTINGS, "Show Settings")
                .build(app)?,
        )
        .separator()
        // Status (dynamic, non-clickable)
        .item(
            &MenuItemBuilder::with_id(menu_ids::STATUS, "Status: Ready")
                .enabled(false)
                .build(app)?,
        )
        // Enable/Disable toggle
        .item(
            &MenuItemBuilder::with_id(menu_ids::TOGGLE_ENABLED, "Disable")
                .build(app)?,
        )
        .separator()
        // Copy Last Transcript
        .item(
            &MenuItemBuilder::with_id(menu_ids::COPY_LAST, "Copy Last Transcript")
                .build(app)?,
        )
        // Restart Sidecar
        .item(
            &MenuItemBuilder::with_id(menu_ids::RESTART_SIDECAR, "Restart Sidecar")
                .build(app)?,
        )
        .separator()
        // Quit
        .item(&PredefinedMenuItem::quit(app, Some("Quit"))?)
        .build()?;

    // Load the initial icon
    let icon = Image::from_bytes(ICON_IDLE)?;

    // Build the tray icon
    let tray = TrayIconBuilder::new()
        .icon(icon)
        .tooltip("OpenVoicy - Ready")
        .menu(&menu)
        .menu_on_left_click(true)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(handle_tray_event)
        .build(app)?;

    Ok(tray)
}

/// Handle menu item clicks.
fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();

    match id {
        menu_ids::SHOW_SETTINGS => {
            log::info!("Tray: Show Settings clicked");
            // Show the main window
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        menu_ids::TOGGLE_ENABLED => {
            log::info!("Tray: Toggle Enabled clicked");
            // Get state manager and toggle
            let state_manager = app.state::<Arc<AppStateManager>>();
            state_manager.toggle_enabled();

            // Update menu item text
            let enabled = state_manager.is_enabled();
            let toggle_text = if enabled { "Disable" } else { "Enable" };

            // Update the tray (done via events in production)
            let _ = tauri::async_runtime::spawn(async move {
                // This would be handled by the state loop
            });

            log::info!("Enabled toggled to: {}", enabled);
        }
        menu_ids::COPY_LAST => {
            log::info!("Tray: Copy Last Transcript clicked");
            let history = app.state::<TranscriptHistory>();
            if let Some(entry) = history.get_latest() {
                use crate::injection;
                let _ = injection::copy_to_clipboard(&entry.text);
                log::info!("Copied last transcript to clipboard");
            } else {
                log::info!("No transcript to copy");
            }
        }
        menu_ids::RESTART_SIDECAR => {
            log::info!("Tray: Restart Sidecar clicked");
            // Emit event to trigger sidecar restart
            let _ = app.emit("sidecar:restart", serde_json::json!({"reason": "user_request"}));
        }
        _ => {
            log::debug!("Tray: Unhandled menu event: {}", id);
        }
    }
}

/// Handle tray icon events (clicks, etc).
fn handle_tray_event(tray: &TrayIcon, event: TrayIconEvent) {
    match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } => {
            log::debug!("Tray icon left-clicked");
            // Menu is shown automatically with menu_on_left_click(true)
        }
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => {
            log::debug!("Tray icon double-clicked");
            // Show main window on double-click
            if let Some(app) = tray.app_handle() {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        }
        _ => {}
    }
}

/// Tray manager for updating tray state.
pub struct TrayManager {
    tray: Option<TrayIcon>,
    app_handle: AppHandle,
}

impl TrayManager {
    /// Create a new tray manager.
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            tray: None,
            app_handle,
        }
    }

    /// Initialize the tray icon.
    pub fn init(&mut self) -> Result<(), String> {
        let tray = setup_tray(&self.app_handle).map_err(|e| e.to_string())?;
        self.tray = Some(tray);
        log::info!("Tray icon initialized");
        Ok(())
    }

    /// Update the tray icon and menu based on app state.
    pub fn update_state(&self, state: AppState, enabled: bool) -> Result<(), String> {
        let tray = self
            .tray
            .as_ref()
            .ok_or_else(|| "Tray not initialized".to_string())?;

        // Update icon
        let icon_bytes = get_icon_for_state(state, enabled);
        let icon = Image::from_bytes(icon_bytes).map_err(|e| e.to_string())?;
        tray.set_icon(Some(icon)).map_err(|e| e.to_string())?;

        // Update tooltip
        let tooltip = get_tooltip_text(state, enabled);
        tray.set_tooltip(Some(tooltip)).map_err(|e| e.to_string())?;

        // Update menu items
        if let Some(menu) = tray.menu() {
            // Update status text
            if let Some(status_item) = menu.get(menu_ids::STATUS) {
                if let Some(menu_item) = status_item.as_menuitem() {
                    let status_text = get_status_text(state, enabled);
                    let _ = menu_item.set_text(status_text);
                }
            }

            // Update toggle text
            if let Some(toggle_item) = menu.get(menu_ids::TOGGLE_ENABLED) {
                if let Some(menu_item) = toggle_item.as_menuitem() {
                    let toggle_text = if enabled { "Disable" } else { "Enable" };
                    let _ = menu_item.set_text(toggle_text);
                }
            }

            // Update copy last enabled state (disabled if no history)
            if let Some(copy_item) = menu.get(menu_ids::COPY_LAST) {
                if let Some(menu_item) = copy_item.as_menuitem() {
                    let history = self.app_handle.state::<TranscriptHistory>();
                    let has_history = !history.is_empty();
                    let _ = menu_item.set_enabled(has_history);
                }
            }
        }

        Ok(())
    }
}

/// Start the tray state update loop.
pub fn start_tray_loop(
    app_handle: AppHandle,
    state_manager: Arc<AppStateManager>,
    tray_manager: Arc<RwLock<TrayManager>>,
) {
    tokio::spawn(async move {
        let mut receiver = state_manager.subscribe();

        log::info!("Tray update loop started");

        while let Ok(event) = receiver.recv().await {
            let tray = tray_manager.read().await;
            if let Err(e) = tray.update_state(event.state, event.enabled) {
                log::warn!("Failed to update tray: {}", e);
            }
        }

        log::info!("Tray update loop ended");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_icon_for_state_disabled() {
        let icon = get_icon_for_state(AppState::Idle, false);
        assert_eq!(icon.as_ptr(), ICON_DISABLED.as_ptr());
    }

    #[test]
    fn test_get_icon_for_state_enabled() {
        let icon = get_icon_for_state(AppState::Idle, true);
        assert_eq!(icon.as_ptr(), ICON_IDLE.as_ptr());

        let icon = get_icon_for_state(AppState::Recording, true);
        assert_eq!(icon.as_ptr(), ICON_RECORDING.as_ptr());

        let icon = get_icon_for_state(AppState::Transcribing, true);
        assert_eq!(icon.as_ptr(), ICON_TRANSCRIBING.as_ptr());

        let icon = get_icon_for_state(AppState::LoadingModel, true);
        assert_eq!(icon.as_ptr(), ICON_LOADING.as_ptr());

        let icon = get_icon_for_state(AppState::Error, true);
        assert_eq!(icon.as_ptr(), ICON_ERROR.as_ptr());
    }

    #[test]
    fn test_get_status_text_disabled() {
        let text = get_status_text(AppState::Idle, false);
        assert_eq!(text, "Status: Paused");
    }

    #[test]
    fn test_get_status_text_enabled() {
        assert_eq!(get_status_text(AppState::Idle, true), "Status: Ready");
        assert_eq!(
            get_status_text(AppState::Recording, true),
            "Status: Recording..."
        );
        assert_eq!(
            get_status_text(AppState::Transcribing, true),
            "Status: Transcribing..."
        );
        assert_eq!(
            get_status_text(AppState::LoadingModel, true),
            "Status: Loading model..."
        );
        assert_eq!(get_status_text(AppState::Error, true), "Status: Error");
    }

    #[test]
    fn test_get_tooltip_text_disabled() {
        let text = get_tooltip_text(AppState::Idle, false);
        assert_eq!(text, "OpenVoicy - Paused");
    }

    #[test]
    fn test_get_tooltip_text_enabled() {
        assert_eq!(get_tooltip_text(AppState::Idle, true), "OpenVoicy - Ready");
        assert_eq!(
            get_tooltip_text(AppState::Recording, true),
            "OpenVoicy - Recording..."
        );
        assert_eq!(
            get_tooltip_text(AppState::Transcribing, true),
            "OpenVoicy - Processing..."
        );
        assert_eq!(
            get_tooltip_text(AppState::LoadingModel, true),
            "OpenVoicy - Loading model..."
        );
        assert_eq!(get_tooltip_text(AppState::Error, true), "OpenVoicy - Error");
    }

    #[test]
    fn test_menu_ids_unique() {
        // Verify all menu IDs are unique
        let ids = [
            menu_ids::SHOW_SETTINGS,
            menu_ids::STATUS,
            menu_ids::TOGGLE_ENABLED,
            menu_ids::COPY_LAST,
            menu_ids::RESTART_SIDECAR,
            menu_ids::QUIT,
        ];

        let mut seen = std::collections::HashSet::new();
        for id in ids {
            assert!(seen.insert(id), "Duplicate menu ID: {}", id);
        }
    }

    #[test]
    fn test_icon_bytes_not_empty() {
        assert!(!ICON_IDLE.is_empty());
        assert!(!ICON_RECORDING.is_empty());
        assert!(!ICON_TRANSCRIBING.is_empty());
        assert!(!ICON_LOADING.is_empty());
        assert!(!ICON_ERROR.is_empty());
        assert!(!ICON_DISABLED.is_empty());
    }
}
