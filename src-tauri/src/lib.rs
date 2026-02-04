//! Voice Input Tool - Tauri backend library
//!
//! This library provides the core functionality for the Voice Input Tool,
//! a desktop application that transcribes speech to text using local ASR.

use std::sync::Mutex;

use tauri::{Manager, State};

mod ipc;
mod sidecar;

use sidecar::{SidecarManager, SidecarStatus};

/// Global sidecar manager state
struct AppState {
    sidecar: Mutex<SidecarManager>,
}

/// Simple echo command for testing Rust-JS communication
#[tauri::command]
fn echo(message: String) -> String {
    format!("Echo from Rust: {}", message)
}

/// Get the current sidecar status
#[tauri::command]
fn get_sidecar_status(state: State<AppState>) -> SidecarStatus {
    state.sidecar.lock().unwrap().get_status()
}

/// Start the sidecar process
#[tauri::command]
fn start_sidecar(state: State<AppState>) -> Result<SidecarStatus, String> {
    let manager = state.sidecar.lock().unwrap();
    manager.start()?;
    Ok(manager.get_status())
}

/// Stop the sidecar process
#[tauri::command]
fn stop_sidecar(state: State<AppState>) -> Result<SidecarStatus, String> {
    let manager = state.sidecar.lock().unwrap();
    manager.stop()?;
    Ok(manager.get_status())
}

/// Retry starting the sidecar after failure
#[tauri::command]
fn retry_sidecar(state: State<AppState>) -> Result<SidecarStatus, String> {
    let manager = state.sidecar.lock().unwrap();
    manager.retry()?;
    Ok(manager.get_status())
}

/// Configure and run the Tauri application
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            sidecar: Mutex::new(SidecarManager::new()),
        })
        .invoke_handler(tauri::generate_handler![
            echo,
            get_sidecar_status,
            start_sidecar,
            stop_sidecar,
            retry_sidecar,
        ])
        .setup(|app| {
            // Set up sidecar manager with app handle
            {
                let state = app.state::<AppState>();
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
