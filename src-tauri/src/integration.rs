//! E2E integration wiring: hotkey → record → transcribe → inject.
//!
//! This module orchestrates the complete voice input flow by connecting:
//! - Global hotkeys (from hotkey.rs)
//! - Recording controller (from recording.rs)
//! - Sidecar RPC (from ipc/ and sidecar.rs)
//! - Text injection (from injection.rs)
//!
//! The IntegrationManager is the central coordinator that handles the
//! event-driven flow across all these components.

use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::RwLock;

use crate::config::{self, HotkeyMode};
use crate::focus::{capture_focus, FocusSignature};
use crate::history::TranscriptHistory;
use crate::hotkey::{HotkeyAction, HotkeyManager, RecordingAction};
use crate::injection::{inject_text, InjectionConfig, InjectionResult};
use crate::ipc::{NotificationEvent, RpcClient, RpcError};
use crate::recording::{RecordingController, RecordingEvent, TranscriptionResult};
use crate::state::{AppState, AppStateManager};

/// Tray icon event name.
const EVENT_TRAY_UPDATE: &str = "tray:update";

/// Status changed event name (mirrors sidecar event).
const EVENT_STATUS_CHANGED: &str = "status:changed";

/// Transcription complete event name.
const EVENT_TRANSCRIPTION_COMPLETE: &str = "transcription:complete";

/// Transcription error event name.
const EVENT_TRANSCRIPTION_ERROR: &str = "transcription:error";

/// Integration manager configuration.
#[derive(Debug, Clone)]
pub struct IntegrationConfig {
    /// Python executable path.
    pub python_path: String,
    /// Sidecar module name.
    pub sidecar_module: String,
    /// Whether to auto-start sidecar.
    pub auto_start_sidecar: bool,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        Self {
            python_path: "python3".to_string(),
            sidecar_module: "openvoicy_sidecar".to_string(),
            auto_start_sidecar: true,
        }
    }
}

/// Focus captured before recording started.
struct RecordingContext {
    /// Focus signature at recording start.
    focus_before: FocusSignature,
    /// Session ID for correlation.
    session_id: String,
}

/// Central integration manager that wires everything together.
pub struct IntegrationManager {
    /// Application state manager.
    state_manager: Arc<AppStateManager>,
    /// Recording controller.
    recording_controller: Arc<RecordingController>,
    /// Hotkey manager.
    hotkey_manager: Arc<RwLock<HotkeyManager>>,
    /// RPC client (if sidecar is connected).
    rpc_client: Arc<RwLock<Option<RpcClient>>>,
    /// Sidecar child process.
    sidecar_process: Arc<RwLock<Option<Child>>>,
    /// Tauri app handle.
    app_handle: Option<AppHandle>,
    /// Focus context for current recording.
    recording_context: Arc<RwLock<Option<RecordingContext>>>,
    /// Configuration.
    config: IntegrationConfig,
}

impl IntegrationManager {
    /// Create a new integration manager.
    pub fn new(state_manager: Arc<AppStateManager>) -> Self {
        let recording_controller = Arc::new(RecordingController::new(Arc::clone(&state_manager)));

        Self {
            state_manager,
            recording_controller,
            hotkey_manager: Arc::new(RwLock::new(HotkeyManager::new())),
            rpc_client: Arc::new(RwLock::new(None)),
            sidecar_process: Arc::new(RwLock::new(None)),
            app_handle: None,
            recording_context: Arc::new(RwLock::new(None)),
            config: IntegrationConfig::default(),
        }
    }

    /// Set the Tauri app handle.
    pub fn set_app_handle(&mut self, handle: AppHandle) {
        self.app_handle = Some(handle);
    }

    /// Get the state manager.
    pub fn state_manager(&self) -> &Arc<AppStateManager> {
        &self.state_manager
    }

    /// Get the recording controller.
    pub fn recording_controller(&self) -> &Arc<RecordingController> {
        &self.recording_controller
    }

    /// Initialize and start all components.
    pub async fn initialize(&self) -> Result<(), String> {
        log::info!("Initializing integration manager");

        // Initialize hotkey manager
        {
            let mut hotkey_manager = self.hotkey_manager.write().await;
            match hotkey_manager.initialize() {
                Ok(status) => {
                    log::info!(
                        "Hotkey registered: primary={}, mode={}",
                        status.primary,
                        status.mode
                    );
                }
                Err(e) => {
                    log::warn!("Failed to register hotkeys: {}", e);
                    // Continue anyway - user can fix in settings
                }
            }
        }

        // Start sidecar if configured
        if self.config.auto_start_sidecar {
            self.start_sidecar().await?;
        }

        // Start event loops
        self.start_hotkey_loop();
        self.start_state_loop();
        self.start_recording_event_loop();

        log::info!("Integration manager initialized");
        Ok(())
    }

    /// Start the sidecar process and connect RPC client.
    pub async fn start_sidecar(&self) -> Result<(), String> {
        log::info!("Starting sidecar process");

        // Spawn sidecar process
        let mut child = Command::new(&self.config.python_path)
            .arg("-m")
            .arg(&self.config.sidecar_module)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn sidecar: {}", e))?;

        let pid = child.id();
        log::info!("Sidecar spawned with PID {}", pid);

        // Extract stdin/stdout for RPC client
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to capture stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture stdout".to_string())?;

        // Create RPC client
        let rpc_client = RpcClient::new(stdin, stdout);

        // Start notification listener
        self.start_notification_loop(rpc_client.subscribe());

        // Store references
        *self.sidecar_process.write().await = Some(child);
        *self.rpc_client.write().await = Some(rpc_client);

        // Verify connection with ping
        self.ping_sidecar().await?;

        log::info!("Sidecar connected");
        Ok(())
    }

    /// Ping the sidecar to verify connection.
    async fn ping_sidecar(&self) -> Result<(), String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        #[derive(Deserialize)]
        struct PingResult {
            version: String,
            protocol: String,
        }

        let result: PingResult = client
            .call("system.ping", None)
            .await
            .map_err(|e| format!("Ping failed: {}", e))?;

        log::info!(
            "Sidecar version: {}, protocol: {}",
            result.version,
            result.protocol
        );
        Ok(())
    }

    /// Initialize ASR model via sidecar.
    pub async fn initialize_asr(&self, model_id: &str, device: &str) -> Result<(), String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        // Transition to loading state
        let _ = self.state_manager.transition(AppState::LoadingModel);
        self.emit_tray_update(AppState::LoadingModel);

        let params = json!({
            "model_id": model_id,
            "device_pref": device
        });

        #[derive(Deserialize)]
        struct InitResult {
            status: String,
        }

        match client.call::<InitResult>("asr.initialize", Some(params)).await {
            Ok(result) => {
                log::info!("ASR initialized: status={}", result.status);
                // Mark model ready
                self.recording_controller.set_model_ready(true).await;
                // Transition to idle
                let _ = self.state_manager.transition(AppState::Idle);
                self.emit_tray_update(AppState::Idle);
                Ok(())
            }
            Err(e) => {
                log::error!("ASR initialization failed: {}", e);
                self.state_manager
                    .transition_to_error(format!("Model initialization failed: {}", e));
                self.emit_tray_update(AppState::Error);
                Err(format!("ASR initialization failed: {}", e))
            }
        }
    }

    /// Start hotkey event processing loop.
    fn start_hotkey_loop(&self) {
        let hotkey_manager = Arc::clone(&self.hotkey_manager);
        let state_manager = Arc::clone(&self.state_manager);
        let recording_controller = Arc::clone(&self.recording_controller);
        let rpc_client = Arc::clone(&self.rpc_client);
        let recording_context = Arc::clone(&self.recording_context);
        let app_handle = self.app_handle.clone();

        tokio::spawn(async move {
            // Take the receiver from hotkey manager
            let mut receiver = {
                let mut hk = hotkey_manager.write().await;
                match hk.take_action_receiver() {
                    Some(rx) => rx,
                    None => {
                        log::warn!("Hotkey receiver already taken");
                        return;
                    }
                }
            };

            log::info!("Hotkey event loop started");

            while let Some(action) = receiver.recv().await {
                let config = config::load_config();

                match action {
                    HotkeyAction::PrimaryDown => {
                        // Handle based on mode
                        let hk = hotkey_manager.read().await;
                        let recording_action = hk.handle_primary_down(&state_manager);

                        if let Some(RecordingAction::Start) = recording_action {
                            // Capture focus before recording
                            let focus = capture_focus();
                            log::debug!("Captured focus before recording: {:?}", focus);

                            // Start recording
                            match recording_controller.start().await {
                                Ok(session_id) => {
                                    log::info!("Recording started: session={}", session_id);

                                    // Store context
                                    *recording_context.write().await = Some(RecordingContext {
                                        focus_before: focus,
                                        session_id: session_id.clone(),
                                    });

                                    // Tell sidecar to start recording
                                    if let Some(client) = rpc_client.read().await.as_ref() {
                                        let params = json!({
                                            "session_id": session_id,
                                            "device_uid": config.audio.device_uid
                                        });
                                        let _: Result<Value, _> =
                                            client.call("recording.start", Some(params)).await;
                                    }
                                }
                                Err(e) => {
                                    log::warn!("Failed to start recording: {}", e);
                                }
                            }
                        } else if let Some(RecordingAction::Stop) = recording_action {
                            // Toggle mode: stop recording
                            Self::stop_recording_flow(
                                &recording_controller,
                                &rpc_client,
                                &recording_context,
                            )
                            .await;
                        }
                    }
                    HotkeyAction::PrimaryUp => {
                        // Only relevant for hold mode
                        if config.hotkeys.mode == HotkeyMode::Hold {
                            let hk = hotkey_manager.read().await;
                            if let Some(RecordingAction::Stop) = hk.handle_primary_up() {
                                Self::stop_recording_flow(
                                    &recording_controller,
                                    &rpc_client,
                                    &recording_context,
                                )
                                .await;
                            }
                        }
                    }
                    HotkeyAction::CopyLast => {
                        // Copy last transcript
                        if let Some(ref handle) = app_handle {
                            let history = handle.state::<TranscriptHistory>();
                            let hk = hotkey_manager.read().await;
                            let result = hk.handle_copy_last(&history);
                            log::debug!("Copy last result: {:?}", result);
                        }
                    }
                }
            }

            log::info!("Hotkey event loop ended");
        });
    }

    /// Stop recording and trigger transcription flow.
    async fn stop_recording_flow(
        recording_controller: &Arc<RecordingController>,
        rpc_client: &Arc<RwLock<Option<RpcClient>>>,
        recording_context: &Arc<RwLock<Option<RecordingContext>>>,
    ) {
        match recording_controller.stop().await {
            Ok(result) => {
                log::info!("Recording stopped: {:?}", result);

                // Tell sidecar to stop recording (triggers async transcription)
                if let Some(client) = rpc_client.read().await.as_ref() {
                    if let Some(ctx) = recording_context.read().await.as_ref() {
                        let params = json!({
                            "session_id": ctx.session_id
                        });
                        let _: Result<Value, _> = client.call("recording.stop", Some(params)).await;
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to stop recording: {}", e);
            }
        }
    }

    /// Start state change event loop (for tray updates).
    fn start_state_loop(&self) {
        let state_manager = Arc::clone(&self.state_manager);
        let app_handle = self.app_handle.clone();

        tokio::spawn(async move {
            let mut receiver = state_manager.subscribe();

            log::info!("State event loop started");

            while let Ok(event) = receiver.recv().await {
                log::debug!("State changed: {:?}", event.state);

                // Emit tray update
                if let Some(ref handle) = app_handle {
                    let icon = match event.state {
                        AppState::Idle => "tray-idle",
                        AppState::Recording => "tray-recording",
                        AppState::Transcribing => "tray-transcribing",
                        AppState::LoadingModel => "tray-loading",
                        AppState::Error => "tray-error",
                    };

                    let _ = handle.emit(
                        EVENT_TRAY_UPDATE,
                        json!({
                            "icon": icon,
                            "state": event.state,
                            "enabled": event.enabled,
                            "detail": event.detail
                        }),
                    );
                }
            }

            log::info!("State event loop ended");
        });
    }

    /// Start recording event loop (for transcription results).
    fn start_recording_event_loop(&self) {
        let recording_controller = Arc::clone(&self.recording_controller);
        let recording_context = Arc::clone(&self.recording_context);
        let app_handle = self.app_handle.clone();

        tokio::spawn(async move {
            let mut receiver = recording_controller.subscribe();

            log::info!("Recording event loop started");

            while let Ok(event) = receiver.recv().await {
                match event {
                    RecordingEvent::TranscriptionComplete {
                        session_id,
                        text,
                        audio_duration_ms,
                        processing_duration_ms,
                        timestamp,
                    } => {
                        log::info!(
                            "Transcription complete: session={}, text_len={}, audio={}ms, processing={}ms",
                            session_id,
                            text.len(),
                            audio_duration_ms,
                            processing_duration_ms
                        );

                        // Get focus context
                        let ctx = recording_context.read().await;
                        let expected_focus = ctx.as_ref().map(|c| &c.focus_before);

                        // Skip injection if text is empty
                        if text.trim().is_empty() {
                            log::info!("Empty transcription, skipping injection");
                            continue;
                        }

                        // Load injection config
                        let config = config::load_config();
                        let injection_config = InjectionConfig {
                            paste_delay_ms: config.injection.paste_delay_ms,
                            restore_clipboard: config.injection.restore_clipboard,
                            suffix: config.injection.suffix.clone(),
                            focus_guard_enabled: config.injection.focus_guard_enabled,
                        };

                        // Inject text
                        let result = inject_text(&text, expected_focus, &injection_config).await;

                        match &result {
                            InjectionResult::Injected { text_length, .. } => {
                                log::info!("Text injected: {} chars", text_length);
                            }
                            InjectionResult::ClipboardOnly { reason, .. } => {
                                log::info!("Clipboard-only mode: {}", reason);
                            }
                            InjectionResult::Failed { error, .. } => {
                                log::error!("Injection failed: {}", error);
                            }
                        }

                        // Add to history
                        if let Some(ref handle) = app_handle {
                            let history = handle.state::<TranscriptHistory>();
                            history.add(text.clone());
                        }

                        // Emit event to frontend
                        if let Some(ref handle) = app_handle {
                            let _ = handle.emit(
                                EVENT_TRANSCRIPTION_COMPLETE,
                                json!({
                                    "session_id": session_id,
                                    "text": text,
                                    "audio_duration_ms": audio_duration_ms,
                                    "processing_duration_ms": processing_duration_ms,
                                    "injection_result": result
                                }),
                            );
                        }

                        // Clear context
                        drop(ctx);
                        *recording_context.write().await = None;
                    }
                    RecordingEvent::TranscriptionFailed {
                        session_id, error, ..
                    } => {
                        log::error!(
                            "Transcription failed: session={}, error={}",
                            session_id,
                            error
                        );

                        if let Some(ref handle) = app_handle {
                            let _ = handle.emit(
                                EVENT_TRANSCRIPTION_ERROR,
                                json!({
                                    "session_id": session_id,
                                    "error": error
                                }),
                            );
                        }

                        // Clear context
                        *recording_context.write().await = None;
                    }
                    _ => {}
                }
            }

            log::info!("Recording event loop ended");
        });
    }

    /// Start sidecar notification processing loop.
    fn start_notification_loop(
        &self,
        mut receiver: tokio::sync::broadcast::Receiver<NotificationEvent>,
    ) {
        let recording_controller = Arc::clone(&self.recording_controller);
        let app_handle = self.app_handle.clone();

        tokio::spawn(async move {
            log::info!("Notification loop started");

            while let Ok(event) = receiver.recv().await {
                log::debug!("Sidecar notification: method={}", event.method);

                match event.method.as_str() {
                    "event.transcription_complete" => {
                        // Parse transcription result
                        #[derive(Deserialize)]
                        struct TranscriptionParams {
                            session_id: String,
                            text: String,
                            duration_ms: u64,
                            #[serde(default)]
                            confidence: Option<f64>,
                        }

                        if let Ok(params) = serde_json::from_value::<TranscriptionParams>(event.params)
                        {
                            let result = TranscriptionResult {
                                session_id: params.session_id,
                                text: params.text,
                                audio_duration_ms: params.duration_ms,
                                processing_duration_ms: 0, // Not provided by sidecar
                            };

                            // Deliver to recording controller (validates session ID)
                            recording_controller.on_transcription_result(result).await;
                        }
                    }
                    "event.transcription_error" => {
                        #[derive(Deserialize)]
                        struct ErrorParams {
                            session_id: String,
                            kind: String,
                            message: String,
                        }

                        if let Ok(params) = serde_json::from_value::<ErrorParams>(event.params) {
                            recording_controller
                                .on_transcription_error(
                                    params.session_id,
                                    format!("{}: {}", params.kind, params.message),
                                )
                                .await;
                        }
                    }
                    "event.status_changed" => {
                        // Forward to frontend
                        if let Some(ref handle) = app_handle {
                            let _ = handle.emit(EVENT_STATUS_CHANGED, event.params);
                        }
                    }
                    "event.audio_level" => {
                        // Forward audio levels to frontend
                        if let Some(ref handle) = app_handle {
                            let _ = handle.emit("audio:level", event.params);
                        }
                    }
                    _ => {
                        log::debug!("Unhandled notification: {}", event.method);
                    }
                }
            }

            log::info!("Notification loop ended");
        });
    }

    /// Emit tray update event.
    fn emit_tray_update(&self, state: AppState) {
        if let Some(ref handle) = self.app_handle {
            let icon = match state {
                AppState::Idle => "tray-idle",
                AppState::Recording => "tray-recording",
                AppState::Transcribing => "tray-transcribing",
                AppState::LoadingModel => "tray-loading",
                AppState::Error => "tray-error",
            };

            let _ = handle.emit(
                EVENT_TRAY_UPDATE,
                json!({
                    "icon": icon,
                    "state": state
                }),
            );
        }
    }

    /// Shutdown all components.
    pub async fn shutdown(&self) {
        log::info!("Shutting down integration manager");

        // Shutdown sidecar
        if let Some(client) = self.rpc_client.write().await.take() {
            // Send shutdown command
            let _: Result<Value, RpcError> = client.call("system.shutdown", None).await;
            client.shutdown().await;
        }

        // Kill sidecar process if still running
        if let Some(mut child) = self.sidecar_process.write().await.take() {
            let _ = child.kill();
        }

        // Shutdown hotkey manager
        {
            let mut hk = self.hotkey_manager.write().await;
            hk.shutdown();
        }

        log::info!("Integration manager shutdown complete");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integration_config_default() {
        let config = IntegrationConfig::default();
        assert_eq!(config.python_path, "python3");
        assert_eq!(config.sidecar_module, "openvoicy_sidecar");
        assert!(config.auto_start_sidecar);
    }

    #[tokio::test]
    async fn test_integration_manager_creation() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        assert!(manager.app_handle.is_none());
    }
}
