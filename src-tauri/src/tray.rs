//! System tray icon and menu implementation.
//!
//! This module provides:
//! - System tray icon that changes based on app state
//! - Context menu with app controls
//! - Tooltip showing current status

use std::sync::Arc;

use tauri::menu::{MenuBuilder, MenuEvent, MenuId, MenuItem, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{image::Image, AppHandle, Emitter, Manager};
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
}

/// Tray icon file paths (embedded at compile time)
const ICON_IDLE: &[u8] = include_bytes!("../icons/tray-idle.png");
const ICON_RECORDING: &[u8] = include_bytes!("../icons/tray-recording.png");
const ICON_TRANSCRIBING: &[u8] = include_bytes!("../icons/tray-transcribing.png");
const ICON_LOADING: &[u8] = include_bytes!("../icons/tray-loading.png");
const ICON_ERROR: &[u8] = include_bytes!("../icons/tray-error.png");
const ICON_DISABLED: &[u8] = include_bytes!("../icons/tray-disabled.png");

fn sidecar_restart_event_payload(reason: &str) -> serde_json::Value {
    crate::event_seq::payload_with_next_seq(serde_json::json!({ "reason": reason }))
}

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

/// Get the Enable/Disable toggle menu text.
fn get_toggle_enabled_text(enabled: bool) -> &'static str {
    if enabled {
        "Disable"
    } else {
        "Enable"
    }
}

type TrayMenuItem = MenuItem<tauri::Wry>;

struct TrayMenuItems {
    status: TrayMenuItem,
    toggle_enabled: TrayMenuItem,
    copy_last: TrayMenuItem,
}

fn should_enable_copy_last(history: &TranscriptHistory) -> bool {
    !history.is_empty()
}

/// Load a PNG icon from bytes into a Tauri Image.
fn load_png_icon(bytes: &[u8]) -> Result<Image<'static>, String> {
    // Decode PNG to RGBA bytes
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("PNG decode error: {}", e))?;

    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG frame error: {}", e))?;

    // Handle different color types
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            // Convert RGB to RGBA
            let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
            for chunk in buf[..info.buffer_size()].chunks(3) {
                rgba.extend_from_slice(chunk);
                rgba.push(255); // Alpha
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            // Convert Grayscale+Alpha to RGBA
            let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
            for chunk in buf[..info.buffer_size()].chunks(2) {
                rgba.push(chunk[0]); // R
                rgba.push(chunk[0]); // G
                rgba.push(chunk[0]); // B
                rgba.push(chunk[1]); // A
            }
            rgba
        }
        png::ColorType::Grayscale => {
            // Convert Grayscale to RGBA
            let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
            for &pixel in &buf[..info.buffer_size()] {
                rgba.push(pixel); // R
                rgba.push(pixel); // G
                rgba.push(pixel); // B
                rgba.push(255); // A
            }
            rgba
        }
        png::ColorType::Indexed => {
            return Err("Indexed PNG not supported".to_string());
        }
    };

    Ok(Image::new_owned(rgba, info.width, info.height))
}

/// Create and set up the system tray.
fn setup_tray(app: &AppHandle) -> Result<(TrayIcon, TrayMenuItems), tauri::Error> {
    let history = app.state::<TranscriptHistory>();
    let copy_last_enabled = should_enable_copy_last(&history);

    let header_item = MenuItemBuilder::with_id(MenuId::new("header"), "OpenVoicy")
        .enabled(false)
        .build(app)?;
    let show_settings_item =
        MenuItemBuilder::with_id(menu_ids::SHOW_SETTINGS, "Show Settings").build(app)?;
    let status_item = MenuItemBuilder::with_id(menu_ids::STATUS, get_status_text(AppState::Idle, true))
        .enabled(false)
        .build(app)?;
    let toggle_enabled_item =
        MenuItemBuilder::with_id(menu_ids::TOGGLE_ENABLED, get_toggle_enabled_text(true))
            .build(app)?;
    let copy_last_item = MenuItemBuilder::with_id(menu_ids::COPY_LAST, "Copy Last Transcript")
        .enabled(copy_last_enabled)
        .build(app)?;
    let restart_sidecar_item =
        MenuItemBuilder::with_id(menu_ids::RESTART_SIDECAR, "Restart Sidecar").build(app)?;

    // Build the menu
    let menu = MenuBuilder::new(app)
        // Header (app name)
        .item(&header_item)
        // Show Settings
        .item(&show_settings_item)
        .separator()
        // Status (dynamic, non-clickable)
        .item(&status_item)
        // Enable/Disable toggle
        .item(&toggle_enabled_item)
        .separator()
        // Copy Last Transcript
        .item(&copy_last_item)
        // Restart Sidecar
        .item(&restart_sidecar_item)
        .separator()
        // Quit
        .item(&PredefinedMenuItem::quit(app, Some("Quit"))?)
        .build()?;

    // Load the initial icon
    let icon = load_png_icon(ICON_IDLE)
        .map_err(|e| tauri::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;

    // Build the tray icon
    let tray = TrayIconBuilder::new()
        .icon(icon)
        .tooltip("OpenVoicy - Ready")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(handle_tray_event)
        .build(app)?;

    Ok((
        tray,
        TrayMenuItems {
            status: status_item,
            toggle_enabled: toggle_enabled_item,
            copy_last: copy_last_item,
        },
    ))
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
            let currently_enabled = state_manager.is_enabled();
            state_manager.set_enabled(!currently_enabled);

            let now_enabled = state_manager.is_enabled();
            log::info!("Enabled toggled to: {}", now_enabled);
        }
        menu_ids::COPY_LAST => {
            log::info!("Tray: Copy Last Transcript clicked");
            let history = app.state::<TranscriptHistory>();
            if let Some(entry) = history.last() {
                use crate::injection;
                let _ = injection::set_clipboard_public(&entry.text);
                log::info!("Copied last transcript to clipboard");
            } else {
                log::info!("No transcript to copy");
            }
        }
        menu_ids::RESTART_SIDECAR => {
            log::info!("Tray: Restart Sidecar clicked");
            // Emit event to trigger sidecar restart
            let _ = app.emit(
                "sidecar:restart",
                sidecar_restart_event_payload("user_request"),
            );
        }
        _ => {
            log::debug!("Tray: Unhandled menu event: {}", id);
        }
    }
}

/// Handle tray icon events (clicks, etc).
fn handle_tray_event(tray: &TrayIcon, event: TrayIconEvent) {
    match event {
        TrayIconEvent::DoubleClick { .. } => {
            log::debug!("Tray icon double-clicked");
            // Show main window on double-click
            let app = tray.app_handle();
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        _ => {}
    }
}

/// Tray manager for updating tray state.
pub struct TrayManager {
    tray: Option<TrayIcon>,
    app_handle: AppHandle,
    status_menu_item: Option<TrayMenuItem>,
    toggle_enabled_menu_item: Option<TrayMenuItem>,
    copy_last_menu_item: Option<TrayMenuItem>,
}

impl TrayManager {
    /// Create a new tray manager.
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            tray: None,
            app_handle,
            status_menu_item: None,
            toggle_enabled_menu_item: None,
            copy_last_menu_item: None,
        }
    }

    /// Initialize the tray icon.
    pub fn init(&mut self) -> Result<(), String> {
        let (tray, menu_items) = setup_tray(&self.app_handle).map_err(|e| e.to_string())?;
        self.tray = Some(tray);
        self.status_menu_item = Some(menu_items.status);
        self.toggle_enabled_menu_item = Some(menu_items.toggle_enabled);
        self.copy_last_menu_item = Some(menu_items.copy_last);
        log::info!("Tray icon initialized");
        Ok(())
    }

    /// Update the tray icon and tooltip based on app state.
    pub fn update_state(&self, state: AppState, enabled: bool) -> Result<(), String> {
        let tray = self
            .tray
            .as_ref()
            .ok_or_else(|| "Tray not initialized".to_string())?;

        // Update icon
        let icon_bytes = get_icon_for_state(state, enabled);
        let icon = load_png_icon(icon_bytes)?;
        tray.set_icon(Some(icon)).map_err(|e| e.to_string())?;

        // Update tooltip
        let tooltip = get_tooltip_text(state, enabled);
        tray.set_tooltip(Some(tooltip)).map_err(|e| e.to_string())?;

        // Update status and toggle labels to match current state.
        let status_item = self
            .status_menu_item
            .as_ref()
            .ok_or_else(|| "Tray status menu item not initialized".to_string())?;
        status_item
            .set_text(get_status_text(state, enabled))
            .map_err(|e| e.to_string())?;

        let toggle_item = self
            .toggle_enabled_menu_item
            .as_ref()
            .ok_or_else(|| "Tray toggle menu item not initialized".to_string())?;
        toggle_item
            .set_text(get_toggle_enabled_text(enabled))
            .map_err(|e| e.to_string())?;

        let history = self.app_handle.state::<TranscriptHistory>();
        let copy_last_item = self
            .copy_last_menu_item
            .as_ref()
            .ok_or_else(|| "Tray copy-last menu item not initialized".to_string())?;
        copy_last_item
            .set_enabled(should_enable_copy_last(&history))
            .map_err(|e| e.to_string())?;

        Ok(())
    }
}

/// Start the tray state update loop.
pub fn start_tray_loop(
    _app_handle: AppHandle,
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

    fn png_dimensions(bytes: &[u8]) -> (u32, u32) {
        assert!(bytes.len() >= 24, "PNG data too short");
        assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n", "Invalid PNG signature");

        let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        (width, height)
    }

    #[test]
    fn test_get_icon_for_state_disabled() {
        let icon = get_icon_for_state(AppState::Idle, false);
        // Compare actual byte content instead of pointers (pointers can differ in test builds)
        assert_eq!(icon, ICON_DISABLED);
    }

    #[test]
    fn test_get_icon_for_state_enabled() {
        let icon = get_icon_for_state(AppState::Idle, true);
        assert_eq!(icon, ICON_IDLE);

        let icon = get_icon_for_state(AppState::Recording, true);
        assert_eq!(icon, ICON_RECORDING);

        let icon = get_icon_for_state(AppState::Transcribing, true);
        assert_eq!(icon, ICON_TRANSCRIBING);

        let icon = get_icon_for_state(AppState::LoadingModel, true);
        assert_eq!(icon, ICON_LOADING);

        let icon = get_icon_for_state(AppState::Error, true);
        assert_eq!(icon, ICON_ERROR);
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
    fn test_get_toggle_enabled_text() {
        assert_eq!(get_toggle_enabled_text(true), "Disable");
        assert_eq!(get_toggle_enabled_text(false), "Enable");
    }

    #[test]
    fn test_should_enable_copy_last_tracks_history_emptiness() {
        let history = TranscriptHistory::new();
        assert!(!should_enable_copy_last(&history));

        history.push(crate::history::TranscriptEntry::new(
            "hello".to_string(),
            100,
            50,
            crate::history::HistoryInjectionResult::Injected,
        ));
        assert!(should_enable_copy_last(&history));

        history.clear();
        assert!(!should_enable_copy_last(&history));
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

    #[test]
    fn test_bundle_app_icons_present_and_non_empty() {
        let ico = include_bytes!("../icons/icon.ico");
        let icns = include_bytes!("../icons/icon.icns");

        assert!(!ico.is_empty(), "icons/icon.ico must not be empty");
        assert!(!icns.is_empty(), "icons/icon.icns must not be empty");

        // ICO signature: reserved=0, type=1 (icon)
        assert_eq!(&ico[0..4], &[0x00, 0x00, 0x01, 0x00]);
        // ICNS signature: file magic
        assert_eq!(&icns[0..4], b"icns");
    }

    #[test]
    fn test_bundle_app_png_icon_dimensions() {
        let icon_128 = include_bytes!("../icons/128x128.png");
        let icon_128_2x = include_bytes!("../icons/128x128@2x.png");

        assert_eq!(png_dimensions(icon_128), (128, 128));
        assert_eq!(png_dimensions(icon_128_2x), (256, 256));
    }

    #[test]
    fn test_load_png_icon() {
        // Test that we can load the embedded icons
        let result = load_png_icon(ICON_IDLE);
        assert!(
            result.is_ok(),
            "Failed to load idle icon: {:?}",
            result.err()
        );

        let result = load_png_icon(ICON_RECORDING);
        assert!(
            result.is_ok(),
            "Failed to load recording icon: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_sidecar_restart_event_payload_includes_seq() {
        let first = sidecar_restart_event_payload("user_request");
        let second = sidecar_restart_event_payload("user_request");

        assert_eq!(first["reason"], "user_request");
        assert!(first["seq"].is_u64());
        assert!(second["seq"].is_u64());

        let first_seq = first["seq"].as_u64().unwrap();
        let second_seq = second["seq"].as_u64().unwrap();
        assert!(second_seq > first_seq);
    }
}
