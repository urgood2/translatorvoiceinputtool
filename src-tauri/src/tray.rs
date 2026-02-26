//! System tray icon and menu implementation.
//!
//! This module provides:
//! - System tray icon that changes based on app state
//! - State-aware dynamic context menu
//! - Tooltip showing current status

use std::sync::Arc;

use tauri::menu::{
    CheckMenuItemBuilder, Menu, MenuEvent, MenuId, MenuItemBuilder, PredefinedMenuItem, Submenu,
};
use tauri::tray::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{image::Image, AppHandle, Emitter, Manager};
use tokio::sync::RwLock;

use crate::config::{self, HotkeyMode};
use crate::history::TranscriptHistory;
use crate::state::{AppState, AppStateManager};

/// Tray menu item IDs.
mod menu_ids {
    pub const HEADER: &str = "header";
    pub const TOGGLE_ENABLED: &str = "toggle_enabled";
    pub const TOGGLE_RECORDING: &str = "toggle_recording";
    pub const MODE_STATUS: &str = "mode_status";
    pub const LANGUAGE_STATUS: &str = "language_status";
    pub const MIC_SUBMENU: &str = "mic_submenu";
    pub const RECENT_SUBMENU: &str = "recent_submenu";
    pub const TOGGLE_OVERLAY: &str = "toggle_overlay";
    pub const MODEL_STATUS: &str = "model_status";
    pub const SIDECAR_STATUS: &str = "sidecar_status";
    pub const TOGGLE_WINDOW: &str = "toggle_window";

    pub const SELECT_MIC_PREFIX: &str = "select_mic::";
    pub const COPY_RECENT_PREFIX: &str = "copy_recent::";
}

/// Tray icon file paths (embedded at compile time).
const ICON_IDLE: &[u8] = include_bytes!("../icons/tray-idle.png");
const ICON_RECORDING: &[u8] = include_bytes!("../icons/tray-recording.png");
const ICON_TRANSCRIBING: &[u8] = include_bytes!("../icons/tray-transcribing.png");
const ICON_LOADING: &[u8] = include_bytes!("../icons/tray-loading.png");
const ICON_ERROR: &[u8] = include_bytes!("../icons/tray-error.png");
const ICON_DISABLED: &[u8] = include_bytes!("../icons/tray-disabled.png");

const MAX_RECENT_TRANSCRIPTS: usize = 5;
const MAX_RECENT_TRANSCRIPT_CHARS: usize = 50;

/// Flat audio-device record needed by the tray menu builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayAudioDevice {
    pub id: String,
    pub name: String,
}

/// Pure-state snapshot used to build a deterministic tray menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuState {
    pub enabled: bool,
    pub recording: bool,
    pub mode: String,
    pub language: Option<String>,
    pub current_device: Option<String>,
    pub devices: Vec<TrayAudioDevice>,
    /// Recent transcripts as `(entry_id, text)` pairs for stable menu item identity.
    pub recent_transcripts: Vec<(String, String)>,
    pub overlay_enabled: bool,
    pub model_status: String,
    pub sidecar_state: String,
    pub window_visible: bool,
}

/// Pure tray menu tree, independent of any Tauri runtime handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayMenuEntry {
    Separator,
    Action {
        id: String,
        text: String,
        enabled: bool,
    },
    Toggle {
        id: String,
        text: String,
        enabled: bool,
        checked: bool,
    },
    Submenu {
        id: String,
        text: String,
        enabled: bool,
        items: Vec<TrayMenuEntry>,
    },
    Quit,
}

type SystemTrayMenu = Menu<tauri::Wry>;

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

fn normalize_mode_label(mode: &str) -> &'static str {
    if mode.eq_ignore_ascii_case("toggle") {
        "Toggle"
    } else {
        "Hold"
    }
}

fn language_label(language: Option<&str>) -> String {
    match language {
        Some("auto") | None => "Language: Auto".to_string(),
        Some(value) => format!("Language: {}", value),
    }
}

fn window_label(window_visible: bool) -> &'static str {
    if window_visible {
        "Hide Window"
    } else {
        "Show Window"
    }
}

fn truncate_for_menu(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let chars_count = trimmed.chars().count();

    if chars_count <= max_chars {
        return trimmed.to_string();
    }

    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let mut out = String::with_capacity(max_chars);
    for ch in trimmed.chars().take(max_chars - 3) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

/// Pure menu builder: deterministic for a given input state.
pub fn build_tray_menu(state: &TrayMenuState) -> Vec<TrayMenuEntry> {
    let mut recent_items = Vec::new();
    for (entry_id, transcript) in state.recent_transcripts.iter().take(MAX_RECENT_TRANSCRIPTS) {
        recent_items.push(TrayMenuEntry::Action {
            id: format!("{}{}", menu_ids::COPY_RECENT_PREFIX, entry_id),
            text: truncate_for_menu(transcript, MAX_RECENT_TRANSCRIPT_CHARS),
            enabled: true,
        });
    }
    if recent_items.is_empty() {
        recent_items.push(TrayMenuEntry::Action {
            id: "recent_empty".to_string(),
            text: "No recent transcripts".to_string(),
            enabled: false,
        });
    }

    let mut mic_items = Vec::new();
    if state.devices.is_empty() {
        mic_items.push(TrayMenuEntry::Action {
            id: "mic_empty".to_string(),
            text: "No microphone devices".to_string(),
            enabled: false,
        });
    } else {
        for device in &state.devices {
            mic_items.push(TrayMenuEntry::Toggle {
                id: format!("{}{}", menu_ids::SELECT_MIC_PREFIX, device.id),
                text: device.name.clone(),
                enabled: true,
                checked: state.current_device.as_ref() == Some(&device.id),
            });
        }
    }

    vec![
        TrayMenuEntry::Action {
            id: menu_ids::HEADER.to_string(),
            text: "OpenVoicy".to_string(),
            enabled: false,
        },
        TrayMenuEntry::Separator,
        TrayMenuEntry::Toggle {
            id: menu_ids::TOGGLE_ENABLED.to_string(),
            text: "Enabled".to_string(),
            enabled: true,
            checked: state.enabled,
        },
        TrayMenuEntry::Action {
            id: menu_ids::TOGGLE_RECORDING.to_string(),
            text: if state.recording {
                "Stop Recording".to_string()
            } else {
                "Start Recording".to_string()
            },
            // Functional start/stop control is wired in downstream issue translatorvoiceinputtool-36i.1.5.
            enabled: false,
        },
        TrayMenuEntry::Separator,
        TrayMenuEntry::Action {
            id: menu_ids::MODE_STATUS.to_string(),
            text: format!("Mode: {}", normalize_mode_label(&state.mode)),
            enabled: false,
        },
        TrayMenuEntry::Action {
            id: menu_ids::LANGUAGE_STATUS.to_string(),
            text: language_label(state.language.as_deref()),
            enabled: false,
        },
        TrayMenuEntry::Separator,
        TrayMenuEntry::Submenu {
            id: menu_ids::MIC_SUBMENU.to_string(),
            text: match state.current_device.as_deref() {
                Some(name) if !name.is_empty() => format!("Mic: {}", name),
                _ => "Mic: System Default".to_string(),
            },
            enabled: true,
            items: mic_items,
        },
        TrayMenuEntry::Separator,
        TrayMenuEntry::Submenu {
            id: menu_ids::RECENT_SUBMENU.to_string(),
            text: "Recent".to_string(),
            enabled: true,
            items: recent_items,
        },
        TrayMenuEntry::Separator,
        TrayMenuEntry::Toggle {
            id: menu_ids::TOGGLE_OVERLAY.to_string(),
            text: "Show Overlay".to_string(),
            enabled: true,
            checked: state.overlay_enabled,
        },
        TrayMenuEntry::Separator,
        TrayMenuEntry::Action {
            id: menu_ids::MODEL_STATUS.to_string(),
            text: format!("Model: {}", state.model_status),
            enabled: false,
        },
        TrayMenuEntry::Action {
            id: menu_ids::SIDECAR_STATUS.to_string(),
            text: format!("Sidecar: {}", state.sidecar_state),
            enabled: false,
        },
        TrayMenuEntry::Separator,
        TrayMenuEntry::Action {
            id: menu_ids::TOGGLE_WINDOW.to_string(),
            text: window_label(state.window_visible).to_string(),
            enabled: true,
        },
        TrayMenuEntry::Quit,
    ]
}

fn append_entry_to_submenu(
    app: &AppHandle,
    submenu: &Submenu<tauri::Wry>,
    entry: &TrayMenuEntry,
) -> Result<(), tauri::Error> {
    match entry {
        TrayMenuEntry::Separator => {
            submenu.append(&PredefinedMenuItem::separator(app)?)?;
        }
        TrayMenuEntry::Action { id, text, enabled } => {
            let item = MenuItemBuilder::with_id(MenuId::new(id), text)
                .enabled(*enabled)
                .build(app)?;
            submenu.append(&item)?;
        }
        TrayMenuEntry::Toggle {
            id,
            text,
            enabled,
            checked,
        } => {
            let item = CheckMenuItemBuilder::with_id(MenuId::new(id), text)
                .enabled(*enabled)
                .checked(*checked)
                .build(app)?;
            submenu.append(&item)?;
        }
        TrayMenuEntry::Submenu {
            id,
            text,
            enabled,
            items,
        } => {
            let nested_submenu = Submenu::with_id(app, MenuId::new(id), text, *enabled)?;
            for child in items {
                append_entry_to_submenu(app, &nested_submenu, child)?;
            }
            submenu.append(&nested_submenu)?;
        }
        TrayMenuEntry::Quit => {
            submenu.append(&PredefinedMenuItem::quit(app, Some("Quit"))?)?;
        }
    }

    Ok(())
}

fn append_entry_to_menu(
    app: &AppHandle,
    menu: &SystemTrayMenu,
    entry: &TrayMenuEntry,
) -> Result<(), tauri::Error> {
    match entry {
        TrayMenuEntry::Separator => {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
        }
        TrayMenuEntry::Action { id, text, enabled } => {
            let item = MenuItemBuilder::with_id(MenuId::new(id), text)
                .enabled(*enabled)
                .build(app)?;
            menu.append(&item)?;
        }
        TrayMenuEntry::Toggle {
            id,
            text,
            enabled,
            checked,
        } => {
            let item = CheckMenuItemBuilder::with_id(MenuId::new(id), text)
                .enabled(*enabled)
                .checked(*checked)
                .build(app)?;
            menu.append(&item)?;
        }
        TrayMenuEntry::Submenu {
            id,
            text,
            enabled,
            items,
        } => {
            let submenu = Submenu::with_id(app, MenuId::new(id), text, *enabled)?;
            for child in items {
                append_entry_to_submenu(app, &submenu, child)?;
            }
            menu.append(&submenu)?;
        }
        TrayMenuEntry::Quit => {
            menu.append(&PredefinedMenuItem::quit(app, Some("Quit"))?)?;
        }
    }

    Ok(())
}

fn map_state_to_model_status(state: AppState, enabled: bool) -> &'static str {
    if !enabled {
        return "paused";
    }

    match state {
        AppState::LoadingModel => "loading",
        AppState::Error => "error",
        _ => "ready",
    }
}

fn map_state_to_sidecar_status(state: AppState) -> &'static str {
    match state {
        AppState::Error => "failed",
        _ => "ready",
    }
}

fn load_runtime_tray_menu_state(app: &AppHandle, state: AppState, enabled: bool) -> TrayMenuState {
    let current_config = config::load_config();
    let history = app.state::<TranscriptHistory>();
    let recent_transcripts = history
        .all()
        .into_iter()
        .map(|entry| (entry.id.to_string(), entry.text))
        .take(MAX_RECENT_TRANSCRIPTS)
        .collect::<Vec<_>>();

    let mode = match current_config.hotkeys.mode {
        HotkeyMode::Hold => "hold".to_string(),
        HotkeyMode::Toggle => "toggle".to_string(),
    };

    let language = current_config
        .model
        .as_ref()
        .and_then(|model| model.language.clone());

    let current_device = current_config.audio.device_uid.clone();

    let window_visible = app
        .get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false);

    TrayMenuState {
        enabled,
        recording: state == AppState::Recording,
        mode,
        language,
        current_device,
        // Device listing from sidecar will be wired in translatorvoiceinputtool-36i.1.4.
        devices: Vec::new(),
        recent_transcripts,
        overlay_enabled: current_config.ui.overlay_enabled,
        model_status: map_state_to_model_status(state, enabled).to_string(),
        sidecar_state: map_state_to_sidecar_status(state).to_string(),
        window_visible,
    }
}

/// Load a PNG icon from bytes into a Tauri Image.
fn load_png_icon(bytes: &[u8]) -> Result<Image<'static>, String> {
    // Decode PNG to RGBA bytes.
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("PNG decode error: {}", e))?;

    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG frame error: {}", e))?;

    // Handle different color types.
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            // Convert RGB to RGBA.
            let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
            for chunk in buf[..info.buffer_size()].chunks(3) {
                rgba.extend_from_slice(chunk);
                rgba.push(255); // Alpha.
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            // Convert Grayscale+Alpha to RGBA.
            let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
            for chunk in buf[..info.buffer_size()].chunks(2) {
                rgba.push(chunk[0]); // R.
                rgba.push(chunk[0]); // G.
                rgba.push(chunk[0]); // B.
                rgba.push(chunk[1]); // A.
            }
            rgba
        }
        png::ColorType::Grayscale => {
            // Convert Grayscale to RGBA.
            let mut rgba = Vec::with_capacity(info.width as usize * info.height as usize * 4);
            for &pixel in &buf[..info.buffer_size()] {
                rgba.push(pixel); // R.
                rgba.push(pixel); // G.
                rgba.push(pixel); // B.
                rgba.push(255); // A.
            }
            rgba
        }
        png::ColorType::Indexed => {
            return Err("Indexed PNG not supported".to_string());
        }
    };

    Ok(Image::new_owned(rgba, info.width, info.height))
}

fn build_system_tray_menu(
    app: &AppHandle,
    state: &TrayMenuState,
) -> Result<SystemTrayMenu, tauri::Error> {
    let menu = Menu::new(app)?;
    for entry in build_tray_menu(state) {
        append_entry_to_menu(app, &menu, &entry)?;
    }
    Ok(menu)
}

/// Create and set up the system tray.
fn setup_tray(app: &AppHandle) -> Result<TrayIcon, tauri::Error> {
    let state_manager = app.state::<Arc<AppStateManager>>();
    let initial_state = state_manager.get();
    let initial_enabled = state_manager.is_enabled();
    let tray_menu_state = load_runtime_tray_menu_state(app, initial_state, initial_enabled);
    let menu = build_system_tray_menu(app, &tray_menu_state)?;

    // Load the initial icon.
    let icon = load_png_icon(get_icon_for_state(initial_state, initial_enabled))
        .map_err(|e| tauri::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;

    // Build the tray icon.
    let tray = TrayIconBuilder::new()
        .icon(icon)
        .tooltip(get_tooltip_text(initial_state, initial_enabled))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(handle_tray_event)
        .build(app)?;

    Ok(tray)
}

fn toggle_window_visibility(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        match window.is_visible() {
            Ok(true) => {
                let _ = window.hide();
            }
            Ok(false) => {
                let _ = window.show();
                let _ = window.set_focus();
            }
            Err(err) => {
                log::warn!("Unable to read main window visibility: {}", err);
            }
        }
    }
}

fn toggle_overlay_setting() -> Result<bool, String> {
    let mut cfg = config::load_config();
    cfg.ui.overlay_enabled = !cfg.ui.overlay_enabled;
    config::save_config(&cfg).map_err(|e| e.to_string())?;
    Ok(cfg.ui.overlay_enabled)
}

fn select_microphone(device_uid: &str) -> Result<(), String> {
    let mut cfg = config::load_config();
    cfg.audio.device_uid = Some(device_uid.to_string());
    config::save_config(&cfg).map_err(|e| e.to_string())
}

/// Handle menu item clicks.
fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();

    if let Some(id_str) = id.strip_prefix(menu_ids::COPY_RECENT_PREFIX) {
        if let Ok(entry_id) = id_str.parse::<uuid::Uuid>() {
            let history = app.state::<TranscriptHistory>();
            if let Some(entry) = history.get(entry_id) {
                let _ = crate::injection::set_clipboard_public(&entry.text);
                log::info!("Copied recent transcript {} to clipboard", entry_id);
            } else {
                log::warn!("Recent transcript {} no longer in history", entry_id);
            }
        }
        return;
    }

    if let Some(device_uid) = id.strip_prefix(menu_ids::SELECT_MIC_PREFIX) {
        match select_microphone(device_uid) {
            Ok(()) => log::info!("Selected microphone device from tray: {}", device_uid),
            Err(err) => log::warn!("Failed to select microphone from tray: {}", err),
        }
        return;
    }

    match id {
        menu_ids::TOGGLE_ENABLED => {
            let state_manager = app.state::<Arc<AppStateManager>>();
            let currently_enabled = state_manager.is_enabled();
            state_manager.set_enabled(!currently_enabled);
            log::info!("Enabled toggled to {}", !currently_enabled);
        }
        menu_ids::TOGGLE_WINDOW => {
            toggle_window_visibility(app);
        }
        menu_ids::TOGGLE_OVERLAY => match toggle_overlay_setting() {
            Ok(now_enabled) => {
                log::info!("Overlay toggled to {}", now_enabled);
                let _ = app.emit(
                    "overlay:toggle",
                    crate::event_seq::payload_with_next_seq(serde_json::json!({
                        "enabled": now_enabled,
                    })),
                );
            }
            Err(err) => log::warn!("Failed to toggle overlay from tray: {}", err),
        },
        menu_ids::TOGGLE_RECORDING => {
            log::info!("Tray start/stop recording requested (not yet wired)");
        }
        _ => {
            log::debug!("Tray: Unhandled menu event: {}", id);
        }
    }
}

/// Handle tray icon events (clicks, etc).
fn handle_tray_event(tray: &TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::DoubleClick { .. } = event {
        let app = tray.app_handle();
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
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

    /// Update tray icon, tooltip, and dynamic menu from current state.
    pub fn update_state(&self, state: AppState, enabled: bool) -> Result<(), String> {
        let tray = self
            .tray
            .as_ref()
            .ok_or_else(|| "Tray not initialized".to_string())?;

        let icon_bytes = get_icon_for_state(state, enabled);
        let icon = load_png_icon(icon_bytes)?;
        tray.set_icon(Some(icon)).map_err(|e| e.to_string())?;

        let tooltip = get_tooltip_text(state, enabled);
        tray.set_tooltip(Some(tooltip)).map_err(|e| e.to_string())?;

        let state = load_runtime_tray_menu_state(&self.app_handle, state, enabled);
        let menu = build_system_tray_menu(&self.app_handle, &state).map_err(|e| e.to_string())?;
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;

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

    fn sample_state() -> TrayMenuState {
        TrayMenuState {
            enabled: true,
            recording: false,
            mode: "hold".to_string(),
            language: None,
            current_device: Some("Built-in Mic".to_string()),
            devices: vec![
                TrayAudioDevice {
                    id: "Built-in Mic".to_string(),
                    name: "Built-in Mic".to_string(),
                },
                TrayAudioDevice {
                    id: "USB Mic".to_string(),
                    name: "USB Mic".to_string(),
                },
            ],
            recent_transcripts: vec![
                ("id-1".to_string(), "first short transcript".to_string()),
                ("id-2".to_string(), "second short transcript".to_string()),
            ],
            overlay_enabled: true,
            model_status: "ready".to_string(),
            sidecar_state: "ready".to_string(),
            window_visible: false,
        }
    }

    #[test]
    fn test_get_icon_for_state_disabled() {
        let icon = get_icon_for_state(AppState::Idle, false);
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
    fn test_truncate_for_menu_applies_ascii_ellipsis() {
        let input = "012345678901234567890123456789012345678901234567890123456789";
        let out = truncate_for_menu(input, 50);
        assert_eq!(out.len(), 50);
        assert!(out.ends_with("..."));
    }

    #[test]
    fn test_build_tray_menu_core_structure_and_labels() {
        let menu = build_tray_menu(&sample_state());

        assert!(matches!(
            menu.first(),
            Some(TrayMenuEntry::Action { id, text, enabled })
                if id == menu_ids::HEADER && text == "OpenVoicy" && !enabled
        ));

        assert!(menu.iter().any(|entry| {
            matches!(
                entry,
                TrayMenuEntry::Toggle {
                    id,
                    text,
                    checked,
                    ..
                } if id == menu_ids::TOGGLE_ENABLED && text == "Enabled" && *checked
            )
        }));

        assert!(menu.iter().any(|entry| {
            matches!(
                entry,
                TrayMenuEntry::Action { id, text, .. }
                    if id == menu_ids::TOGGLE_RECORDING && text == "Start Recording"
            )
        }));

        assert!(menu.iter().any(|entry| {
            matches!(
                entry,
                TrayMenuEntry::Submenu { id, text, items, .. }
                    if id == menu_ids::MIC_SUBMENU
                        && text == "Mic: Built-in Mic"
                        && !items.is_empty()
            )
        }));

        assert!(menu.iter().any(|entry| {
            matches!(
                entry,
                TrayMenuEntry::Submenu { id, text, items, .. }
                    if id == menu_ids::RECENT_SUBMENU
                        && text == "Recent"
                        && !items.is_empty()
            )
        }));

        assert!(menu.iter().any(|entry| {
            matches!(
                entry,
                TrayMenuEntry::Action { id, text, .. }
                    if id == menu_ids::TOGGLE_WINDOW && text == "Show Window"
            )
        }));

        assert!(matches!(menu.last(), Some(TrayMenuEntry::Quit)));
    }

    #[test]
    fn test_build_tray_menu_recent_is_limited_to_five_entries_regression() {
        let mut state = sample_state();
        state.recent_transcripts = (0..8)
            .map(|idx| (format!("id-{}", idx), format!("transcript number {}", idx)))
            .collect();

        let menu = build_tray_menu(&state);
        let recent = menu
            .iter()
            .find_map(|entry| match entry {
                TrayMenuEntry::Submenu { id, items, .. } if id == menu_ids::RECENT_SUBMENU => {
                    Some(items)
                }
                _ => None,
            })
            .expect("recent submenu should exist");

        assert_eq!(recent.len(), 5);
    }

    #[test]
    fn test_recent_menu_items_use_stable_entry_ids_not_indices() {
        let mut state = sample_state();
        state.recent_transcripts = vec![
            ("abc-uuid-1".to_string(), "first transcript".to_string()),
            ("def-uuid-2".to_string(), "second transcript".to_string()),
        ];

        let menu = build_tray_menu(&state);
        let recent = menu
            .iter()
            .find_map(|entry| match entry {
                TrayMenuEntry::Submenu { id, items, .. } if id == menu_ids::RECENT_SUBMENU => {
                    Some(items)
                }
                _ => None,
            })
            .expect("recent submenu should exist");

        let ids: Vec<&str> = recent
            .iter()
            .filter_map(|entry| match entry {
                TrayMenuEntry::Action { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();

        // IDs must embed the entry UUID, not a volatile index
        assert_eq!(
            ids,
            vec!["copy_recent::abc-uuid-1", "copy_recent::def-uuid-2",]
        );
    }

    #[test]
    fn test_build_tray_menu_handles_empty_devices_and_recent() {
        let mut state = sample_state();
        state.devices.clear();
        state.recent_transcripts.clear();
        state.window_visible = true;

        let menu = build_tray_menu(&state);

        assert!(menu.iter().any(|entry| {
            matches!(
                entry,
                TrayMenuEntry::Submenu { id, items, .. }
                    if id == menu_ids::MIC_SUBMENU
                        && matches!(
                            items.first(),
                            Some(TrayMenuEntry::Action { text, enabled, .. })
                                if text == "No microphone devices" && !enabled
                        )
            )
        }));

        assert!(menu.iter().any(|entry| {
            matches!(
                entry,
                TrayMenuEntry::Submenu { id, items, .. }
                    if id == menu_ids::RECENT_SUBMENU
                        && matches!(
                            items.first(),
                            Some(TrayMenuEntry::Action { text, enabled, .. })
                                if text == "No recent transcripts" && !enabled
                        )
            )
        }));

        assert!(menu.iter().any(|entry| {
            matches!(
                entry,
                TrayMenuEntry::Action { id, text, .. }
                    if id == menu_ids::TOGGLE_WINDOW && text == "Hide Window"
            )
        }));
    }

    #[test]
    fn test_menu_ids_unique() {
        let ids = [
            menu_ids::HEADER,
            menu_ids::TOGGLE_ENABLED,
            menu_ids::TOGGLE_RECORDING,
            menu_ids::MODE_STATUS,
            menu_ids::LANGUAGE_STATUS,
            menu_ids::MIC_SUBMENU,
            menu_ids::RECENT_SUBMENU,
            menu_ids::TOGGLE_OVERLAY,
            menu_ids::MODEL_STATUS,
            menu_ids::SIDECAR_STATUS,
            menu_ids::TOGGLE_WINDOW,
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
}
