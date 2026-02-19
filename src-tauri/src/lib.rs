//! Voice Input Tool - Tauri backend library
//!
//! This library provides the core functionality for the Voice Input Tool,
//! a desktop application that transcribes speech to text using local ASR.

use std::sync::Arc;

use tauri::Manager;
use tokio::sync::RwLock;

mod capabilities;
mod commands;
mod config;
pub mod contracts;
mod errors;
mod event_seq;
mod focus;
mod history;
mod hotkey;
mod injection;
mod integration;
pub mod ipc;
mod log_buffer;
mod model_defaults;
mod recording;
mod sidecar;
mod state;
mod supervisor;
mod tray;
mod watchdog;

use history::TranscriptHistory;
use integration::IntegrationManager;
use state::AppStateManager;

/// Integration manager wrapper for Tauri state.
pub struct IntegrationState(pub Arc<RwLock<IntegrationManager>>);

/// Configure and run the Tauri application
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging with diagnostics ring-buffer capture.
    log_buffer::init_buffer_logger(log::Level::Info);

    // Create shared state manager
    let state_manager = Arc::new(AppStateManager::new());

    // Create integration manager
    let integration_manager = Arc::new(RwLock::new(IntegrationManager::new(Arc::clone(
        &state_manager,
    ))));
    let initial_config = config::load_config();
    let transcript_history =
        TranscriptHistory::with_capacity(initial_config.history.max_entries as usize);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        // Manage state components
        .manage(IntegrationState(Arc::clone(&integration_manager)))
        .manage(state_manager)
        .manage(transcript_history)
        .invoke_handler(tauri::generate_handler![
            // State commands
            commands::get_app_state,
            commands::get_capabilities,
            commands::get_capability_issues,
            commands::can_start_recording,
            commands::run_self_check,
            // Config commands
            commands::get_config,
            commands::update_config,
            commands::reset_config_to_defaults,
            // Audio commands
            commands::list_audio_devices,
            commands::set_audio_device,
            commands::start_mic_test,
            commands::stop_mic_test,
            // Model commands
            commands::get_model_status,
            commands::get_model_catalog,
            commands::download_model,
            commands::purge_model_cache,
            commands::restart_sidecar,
            // History commands
            commands::get_transcript_history,
            commands::copy_transcript,
            commands::copy_last_transcript,
            commands::clear_history,
            // Hotkey commands
            commands::get_hotkey_status,
            commands::set_hotkey,
            // Replacement commands
            commands::get_replacement_rules,
            commands::set_replacement_rules,
            commands::preview_replacement,
            commands::get_available_presets,
            commands::load_preset,
            // Control commands
            commands::start_recording,
            commands::stop_recording,
            commands::toggle_enabled,
            commands::is_enabled,
            commands::set_enabled,
            // Diagnostics commands
            commands::generate_diagnostics,
            commands::get_recent_logs,
        ])
        .setup(|app| {
            // Configure sidecar path for development
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

            // Set up system tray
            let state_manager_for_tray = app.state::<Arc<AppStateManager>>().inner().clone();
            let app_handle_for_tray = app.handle().clone();

            // Initialize tray
            let mut tray_manager = tray::TrayManager::new(app_handle_for_tray.clone());
            if let Err(e) = tray_manager.init() {
                log::warn!("Failed to initialize tray: {}", e);
            }
            let tray_manager = Arc::new(RwLock::new(tray_manager));

            // Start tray update loop
            tray::start_tray_loop(app_handle_for_tray, state_manager_for_tray, tray_manager);

            // Set up integration manager with app handle and initialize
            let integration_state = app.state::<IntegrationState>();
            let integration_manager = Arc::clone(&integration_state.0);
            let app_handle = app.handle().clone();

            // Initialize integration manager in async context
            tauri::async_runtime::spawn(async move {
                {
                    let mut manager = integration_manager.write().await;
                    manager.set_app_handle(app_handle);
                }

                // Initialize all components (hotkeys, sidecar, event loops)
                let manager = integration_manager.read().await;
                if let Err(e) = manager.initialize().await {
                    log::error!("Failed to initialize integration manager: {}", e);
                }

                // Initialize ASR model
                // This is optional at startup - user can trigger via UI
                log::info!("Integration manager initialized, ASR will initialize on first use");
            });

            #[cfg(debug_assertions)]
            {
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            log::info!("Voice Input Tool starting");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
