//! Voice Input Tool - Tauri backend library
//!
//! This library provides the core functionality for the Voice Input Tool,
//! a desktop application that transcribes speech to text using local ASR.

use std::sync::Mutex;

use tauri::{Manager, State};

mod capabilities;
mod ipc;
mod recording;
mod sidecar;
mod state;

use sidecar::{SidecarManager, SidecarStatus};
use state::AppStateManager;

/// Global application state
struct TauriAppState {
    sidecar: Mutex<SidecarManager>,
    state_manager: AppStateManager,
}

/// Simple echo command for testing Rust-JS communication
#[tauri::command]
fn echo(message: String) -> String {
    format!("Echo from Rust: {}", message)
}

/// Get the current sidecar status
#[tauri::command]
fn get_sidecar_status(state: State<TauriAppState>) -> SidecarStatus {
    state.sidecar.lock().unwrap().get_status()
}

/// Start the sidecar process
#[tauri::command]
fn start_sidecar(state: State<TauriAppState>) -> Result<SidecarStatus, String> {
    let manager = state.sidecar.lock().unwrap();
    manager.start()?;
    Ok(manager.get_status())
}

/// Stop the sidecar process
#[tauri::command]
fn stop_sidecar(state: State<TauriAppState>) -> Result<SidecarStatus, String> {
    let manager = state.sidecar.lock().unwrap();
    manager.stop()?;
    Ok(manager.get_status())
}

/// Retry starting the sidecar after failure
#[tauri::command]
fn retry_sidecar(state: State<TauriAppState>) -> Result<SidecarStatus, String> {
    let manager = state.sidecar.lock().unwrap();
    manager.retry()?;
    Ok(manager.get_status())
}

/// Get platform capabilities for the current system
#[tauri::command]
fn get_capabilities() -> capabilities::Capabilities {
    capabilities::Capabilities::detect()
}

/// Get capability issues that need user attention
#[tauri::command]
fn get_capability_issues() -> Vec<capabilities::CapabilityIssue> {
    capabilities::Capabilities::detect().issues()
}

/// Get the current application state
#[tauri::command]
fn get_app_state(state: State<TauriAppState>) -> state::StateEvent {
    state.state_manager.get_event()
}

/// Set whether hotkey listening is enabled (pause/resume)
#[tauri::command]
fn set_app_enabled(state: State<TauriAppState>, enabled: bool) {
    state.state_manager.set_enabled(enabled);
}

/// Check if recording can start (for UI indication)
#[tauri::command]
fn can_start_recording(state: State<TauriAppState>) -> Result<(), state::CannotRecordReason> {
    state.state_manager.can_start_recording()
}

/// Configure and run the Tauri application
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(TauriAppState {
            sidecar: Mutex::new(SidecarManager::new()),
            state_manager: AppStateManager::new(),
        })
        .invoke_handler(tauri::generate_handler![
            echo,
            get_sidecar_status,
            start_sidecar,
            stop_sidecar,
            retry_sidecar,
            get_capabilities,
            get_capability_issues,
            get_app_state,
            set_app_enabled,
            can_start_recording,
        ])
        .setup(|app| {
            // Set up sidecar manager with app handle
            {
                let state = app.state::<TauriAppState>();
                let mut manager = state.sidecar.lock().unwrap();
                manager.set_app_handle(app.handle().clone());

                // Configure sidecar path based on app resources
                // For development, use the local sidecar directory
                #[cfg(debug_assertions)]
                {
                    // In dev mode, use PYTHONPATH to find sidecar
                    std::env::set_var(
                        "PYTHONPATH",
                        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .parent()
                            .unwrap()
                            .join("sidecar")
                            .join("src"),
                    );
                }
            }

            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }

            log::info!("Voice Input Tool starting");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
