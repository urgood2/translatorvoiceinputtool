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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::RwLock;

use crate::config::{self, HotkeyMode};
use crate::errors::{AppError, ErrorKind};
use crate::focus::{capture_focus, FocusSignature};
use crate::history::{
    HistoryInjectionResult, TranscriptEntry, TranscriptHistory, TranscriptTimings,
};
use crate::hotkey::{HotkeyAction, HotkeyManager, RecordingAction};
use crate::injection::{inject_text, InjectionConfig, InjectionResult};
use crate::ipc::{NotificationEvent, RpcClient, RpcError};
use crate::model_defaults;
use crate::recording::{RecordingController, RecordingEvent, StopResult, TranscriptionResult};
use crate::state::{AppState, AppStateManager};
use crate::watchdog::{self, PingCallback, Watchdog, WatchdogConfig, WatchdogEvent};

/// Tray icon event name.
const EVENT_TRAY_UPDATE: &str = "tray:update";

/// Model progress event name.
const EVENT_MODEL_PROGRESS: &str = "model:progress";

/// Model status event name.
const EVENT_MODEL_STATUS: &str = "model:status";

/// Model status tracking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    /// Model status unknown (not yet queried).
    Unknown,
    /// Model not downloaded.
    Missing,
    /// Model download in progress.
    Downloading,
    /// Model download/verification complete, loading.
    Loading,
    /// Model ready for transcription.
    Ready,
    /// Model failed to load or download.
    Error(String),
}

impl Default for ModelStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Download/initialization progress.
#[derive(Debug, Clone, Serialize)]
pub struct ModelProgress {
    /// Current bytes downloaded or processed.
    pub current: u64,
    /// Total bytes (if known).
    pub total: Option<u64>,
    /// Progress stage description.
    pub stage: String,
}

/// Canonical progress payload for model status events.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatusProgress {
    pub current: u64,
    pub total: Option<u64>,
    pub unit: String,
}

/// Canonical model status event payload.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatusPayload {
    pub model_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<ModelStatusProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Status changed event name (mirrors sidecar event).
const EVENT_STATUS_CHANGED: &str = "status:changed";

/// Transcription complete event name.
const EVENT_TRANSCRIPTION_COMPLETE: &str = "transcription:complete";

/// Transcription error event name.
const EVENT_TRANSCRIPTION_ERROR: &str = "transcription:error";
/// Application error event name (legacy + structured compatibility payload).
const EVENT_APP_ERROR: &str = "app:error";

fn status_progress_from_parts(
    current: u64,
    total: Option<u64>,
    unit: Option<String>,
    stage: Option<String>,
) -> ModelStatusProgress {
    ModelStatusProgress {
        current,
        total,
        unit: unit.or(stage).unwrap_or_else(|| "processing".to_string()),
    }
}

fn configured_model_id() -> String {
    config::load_config()
        .model
        .and_then(|m| m.model_id)
        .and_then(|id| {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .unwrap_or_else(|| model_defaults::default_model_id().to_string())
}

fn resolve_model_id(model_id: Option<String>) -> String {
    model_id
        .and_then(|id| {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .unwrap_or_else(configured_model_id)
}

fn model_status_to_event_fields(status: ModelStatus) -> (String, Option<String>) {
    match status {
        ModelStatus::Unknown => ("unknown".to_string(), None),
        ModelStatus::Missing => ("missing".to_string(), None),
        ModelStatus::Downloading => ("downloading".to_string(), None),
        ModelStatus::Loading => ("loading".to_string(), None),
        ModelStatus::Ready => ("ready".to_string(), None),
        ModelStatus::Error(message) => ("error".to_string(), Some(message)),
    }
}

#[derive(Debug, Deserialize)]
struct AudioDeviceSummary {
    uid: String,
}

#[derive(Debug, Deserialize)]
struct AudioListResult {
    #[serde(default)]
    devices: Vec<AudioDeviceSummary>,
}

fn is_configured_device_available(
    configured_device_uid: Option<&str>,
    devices: &[AudioDeviceSummary],
) -> bool {
    match configured_device_uid {
        Some(uid) => devices.iter().any(|device| device.uid == uid),
        None => true,
    }
}

fn add_seq_to_payload(payload: Value, seq: u64) -> Value {
    crate::event_seq::add_seq_to_payload(payload, seq)
}

fn sha256_prefix(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{:x}", digest);
    hex[..8].to_string()
}

fn extract_session_id(params: &Value) -> Option<&str> {
    params.get("session_id").and_then(Value::as_str)
}

fn is_stale_session(
    notification_session_id: Option<&str>,
    active_session_id: Option<&str>,
) -> bool {
    match notification_session_id {
        Some(incoming) => active_session_id != Some(incoming),
        None => false,
    }
}

#[derive(Debug, Clone, Default)]
struct PipelineTimingMarks {
    t0_stop_called: Option<Instant>,
    t1_stop_rpc_returned: Option<Instant>,
    t2_transcription_received: Option<Instant>,
    t3_postprocess_completed: Option<Instant>,
    t4_injection_completed: Option<Instant>,
}

fn delta_ms(start: Option<Instant>, end: Option<Instant>) -> Option<u64> {
    match (start, end) {
        (Some(start), Some(end)) if end >= start => {
            Some(end.duration_since(start).as_millis() as u64)
        }
        _ => None,
    }
}

fn pipeline_timings_from_marks(marks: &PipelineTimingMarks) -> Option<TranscriptTimings> {
    let timings = TranscriptTimings {
        ipc_ms: delta_ms(marks.t0_stop_called, marks.t1_stop_rpc_returned),
        transcribe_ms: delta_ms(marks.t1_stop_rpc_returned, marks.t2_transcription_received),
        postprocess_ms: delta_ms(
            marks.t2_transcription_received,
            marks.t3_postprocess_completed,
        ),
        inject_ms: delta_ms(marks.t3_postprocess_completed, marks.t4_injection_completed),
        total_ms: delta_ms(marks.t0_stop_called, marks.t4_injection_completed),
    };

    if timings.ipc_ms.is_none()
        && timings.transcribe_ms.is_none()
        && timings.postprocess_ms.is_none()
        && timings.inject_ms.is_none()
        && timings.total_ms.is_none()
    {
        None
    } else {
        Some(timings)
    }
}

fn log_pipeline_timings(timings: &TranscriptTimings) {
    let fmt = |v: Option<u64>| match v {
        Some(ms) => format!("{}ms", ms),
        None => "n/a".to_string(),
    };

    log::info!(
        "Pipeline: total={} (ipc={}, transcribe={}, postprocess={}, inject={})",
        fmt(timings.total_ms),
        fmt(timings.ipc_ms),
        fmt(timings.transcribe_ms),
        fmt(timings.postprocess_ms),
        fmt(timings.inject_ms),
    );
}

fn emit_with_shared_seq(
    handle: &AppHandle,
    events: &[&str],
    payload: Value,
    _seq_counter: &Arc<AtomicU64>,
) -> u64 {
    let seq = crate::event_seq::next_event_seq();
    let payload_with_seq = add_seq_to_payload(payload, seq);

    for event in events {
        let _ = handle.emit(*event, payload_with_seq.clone());
    }

    seq
}

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
    /// Pipeline timing marks for stop -> injection latency tracking.
    timing_marks: PipelineTimingMarks,
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
    /// Current active recording/transcription session for correlation.
    current_session_id: Arc<RwLock<Option<String>>>,
    /// Configuration.
    config: IntegrationConfig,
    /// Current model status.
    model_status: Arc<RwLock<ModelStatus>>,
    /// Current model download progress.
    model_progress: Arc<RwLock<Option<ModelProgress>>>,
    /// Whether model initialization has been attempted.
    model_init_attempted: Arc<AtomicBool>,
    /// Watchdog for sidecar health monitoring.
    watchdog: Arc<Watchdog>,
    /// Monotonic event sequence counter for frontend events.
    event_seq: Arc<AtomicU64>,
}

impl IntegrationManager {
    /// Create a new integration manager.
    pub fn new(state_manager: Arc<AppStateManager>) -> Self {
        let recording_controller = Arc::new(RecordingController::new(Arc::clone(&state_manager)));
        let watchdog = Arc::new(Watchdog::with_config(WatchdogConfig::default()));

        Self {
            state_manager,
            recording_controller,
            hotkey_manager: Arc::new(RwLock::new(HotkeyManager::new())),
            rpc_client: Arc::new(RwLock::new(None)),
            sidecar_process: Arc::new(RwLock::new(None)),
            app_handle: None,
            recording_context: Arc::new(RwLock::new(None)),
            current_session_id: Arc::new(RwLock::new(None)),
            config: IntegrationConfig::default(),
            model_status: Arc::new(RwLock::new(ModelStatus::Unknown)),
            model_progress: Arc::new(RwLock::new(None)),
            model_init_attempted: Arc::new(AtomicBool::new(false)),
            watchdog,
            event_seq: Arc::new(AtomicU64::new(1)),
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

        // Check and initialize model in background
        self.spawn_model_check();

        // Start watchdog loop
        self.start_watchdog_loop();

        log::info!("Integration manager initialized");
        Ok(())
    }

    /// Start the watchdog monitoring loop.
    fn start_watchdog_loop(&self) {
        let rpc_client = Arc::clone(&self.rpc_client);
        let watchdog = Arc::clone(&self.watchdog);
        let state_manager = Arc::clone(&self.state_manager);
        let recording_controller = Arc::clone(&self.recording_controller);
        let model_status = Arc::clone(&self.model_status);
        let app_handle = self.app_handle.clone();
        let event_seq = Arc::clone(&self.event_seq);

        // Create ping adapter
        let pinger = Arc::new(RpcPinger {
            rpc_client: Arc::clone(&rpc_client),
        });

        // Start the watchdog loop
        watchdog.start_loop(pinger);

        // Start platform power listener (if available) and feed events into watchdog.
        if let Some(mut power_rx) = watchdog::platform::start_power_listener() {
            let watchdog_for_power = Arc::clone(&watchdog);
            tokio::spawn(async move {
                log::info!("Power event listener started");
                while let Some(event) = power_rx.recv().await {
                    watchdog_for_power.on_power_event(event).await;
                }
                log::info!("Power event listener ended");
            });
        } else {
            log::info!(
                "Power event listener unavailable; watchdog will infer resume via loop-gap fallback"
            );
        }

        // Start event handler loop
        let watchdog_for_events = Arc::clone(&watchdog);
        let mut event_rx = watchdog.subscribe();
        tokio::spawn(async move {
            log::info!("Watchdog event handler started");

            while let Ok(event) = event_rx.recv().await {
                match event {
                    WatchdogEvent::HealthCheck { status } => {
                        log::debug!("Watchdog health check: {:?}", status);
                        // Could emit event to frontend for status display
                    }
                    WatchdogEvent::SidecarHung => {
                        log::error!("Watchdog detected hung sidecar, requesting restart");
                        // Transition to error state
                        state_manager
                            .transition_to_error("Sidecar hung, restarting...".to_string());

                        // Kill and restart sidecar
                        // Note: This would need to be more sophisticated in production
                        // to actually kill and restart the sidecar process
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &["sidecar:restart"],
                                serde_json::json!({ "reason": "hung" }),
                                &event_seq,
                            );
                        }
                    }
                    WatchdogEvent::SystemResumed => {
                        log::info!("System resumed from suspend, triggering revalidation");
                    }
                    WatchdogEvent::RevalidationNeeded => {
                        log::info!("Revalidation needed after resume");

                        // Revalidate sidecar connection
                        let client = rpc_client.read().await;
                        if let Some(ref c) = *client {
                            #[derive(serde::Deserialize)]
                            struct PingResult {
                                #[allow(dead_code)]
                                version: String,
                            }

                            match c.call::<PingResult>("system.ping", None).await {
                                Ok(_) => {
                                    log::info!("Sidecar responsive after resume");
                                }
                                Err(e) => {
                                    log::warn!("Sidecar unresponsive after resume: {}", e);
                                    state_manager.transition_to_error(
                                        "Sidecar unresponsive after resume".to_string(),
                                    );
                                }
                            }
                        }

                        // Revalidate model status
                        #[derive(serde::Deserialize)]
                        struct StatusResult {
                            status: String,
                        }

                        let client = rpc_client.read().await;
                        if let Some(ref c) = *client {
                            match c.call::<StatusResult>("model.get_status", None).await {
                                Ok(result) => {
                                    log::info!("Model status after resume: {}", result.status);
                                    if result.status == "ready" {
                                        recording_controller.set_model_ready(true).await;
                                        *model_status.write().await = ModelStatus::Ready;
                                    } else {
                                        recording_controller.set_model_ready(false).await;
                                        *model_status.write().await = ModelStatus::Missing;
                                    }
                                }
                                Err(e) => {
                                    log::warn!("Failed to get model status after resume: {}", e);
                                }
                            }
                        }

                        // Revalidate available input devices and configured device presence.
                        let client = rpc_client.read().await;
                        if let Some(ref c) = *client {
                            match c.call::<AudioListResult>("audio.list_devices", None).await {
                                Ok(result) => {
                                    let configured_uid = config::load_config().audio.device_uid;
                                    if !is_configured_device_available(
                                        configured_uid.as_deref(),
                                        &result.devices,
                                    ) {
                                        if let Some(uid) = configured_uid {
                                            log::warn!(
                                                "Configured audio device missing after resume: {}",
                                                uid
                                            );
                                            state_manager.transition_to_error(format!(
                                                "Configured audio device unavailable after resume: {}",
                                                uid
                                            ));
                                        }
                                    } else {
                                        log::info!(
                                            "Audio devices revalidated after resume ({} devices)",
                                            result.devices.len()
                                        );
                                    }
                                }
                                Err(e) => {
                                    log::warn!("Failed to list audio devices after resume: {}", e);
                                }
                            }
                        }

                        watchdog_for_events.clear_revalidation_pending().await;
                    }
                }
            }

            log::info!("Watchdog event handler ended");
        });
    }

    /// Spawn model check and initialization in background.
    fn spawn_model_check(&self) {
        let rpc_client = Arc::clone(&self.rpc_client);
        let state_manager = Arc::clone(&self.state_manager);
        let recording_controller = Arc::clone(&self.recording_controller);
        let model_status = Arc::clone(&self.model_status);
        let model_init_attempted = Arc::clone(&self.model_init_attempted);
        let app_handle = self.app_handle.clone();
        let event_seq = Arc::clone(&self.event_seq);

        tokio::spawn(async move {
            // Check if already attempted
            if model_init_attempted.swap(true, Ordering::SeqCst) {
                log::debug!("Model initialization already attempted");
                return;
            }

            // Query model status from sidecar
            let client = rpc_client.read().await;
            let client = match client.as_ref() {
                Some(c) => c,
                None => {
                    log::warn!("Cannot check model status: sidecar not connected");
                    return;
                }
            };

            log::info!("Checking model status on startup");

            #[derive(Deserialize, Debug)]
            struct StatusResult {
                status: String,
                #[serde(default)]
                model_id: Option<String>,
                #[serde(default)]
                revision: Option<String>,
                #[serde(default)]
                cache_path: Option<String>,
                #[serde(default)]
                progress: Option<ProgressResult>,
            }

            #[derive(Deserialize, Debug)]
            struct ProgressResult {
                current: u64,
                #[serde(default)]
                total: Option<u64>,
                #[serde(default)]
                unit: Option<String>,
                #[serde(default)]
                stage: Option<String>,
            }

            match client.call::<StatusResult>("model.get_status", None).await {
                Ok(result) => {
                    log::info!("Model status: {:?}", result);
                    let status_progress = result.progress.as_ref().map(|progress| {
                        status_progress_from_parts(
                            progress.current,
                            progress.total,
                            progress.unit.clone(),
                            progress.stage.clone(),
                        )
                    });

                    match result.status.as_str() {
                        "ready" => {
                            *model_status.write().await = ModelStatus::Ready;
                            recording_controller.set_model_ready(true).await;
                            Self::emit_model_status_with_details(
                                &app_handle,
                                ModelStatus::Ready,
                                &event_seq,
                                result.model_id.clone(),
                                result.revision.clone(),
                                result.cache_path.clone(),
                                status_progress.clone(),
                            );
                            log::info!("Model ready for transcription");
                        }
                        "missing" | "not_found" | "error" => {
                            log::info!(
                                "Model not ready ({}), triggering initialization",
                                result.status
                            );
                            *model_status.write().await = ModelStatus::Missing;
                            Self::emit_model_status_with_details(
                                &app_handle,
                                ModelStatus::Missing,
                                &event_seq,
                                result.model_id.clone(),
                                result.revision.clone(),
                                result.cache_path.clone(),
                                status_progress.clone(),
                            );

                            // Trigger model initialization
                            Self::trigger_model_init(
                                &client,
                                &state_manager,
                                &recording_controller,
                                &model_status,
                                &app_handle,
                                &event_seq,
                            )
                            .await;
                        }
                        "downloading" | "loading" => {
                            // Already in progress (maybe from another session)
                            let status = if result.status == "downloading" {
                                ModelStatus::Downloading
                            } else {
                                ModelStatus::Loading
                            };
                            *model_status.write().await = status.clone();
                            Self::emit_model_status_with_details(
                                &app_handle,
                                status,
                                &event_seq,
                                result.model_id.clone(),
                                result.revision.clone(),
                                result.cache_path.clone(),
                                status_progress.clone(),
                            );
                            log::info!("Model {} in progress", result.status);
                        }
                        _ => {
                            log::warn!("Unknown model status: {}", result.status);
                            *model_status.write().await = ModelStatus::Unknown;
                            Self::emit_model_status_with_details(
                                &app_handle,
                                ModelStatus::Unknown,
                                &event_seq,
                                result.model_id.clone(),
                                result.revision.clone(),
                                result.cache_path.clone(),
                                status_progress.clone(),
                            );
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to get model status: {}", e);
                    // Don't block on this - user can trigger manually
                }
            }
        });
    }

    /// Trigger model initialization via sidecar.
    async fn trigger_model_init(
        client: &RpcClient,
        state_manager: &Arc<AppStateManager>,
        recording_controller: &Arc<RecordingController>,
        model_status: &Arc<RwLock<ModelStatus>>,
        app_handle: &Option<AppHandle>,
        event_seq: &Arc<AtomicU64>,
    ) {
        // Transition to loading state
        let _ = state_manager.transition(AppState::LoadingModel);
        *model_status.write().await = ModelStatus::Downloading;
        Self::emit_model_status(app_handle, ModelStatus::Downloading, event_seq);

        // Get configured model or use default
        let config = config::load_config();
        let model_id = config
            .model
            .as_ref()
            .and_then(|m| m.model_id.clone())
            .unwrap_or_else(|| model_defaults::default_model_id().to_string());

        let raw_device = config.model.as_ref().and_then(|m| m.device.as_deref());
        let raw_preferred = config
            .model
            .as_ref()
            .map(|m| m.preferred_device.as_str())
            .unwrap_or("auto");
        let device_pref = config.effective_model_device_pref();

        log::info!(
            "Resolved model device preference: model.device={:?}, model.preferred_device='{}', effective='{}'",
            raw_device,
            raw_preferred,
            device_pref
        );

        log::info!(
            "Initializing ASR model: model={}, device={}",
            model_id,
            device_pref
        );

        let params = json!({
            "model_id": model_id,
            "device_pref": device_pref
        });

        #[derive(Deserialize)]
        struct InitResult {
            status: String,
        }

        match client
            .call::<InitResult>("asr.initialize", Some(params))
            .await
        {
            Ok(result) => {
                log::info!("ASR initialization complete: status={}", result.status);
                *model_status.write().await = ModelStatus::Ready;
                recording_controller.set_model_ready(true).await;
                let _ = state_manager.transition(AppState::Idle);
                Self::emit_model_status(app_handle, ModelStatus::Ready, event_seq);
            }
            Err(e) => {
                log::error!("ASR initialization failed: {}", e);
                let error_msg = format!("Model initialization failed: {}", e);
                *model_status.write().await = ModelStatus::Error(error_msg.clone());
                state_manager.transition_to_error(error_msg.clone());
                Self::emit_model_status(app_handle, ModelStatus::Error(error_msg), event_seq);
            }
        }
    }

    /// Emit model status event to frontend.
    fn emit_model_status(
        app_handle: &Option<AppHandle>,
        status: ModelStatus,
        seq_counter: &Arc<AtomicU64>,
    ) {
        Self::emit_model_status_with_details(
            app_handle,
            status,
            seq_counter,
            None,
            None,
            None,
            None,
        );
    }

    /// Emit canonical model status event with optional metadata.
    fn emit_model_status_with_details(
        app_handle: &Option<AppHandle>,
        status: ModelStatus,
        seq_counter: &Arc<AtomicU64>,
        model_id: Option<String>,
        revision: Option<String>,
        cache_path: Option<String>,
        progress: Option<ModelStatusProgress>,
    ) {
        if let Some(ref handle) = app_handle {
            let (status_name, error) = model_status_to_event_fields(status);
            let payload = ModelStatusPayload {
                model_id: resolve_model_id(model_id),
                status: status_name,
                revision,
                cache_path,
                progress,
                error,
            };

            emit_with_shared_seq(handle, &[EVENT_MODEL_STATUS], json!(payload), seq_counter);
        }
    }

    /// Get current model status.
    pub async fn get_model_status(&self) -> ModelStatus {
        self.model_status.read().await.clone()
    }

    /// Manually trigger model download.
    pub async fn download_model(&self) -> Result<(), String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        Self::trigger_model_init(
            client,
            &self.state_manager,
            &self.recording_controller,
            &self.model_status,
            &self.app_handle,
            &self.event_seq,
        )
        .await;

        Ok(())
    }

    /// Purge model cache.
    pub async fn purge_model_cache(&self) -> Result<(), String> {
        let current_status = self.model_status.read().await.clone();
        if current_status == ModelStatus::Downloading || current_status == ModelStatus::Loading {
            return Err(
                "Cannot purge model while download or initialization is in progress".to_string(),
            );
        }

        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        #[derive(Deserialize)]
        struct PurgeResult {
            #[allow(dead_code)]
            purged: bool,
        }

        client
            .call::<PurgeResult>("model.purge_cache", None)
            .await
            .map_err(|e| format!("Failed to purge cache: {}", e))?;

        // Update status
        *self.model_status.write().await = ModelStatus::Missing;
        self.recording_controller.set_model_ready(false).await;
        Self::emit_model_status(&self.app_handle, ModelStatus::Missing, &self.event_seq);

        log::info!("Model cache purged");
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

        match client
            .call::<InitResult>("asr.initialize", Some(params))
            .await
        {
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
        let current_session_id = Arc::clone(&self.current_session_id);
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
                                        timing_marks: PipelineTimingMarks::default(),
                                    });
                                    *current_session_id.write().await = Some(session_id.clone());

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
                                    *current_session_id.write().await = None;
                                }
                            }
                        } else if let Some(RecordingAction::Stop) = recording_action {
                            // Toggle mode: stop recording
                            Self::stop_recording_flow(
                                &recording_controller,
                                &rpc_client,
                                &recording_context,
                                &current_session_id,
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
                                    &current_session_id,
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
        current_session_id: &Arc<RwLock<Option<String>>>,
    ) {
        match recording_controller.stop().await {
            Ok(result) => {
                log::info!("Recording stopped: {:?}", result);

                // Too-short recordings don't produce transcription and should clear session context.
                if matches!(result, StopResult::TooShort) {
                    *current_session_id.write().await = None;
                    *recording_context.write().await = None;
                    return;
                }

                // Tell sidecar to stop recording (triggers async transcription)
                if let Some(client) = rpc_client.read().await.as_ref() {
                    let stop_called_at = Instant::now();
                    let session_id = {
                        let mut ctx = recording_context.write().await;
                        if let Some(ctx) = ctx.as_mut() {
                            ctx.timing_marks.t0_stop_called = Some(stop_called_at);
                            Some(ctx.session_id.clone())
                        } else {
                            None
                        }
                    };

                    if let Some(session_id) = session_id {
                        let params = json!({
                            "session_id": session_id
                        });
                        let stop_result: Result<Value, _> =
                            client.call("recording.stop", Some(params)).await;

                        let stop_rpc_returned_at = Instant::now();
                        let mut ctx = recording_context.write().await;
                        if let Some(ctx) = ctx.as_mut() {
                            if ctx.session_id == session_id {
                                ctx.timing_marks.t1_stop_rpc_returned = Some(stop_rpc_returned_at);
                            }
                        }

                        if let Err(err) = stop_result {
                            log::warn!("Failed to call recording.stop RPC: {}", err);
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to stop recording: {}", e);
                *current_session_id.write().await = None;
            }
        }
    }

    /// Start state change event loop (for tray updates).
    fn start_state_loop(&self) {
        let state_manager = Arc::clone(&self.state_manager);
        let app_handle = self.app_handle.clone();
        let event_seq = Arc::clone(&self.event_seq);

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

                    emit_with_shared_seq(
                        handle,
                        &[EVENT_TRAY_UPDATE],
                        json!({
                            "icon": icon,
                            "state": event.state,
                            "enabled": event.enabled,
                            "detail": event.detail,
                        }),
                        &event_seq,
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
        let current_session_id = Arc::clone(&self.current_session_id);
        let app_handle = self.app_handle.clone();
        let event_seq = Arc::clone(&self.event_seq);

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
                        timestamp: _,
                    } => {
                        log::info!(
                            "Transcription complete: session={}, text_len={}, text_sha256_prefix={}, audio={}ms, processing={}ms",
                            session_id,
                            text.len(),
                            sha256_prefix(&text),
                            audio_duration_ms,
                            processing_duration_ms
                        );

                        if log::log_enabled!(log::Level::Debug) {
                            log::debug!("Transcription text (debug): {}", text);
                        }

                        // Snapshot focus context and timing marks for this session.
                        let (focus_before, mut timing_marks) = {
                            let ctx = recording_context.read().await;
                            if let Some(ctx) = ctx.as_ref() {
                                (Some(ctx.focus_before.clone()), ctx.timing_marks.clone())
                            } else {
                                (None, PipelineTimingMarks::default())
                            }
                        };
                        if timing_marks.t2_transcription_received.is_none() {
                            timing_marks.t2_transcription_received = Some(Instant::now());
                        }
                        let expected_focus = focus_before.as_ref();

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
                            app_overrides: config
                                .injection
                                .app_overrides
                                .iter()
                                .map(|(app_id, ov)| {
                                    (
                                        app_id.clone(),
                                        crate::injection::AppOverride {
                                            paste_delay_ms: ov.paste_delay_ms,
                                            use_clipboard_only: ov.use_clipboard_only,
                                        },
                                    )
                                })
                                .collect(),
                        };

                        timing_marks.t3_postprocess_completed = Some(Instant::now());

                        // Inject text
                        let result = inject_text(&text, expected_focus, &injection_config).await;
                        timing_marks.t4_injection_completed = Some(Instant::now());

                        let pipeline_timings = pipeline_timings_from_marks(&timing_marks);
                        if let Some(timings) = pipeline_timings.as_ref() {
                            log_pipeline_timings(timings);
                        }

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
                            let injection_result_for_history =
                                HistoryInjectionResult::from_injection_result(&result);
                            let mut entry = TranscriptEntry::new(
                                text.clone(),
                                audio_duration_ms as u32,
                                processing_duration_ms as u32,
                                injection_result_for_history,
                            );
                            if let Some(timings) = pipeline_timings.clone() {
                                entry = entry.with_timings(timings);
                            }
                            history.push(entry);
                        }

                        // Emit event to frontend
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_TRANSCRIPTION_COMPLETE],
                                json!({
                                    "session_id": session_id,
                                    "text": text,
                                    "audio_duration_ms": audio_duration_ms,
                                    "processing_duration_ms": processing_duration_ms,
                                    "injection_result": result,
                                    "timings": pipeline_timings,
                                }),
                                &event_seq,
                            );
                        }

                        // Clear context
                        *recording_context.write().await = None;
                        *current_session_id.write().await = None;
                    }
                    RecordingEvent::TranscriptionFailed {
                        session_id, error, ..
                    } => {
                        log::error!(
                            "Transcription failed: session={}, error_len={}, error_sha256_prefix={}",
                            session_id,
                            error.len(),
                            sha256_prefix(&error),
                        );
                        if log::log_enabled!(log::Level::Debug) {
                            log::debug!("Transcription error message (debug): {}", error);
                        }

                        if let Some(ref handle) = app_handle {
                            let app_error = AppError::new(
                                ErrorKind::TranscriptionFailed.to_sidecar(),
                                "Transcription failed",
                                Some(json!({
                                    "session_id": session_id,
                                    "error_kind": ErrorKind::TranscriptionFailed.to_sidecar(),
                                    "sidecar_message": error
                                })),
                                true,
                            );
                            let legacy_message = app_error.message.clone();
                            let legacy_recoverable = app_error.recoverable;
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_TRANSCRIPTION_ERROR, EVENT_APP_ERROR],
                                json!({
                                    "session_id": session_id,
                                    "message": legacy_message,
                                    "recoverable": legacy_recoverable,
                                    "error": app_error,
                                }),
                                &event_seq,
                            );
                        }

                        // Clear context
                        *recording_context.write().await = None;
                        *current_session_id.write().await = None;
                    }
                    RecordingEvent::Cancelled { .. } => {
                        *recording_context.write().await = None;
                        *current_session_id.write().await = None;
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
        let model_status = Arc::clone(&self.model_status);
        let model_progress = Arc::clone(&self.model_progress);
        let app_handle = self.app_handle.clone();
        let watchdog = Arc::clone(&self.watchdog);
        let recording_context = Arc::clone(&self.recording_context);
        let current_session_id = Arc::clone(&self.current_session_id);
        let event_seq = Arc::clone(&self.event_seq);

        tokio::spawn(async move {
            log::info!("Notification loop started");

            while let Ok(event) = receiver.recv().await {
                // Any notification means the sidecar is alive
                watchdog.mark_activity().await;

                log::debug!("Sidecar notification: method={}", event.method);

                // Drop stale session-scoped notifications from old sessions.
                let incoming_session_id = extract_session_id(&event.params);
                let active_session_id = current_session_id.read().await.clone();
                if is_stale_session(incoming_session_id, active_session_id.as_deref()) {
                    log::debug!(
                        "Dropping stale notification: method={} incoming_session_id={:?} active_session_id={:?}",
                        event.method,
                        incoming_session_id,
                        active_session_id
                    );
                    continue;
                }

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

                        if let Ok(params) =
                            serde_json::from_value::<TranscriptionParams>(event.params)
                        {
                            {
                                let transcription_received_at = Instant::now();
                                let mut ctx = recording_context.write().await;
                                if let Some(ctx) = ctx.as_mut() {
                                    if ctx.session_id == params.session_id {
                                        ctx.timing_marks.t2_transcription_received =
                                            Some(transcription_received_at);
                                    }
                                }
                            }

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
                        // Handle model progress updates
                        #[derive(Deserialize)]
                        struct StatusParams {
                            #[allow(dead_code)]
                            #[serde(default)]
                            state: Option<String>,
                            #[allow(dead_code)]
                            #[serde(default)]
                            detail: Option<String>,
                            #[serde(default)]
                            model: Option<Value>,
                            #[serde(default)]
                            progress: Option<ProgressParams>,
                        }

                        #[derive(Deserialize)]
                        struct ModelParams {
                            #[allow(dead_code)]
                            #[serde(default)]
                            model_id: Option<String>,
                            status: String,
                            #[serde(default)]
                            revision: Option<String>,
                            #[serde(default)]
                            cache_path: Option<String>,
                        }

                        #[derive(Deserialize)]
                        struct ProgressParams {
                            current: u64,
                            #[serde(default)]
                            total: Option<u64>,
                            #[serde(default)]
                            unit: Option<String>,
                            #[serde(default)]
                            stage: Option<String>,
                        }

                        if let Ok(params) =
                            serde_json::from_value::<StatusParams>(event.params.clone())
                        {
                            let parsed_model = params.model.as_ref().and_then(|model| {
                                serde_json::from_value::<ModelParams>(model.clone()).ok()
                            });

                            // Support both spec-compliant model object and legacy string model state.
                            let model_state = parsed_model
                                .as_ref()
                                .map(|parsed| parsed.status.clone())
                                .or_else(|| {
                                    params
                                        .model
                                        .as_ref()
                                        .and_then(|model| model.as_str().map(ToOwned::to_owned))
                                });

                            let status_progress = params.progress.as_ref().map(|progress| {
                                status_progress_from_parts(
                                    progress.current,
                                    progress.total,
                                    progress.unit.clone(),
                                    progress.stage.clone(),
                                )
                            });

                            // Update model status if provided
                            if let Some(model_state) = model_state {
                                let new_status = match model_state.as_str() {
                                    "downloading" => ModelStatus::Downloading,
                                    "loading" => ModelStatus::Loading,
                                    "ready" => ModelStatus::Ready,
                                    "missing" => ModelStatus::Missing,
                                    "error" => ModelStatus::Error(
                                        params
                                            .detail
                                            .clone()
                                            .unwrap_or_else(|| "model status error".to_string()),
                                    ),
                                    _ => ModelStatus::Unknown,
                                };
                                *model_status.write().await = new_status.clone();

                                if new_status == ModelStatus::Ready {
                                    recording_controller.set_model_ready(true).await;
                                } else {
                                    recording_controller.set_model_ready(false).await;
                                }

                                Self::emit_model_status_with_details(
                                    &app_handle,
                                    new_status,
                                    &event_seq,
                                    parsed_model.as_ref().and_then(|m| m.model_id.clone()),
                                    parsed_model.as_ref().and_then(|m| m.revision.clone()),
                                    parsed_model.as_ref().and_then(|m| m.cache_path.clone()),
                                    status_progress.clone(),
                                );
                            }

                            // Update and emit progress if provided
                            if let Some(ref progress) = params.progress {
                                let model_progress_data = ModelProgress {
                                    current: progress.current,
                                    total: progress.total,
                                    stage: progress
                                        .stage
                                        .clone()
                                        .or_else(|| progress.unit.clone())
                                        .unwrap_or_else(|| "processing".to_string()),
                                };
                                *model_progress.write().await = Some(model_progress_data.clone());

                                if let Some(ref handle) = app_handle {
                                    emit_with_shared_seq(
                                        handle,
                                        &[EVENT_MODEL_PROGRESS],
                                        json!(model_progress_data),
                                        &event_seq,
                                    );
                                }
                            }
                        }

                        // Also forward raw event to frontend
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_STATUS_CHANGED],
                                event.params,
                                &event_seq,
                            );
                        }
                    }
                    "event.audio_level" => {
                        #[derive(Deserialize)]
                        struct AudioLevelParams {
                            source: String,
                            rms: f64,
                            peak: f64,
                            #[serde(default)]
                            session_id: Option<String>,
                        }

                        if let Ok(params) = serde_json::from_value::<AudioLevelParams>(event.params)
                        {
                            // recording source is session-scoped and should include session_id.
                            if params.source == "recording" && params.session_id.is_none() {
                                log::warn!("Ignoring invalid audio_level event: missing session_id for recording source");
                                continue;
                            }

                            if let Some(ref handle) = app_handle {
                                emit_with_shared_seq(
                                    handle,
                                    &["audio:level"],
                                    json!({
                                        "source": params.source,
                                        "rms": params.rms,
                                        "peak": params.peak,
                                        "session_id": params.session_id,
                                    }),
                                    &event_seq,
                                );
                            }
                        } else {
                            log::warn!("Ignoring invalid audio_level payload");
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

            emit_with_shared_seq(
                handle,
                &[EVENT_TRAY_UPDATE],
                json!({
                    "icon": icon,
                    "state": state,
                }),
                &self.event_seq,
            );
        }
    }

    /// Shutdown all components.
    pub async fn shutdown(&self) {
        log::info!("Shutting down integration manager");

        // Shutdown watchdog first
        self.watchdog.shutdown();

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

    /// Get the watchdog for external monitoring.
    pub fn watchdog(&self) -> &Arc<Watchdog> {
        &self.watchdog
    }
}

/// RPC ping adapter for watchdog.
struct RpcPinger {
    rpc_client: Arc<RwLock<Option<RpcClient>>>,
}

impl PingCallback for RpcPinger {
    async fn ping(&self) -> Result<(), String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        #[derive(serde::Deserialize)]
        struct PingResult {
            #[allow(dead_code)]
            version: String,
        }

        client
            .call::<PingResult>("system.ping", None)
            .await
            .map(|_| ())
            .map_err(|e| format!("Ping failed: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_add_seq_to_object_payload() {
        let payload = json!({"state":"idle"});
        let with_seq = add_seq_to_payload(payload, 42);
        assert_eq!(with_seq["state"], "idle");
        assert_eq!(with_seq["seq"], 42);
    }

    #[test]
    fn test_add_seq_wraps_non_object_payload() {
        let payload = json!("value");
        let with_seq = add_seq_to_payload(payload, 7);
        assert_eq!(with_seq["seq"], 7);
        assert_eq!(with_seq["data"], "value");
    }

    #[test]
    fn test_event_seq_is_monotonic() {
        let counter = Arc::new(AtomicU64::new(1));
        let first = counter.fetch_add(1, Ordering::Relaxed);
        let second = counter.fetch_add(1, Ordering::Relaxed);
        assert_eq!(first, 1);
        assert_eq!(second, 2);
    }

    #[test]
    fn test_extract_session_id() {
        let payload = json!({
            "session_id": "11111111-1111-1111-1111-111111111111",
            "text": "hello"
        });
        assert_eq!(
            extract_session_id(&payload),
            Some("11111111-1111-1111-1111-111111111111")
        );
        assert_eq!(extract_session_id(&json!({"text": "no-session"})), None);
    }

    #[test]
    fn test_is_stale_session_logic() {
        assert!(!is_stale_session(None, Some("active")));
        assert!(!is_stale_session(Some("active"), Some("active")));
        assert!(is_stale_session(Some("old"), Some("active")));
        assert!(is_stale_session(Some("old"), None));
    }

    #[test]
    fn test_integration_config_default() {
        let config = IntegrationConfig::default();
        assert_eq!(config.python_path, "python3");
        assert_eq!(config.sidecar_module, "openvoicy_sidecar");
        assert!(config.auto_start_sidecar);
    }

    #[test]
    fn test_status_progress_from_parts_prefers_unit() {
        let progress = status_progress_from_parts(
            10,
            Some(100),
            Some("bytes".to_string()),
            Some("downloading".to_string()),
        );

        assert_eq!(progress.current, 10);
        assert_eq!(progress.total, Some(100));
        assert_eq!(progress.unit, "bytes");
    }

    #[test]
    fn test_status_progress_from_parts_uses_stage_fallback() {
        let progress =
            status_progress_from_parts(20, Some(200), None, Some("verifying".to_string()));

        assert_eq!(progress.current, 20);
        assert_eq!(progress.total, Some(200));
        assert_eq!(progress.unit, "verifying");
    }

    #[test]
    fn test_pipeline_timings_from_marks() {
        let base = Instant::now();
        let marks = PipelineTimingMarks {
            t0_stop_called: Some(base),
            t1_stop_rpc_returned: Some(base + Duration::from_millis(15)),
            t2_transcription_received: Some(base + Duration::from_millis(795)),
            t3_postprocess_completed: Some(base + Duration::from_millis(800)),
            t4_injection_completed: Some(base + Duration::from_millis(850)),
        };

        let timings = pipeline_timings_from_marks(&marks).expect("timings should exist");
        assert_eq!(timings.ipc_ms, Some(15));
        assert_eq!(timings.transcribe_ms, Some(780));
        assert_eq!(timings.postprocess_ms, Some(5));
        assert_eq!(timings.inject_ms, Some(50));
        assert_eq!(timings.total_ms, Some(850));
    }

    #[test]
    fn test_pipeline_timings_from_marks_none_when_missing() {
        let marks = PipelineTimingMarks::default();
        assert!(pipeline_timings_from_marks(&marks).is_none());
    }

    #[test]
    fn test_model_status_to_event_fields_maps_error() {
        let (status, error) = model_status_to_event_fields(ModelStatus::Error("boom".to_string()));
        assert_eq!(status, "error");
        assert_eq!(error, Some("boom".to_string()));
    }

    #[test]
    fn test_resolve_model_id_prefers_explicit_value() {
        assert_eq!(
            resolve_model_id(Some("custom/model".to_string())),
            "custom/model"
        );
    }

    #[test]
    fn test_configured_device_available_when_uid_present() {
        let devices = vec![
            AudioDeviceSummary {
                uid: "mic-a".to_string(),
            },
            AudioDeviceSummary {
                uid: "mic-b".to_string(),
            },
        ];
        assert!(is_configured_device_available(Some("mic-b"), &devices));
    }

    #[test]
    fn test_configured_device_unavailable_when_uid_missing() {
        let devices = vec![AudioDeviceSummary {
            uid: "mic-a".to_string(),
        }];
        assert!(!is_configured_device_available(Some("mic-z"), &devices));
    }

    #[tokio::test]
    async fn test_integration_manager_creation() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        assert!(manager.app_handle.is_none());
    }

    #[tokio::test]
    async fn test_integration_manager_has_watchdog() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        // Verify watchdog is initialized with default status
        let status = manager.watchdog.get_status().await;
        assert_eq!(status, crate::watchdog::HealthStatus::NotRunning);
    }
}
