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
use std::time::{Duration, Instant};

use global_hotkey::GlobalHotKeyEvent;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::{self, HotkeyMode, ReplacementRule};
use crate::errors::{AppError, ErrorKind};
use crate::focus::{capture_focus, FocusSignature};
use crate::history::{
    HistoryInjectionResult, TranscriptEntry, TranscriptHistory, TranscriptTimings,
};
use crate::hotkey::{HotkeyAction, HotkeyManager, RecordingAction};
use crate::injection::{inject_text, InjectionConfig, InjectionResult};
use crate::ipc::{NotificationEvent, RpcClient, RpcError};
use crate::model_defaults;
use crate::recording::{
    CancelReason, RecordingController, RecordingEvent, StopResult, TranscriptionResult,
};
use crate::state::{AppState, AppStateManager, StateEvent};
use crate::watchdog::{self, PingCallback, Watchdog, WatchdogConfig, WatchdogEvent};

/// Tray icon event name.
const EVENT_TRAY_UPDATE: &str = "tray:update";
/// Canonical app state change event.
const EVENT_STATE_CHANGED: &str = "state:changed";
/// Legacy app state change event alias.
const EVENT_STATE_CHANGED_LEGACY: &str = "state_changed";

/// Model progress event name.
const EVENT_MODEL_PROGRESS: &str = "model:progress";

/// Model status event name.
const EVENT_MODEL_STATUS: &str = "model:status";

/// Canonical sidecar status event name.
const EVENT_SIDECAR_STATUS: &str = "sidecar:status";
/// Canonical recording phase event name.
const EVENT_RECORDING_STATUS: &str = "recording:status";

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

/// Sidecar `model.get_status` payload.
#[derive(Debug, Clone, Deserialize)]
pub struct SidecarModelStatus {
    pub model_id: String,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub cache_path: Option<String>,
    pub status: String,
    #[serde(default)]
    pub progress: Option<SidecarModelProgress>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
}

/// Sidecar model download/verification progress payload.
#[derive(Debug, Clone, Deserialize)]
pub struct SidecarModelProgress {
    pub current: u64,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub unit: Option<String>,
}

/// Status changed event name (mirrors sidecar event).
const EVENT_STATUS_CHANGED: &str = "status:changed";

/// Canonical transcription complete event name.
const EVENT_TRANSCRIPT_COMPLETE: &str = "transcript:complete";
/// Legacy transcription complete event alias.
const EVENT_TRANSCRIPTION_COMPLETE: &str = "transcription:complete";

/// Canonical transcription error event name.
const EVENT_TRANSCRIPT_ERROR: &str = "transcript:error";
/// Transcription error event name.
/// Legacy alias retained for compatibility.
const EVENT_TRANSCRIPTION_ERROR: &str = "transcription:error";
/// Application error event name (legacy + structured compatibility payload).
const EVENT_APP_ERROR: &str = "app:error";
const AUDIO_LEVEL_METER_MIN_INTERVAL_MS: u64 = 34; // <=30Hz
const AUDIO_LEVEL_NON_METER_MIN_INTERVAL_MS: u64 = 67; // <=15Hz

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

fn model_download_params(model_id: Option<String>, force: Option<bool>) -> Option<Value> {
    let mut params = serde_json::Map::new();
    if let Some(model_id) = model_id {
        let trimmed = model_id.trim();
        if !trimmed.is_empty() {
            params.insert("model_id".to_string(), json!(trimmed));
        }
    }
    if let Some(force) = force {
        params.insert("force".to_string(), json!(force));
    }

    if params.is_empty() {
        None
    } else {
        Some(Value::Object(params))
    }
}

fn map_download_response_status(status: &SidecarModelStatus) -> ModelStatus {
    match status.status.as_str() {
        "missing" => ModelStatus::Missing,
        "downloading" => ModelStatus::Downloading,
        "loading" | "verifying" => ModelStatus::Loading,
        "ready" => ModelStatus::Ready,
        "error" => ModelStatus::Error(
            status
                .error
                .clone()
                .or_else(|| status.error_message.clone())
                .unwrap_or_else(|| "model download failed".to_string()),
        ),
        _ => ModelStatus::Unknown,
    }
}

fn model_download_method_unsupported_message() -> String {
    "E_METHOD_NOT_FOUND: Sidecar does not support model.download or model.install".to_string()
}

fn map_model_download_rpc_error(error: RpcError) -> String {
    match error {
        RpcError::Remote { kind, message, .. } => match kind.as_str() {
            "E_NETWORK" => {
                format!("E_NETWORK: {message}. Check your network connection and retry.")
            }
            "E_DISK_FULL" => {
                format!("E_DISK_FULL: {message}. Free disk space and retry the download.")
            }
            "E_CACHE_CORRUPT" => {
                format!("E_CACHE_CORRUPT: {message}. Purge model cache, then retry.")
            }
            "E_METHOD_NOT_FOUND" => model_download_method_unsupported_message(),
            _ if !kind.is_empty() => format!("{kind}: {message}"),
            _ => format!("E_MODEL_DOWNLOAD: {message}"),
        },
        RpcError::Timeout { .. } => {
            "E_MODEL_DOWNLOAD: Model download timed out. Retry and keep the app running while the model installs."
                .to_string()
        }
        RpcError::Disconnected => "E_SIDECAR_IPC: Sidecar not connected".to_string(),
        other => format!("E_MODEL_DOWNLOAD: Failed to download model: {other}"),
    }
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

fn model_status_event_payload(
    status: ModelStatus,
    model_id: Option<String>,
    revision: Option<String>,
    cache_path: Option<String>,
    progress: Option<ModelStatusProgress>,
) -> ModelStatusPayload {
    let (status_name, error) = model_status_to_event_fields(status);
    ModelStatusPayload {
        model_id: resolve_model_id(model_id),
        status: status_name,
        revision,
        cache_path,
        progress,
        error,
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

/// Audio device payload returned by sidecar `audio.list_devices`.
#[derive(Debug, Clone, Deserialize)]
pub struct SidecarAudioDevice {
    pub uid: String,
    pub name: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub default_sample_rate: u32,
    #[serde(default)]
    pub channels: u32,
}

/// Preset metadata payload returned by sidecar replacements APIs.
#[derive(Debug, Clone, Deserialize)]
pub struct SidecarPresetInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub rule_count: usize,
}

/// Replacement preview payload returned by sidecar `replacements.preview`.
#[derive(Debug, Clone, Deserialize)]
pub struct SidecarReplacementPreviewResult {
    pub result: String,
    pub truncated: bool,
    #[serde(default)]
    pub applied_rules_count: Option<usize>,
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

fn stale_notification_message(
    notification_session_id: Option<&str>,
    active_session_id: Option<&str>,
) -> String {
    format!(
        "Dropping stale notification: session_id={}, current={}",
        notification_session_id.unwrap_or("<none>"),
        active_session_id.unwrap_or("<none>")
    )
}

fn map_transcription_complete_durations(
    notification_duration_ms: u64,
    stop_audio_duration_ms: Option<u64>,
) -> (u64, u64) {
    let audio_duration_ms = stop_audio_duration_ms.unwrap_or(0);
    let processing_duration_ms = notification_duration_ms;
    (audio_duration_ms, processing_duration_ms)
}

fn stop_rpc_method_for_result(result: &StopResult) -> &'static str {
    match result {
        StopResult::TooShort => "recording.cancel",
        StopResult::Transcribing { .. } => "recording.stop",
    }
}

fn has_transcription_timed_out(
    stop_called_at: Option<Instant>,
    transcription_timeout: Duration,
    now: Instant,
) -> bool {
    stop_called_at
        .map(|started| now.duration_since(started) >= transcription_timeout)
        .unwrap_or(false)
}

fn startup_model_status_requires_loading_state(status: &str) -> bool {
    matches!(status, "downloading" | "loading" | "verifying")
}

fn map_status_event_model_state(model_state: &str, detail: Option<String>) -> ModelStatus {
    match model_state {
        "downloading" => ModelStatus::Downloading,
        "loading" | "verifying" => ModelStatus::Loading,
        "ready" => ModelStatus::Ready,
        "missing" => ModelStatus::Missing,
        "error" => ModelStatus::Error(detail.unwrap_or_else(|| "model status error".to_string())),
        _ => ModelStatus::Unknown,
    }
}

fn infer_sidecar_state_from_detail(detail: Option<&str>) -> Option<&'static str> {
    let normalized = detail?.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    if normalized.contains("restart") {
        return Some("restarting");
    }

    if normalized.contains("failed") || normalized.contains("crash") || normalized.contains("error")
    {
        return Some("failed");
    }

    if normalized.contains("stop") || normalized.contains("shutdown") {
        return Some("stopped");
    }

    None
}

fn map_status_event_sidecar_state(state: Option<&str>, detail: Option<&str>) -> &'static str {
    let normalized_state = state
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());

    match normalized_state.as_deref() {
        Some("starting") => "starting",
        Some("ready") | Some("running") => "ready",
        Some("failed") | Some("error") => "failed",
        Some("restarting") => "restarting",
        Some("stopped") | Some("stopping") | Some("shutdown") | Some("shutting_down") => "stopped",
        Some("idle") | Some("loading_model") | Some("recording") | Some("transcribing") => "ready",
        Some(_) | None => infer_sidecar_state_from_detail(detail).unwrap_or("ready"),
    }
}

fn sidecar_status_payload_from_status_event(
    state: Option<&str>,
    detail: Option<String>,
    restart_count: Option<u32>,
) -> Value {
    let normalized_detail = detail.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });

    let sidecar_state = map_status_event_sidecar_state(state, normalized_detail.as_deref());
    let mut payload = json!({
        "state": sidecar_state,
        "restart_count": restart_count.unwrap_or(0),
    });

    if matches!(sidecar_state, "failed" | "restarting") {
        if let Some(message) = normalized_detail {
            if let Some(object) = payload.as_object_mut() {
                object.insert("message".to_string(), json!(message));
            }
        }
    }

    payload
}

fn transcription_error_event_payload(session_id: &str, app_error: &AppError) -> Value {
    json!({
        "session_id": session_id,
        // Canonical structured payload consumed by transcript:error.
        "error": app_error,
        // Compatibility fields retained for one release cycle.
        "message": app_error.message,
        "recoverable": app_error.recoverable,
        // Legacy structured alias consumed by existing app:error handler.
        "app_error": app_error,
    })
}

fn app_error_event_payload(app_error: &AppError) -> Value {
    json!({
        "error": app_error,
        "message": app_error.message,
        "recoverable": app_error.recoverable,
    })
}

fn transcript_complete_event_payload(entry: &TranscriptEntry) -> Value {
    json!({
        "entry": entry
    })
}

fn state_changed_event_payload(event: &StateEvent) -> Value {
    json!({
        "state": event.state,
        "enabled": event.enabled,
        "detail": event.detail,
        "timestamp": event.timestamp.to_rfc3339(),
    })
}

fn recording_status_event_payload(
    phase: &str,
    session_id: Option<&str>,
    started_at: Option<String>,
    audio_ms: Option<u64>,
) -> Value {
    let mut payload = json!({ "phase": phase });
    if let Some(session_id) = session_id {
        if let Some(object) = payload.as_object_mut() {
            object.insert("session_id".to_string(), json!(session_id));
        }
    }
    if let Some(started_at) = started_at {
        if let Some(object) = payload.as_object_mut() {
            object.insert("started_at".to_string(), json!(started_at));
        }
    }
    if let Some(audio_ms) = audio_ms {
        if let Some(object) = payload.as_object_mut() {
            object.insert("audio_ms".to_string(), json!(audio_ms));
        }
    }
    payload
}

fn should_emit_audio_level(
    now: Instant,
    last_emitted_at: &mut Option<Instant>,
    min_interval: Duration,
) -> bool {
    if let Some(last) = *last_emitted_at {
        if now.duration_since(last) < min_interval {
            return false;
        }
    }

    *last_emitted_at = Some(now);
    true
}

fn canonical_transcription_error_kind(sidecar_kind: &str) -> String {
    let normalized = sidecar_kind.trim();
    if normalized.is_empty() {
        return ErrorKind::TranscriptionFailed.to_sidecar().to_string();
    }

    if let Some(mapped) = ErrorKind::from_sidecar(normalized) {
        return mapped.to_sidecar().to_string();
    }

    if normalized.starts_with("E_") {
        // Preserve explicit sidecar E_* kinds even if not yet in host catalog.
        return normalized.to_string();
    }

    match normalized.replace('-', "_").to_ascii_uppercase().as_str() {
        "ASR_FAILED" | "TRANSCRIPTION_FAILED" | "TRANSCRIBE_FAILED" | "ASR_ERROR" => {
            ErrorKind::TranscriptionFailed.to_sidecar().to_string()
        }
        "ASR_TIMEOUT" | "TRANSCRIPTION_TIMEOUT" | "TIMEOUT" => {
            ErrorKind::TranscriptionTimeout.to_sidecar().to_string()
        }
        _ => ErrorKind::TranscriptionFailed.to_sidecar().to_string(),
    }
}

fn parse_sidecar_transcription_error(raw_error: &str) -> (String, String) {
    if let Some((kind_part, message_part)) = raw_error.split_once(':') {
        let kind = kind_part.trim();
        let message = message_part.trim_start();
        if !kind.is_empty() && !message.is_empty() {
            return (
                canonical_transcription_error_kind(kind),
                message.to_string(),
            );
        }
    }

    (
        ErrorKind::TranscriptionFailed.to_sidecar().to_string(),
        raw_error.to_string(),
    )
}

fn transcription_failure_app_error(session_id: &str, raw_error: &str) -> AppError {
    let (sidecar_error_kind, sidecar_message) = parse_sidecar_transcription_error(raw_error);
    AppError::new(
        ErrorKind::TranscriptionFailed.to_sidecar(),
        "Transcription failed",
        Some(json!({
            "session_id": session_id,
            "error_kind": sidecar_error_kind,
            "sidecar_message": sidecar_message
        })),
        true,
    )
}

fn validate_recording_start_response(
    expected_session_id: &str,
    response: &Value,
) -> Result<(), String> {
    #[derive(Deserialize)]
    struct StartResult {
        session_id: String,
    }

    let start_result: StartResult = serde_json::from_value(response.clone())
        .map_err(|e| format!("Invalid recording.start response payload: {e}"))?;
    if start_result.session_id == expected_session_id {
        Ok(())
    } else {
        Err(format!(
            "recording.start session mismatch: host={} sidecar={}",
            expected_session_id, start_result.session_id
        ))
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

trait AppEventBroadcaster {
    fn emit_all(&self, event: &str, payload: Value);
}

impl AppEventBroadcaster for AppHandle {
    fn emit_all(&self, event: &str, payload: Value) {
        let _ = self.emit(event, payload);
    }
}

fn emit_with_existing_seq_to_all_windows<B: AppEventBroadcaster>(
    broadcaster: &B,
    event: &str,
    payload: Value,
    seq: u64,
) {
    broadcaster.emit_all(event, add_seq_to_payload(payload, seq));
}

fn emit_with_shared_seq_for_broadcaster<B: AppEventBroadcaster>(
    broadcaster: &B,
    events: &[&str],
    payload: Value,
    seq_counter: &Arc<AtomicU64>,
) -> u64 {
    let seq = next_seq(seq_counter);
    for event in events {
        emit_with_existing_seq_to_all_windows(broadcaster, event, payload.clone(), seq);
    }
    seq
}

fn emit_with_shared_seq(
    handle: &AppHandle,
    events: &[&str],
    payload: Value,
    seq_counter: &Arc<AtomicU64>,
) -> u64 {
    emit_with_shared_seq_for_broadcaster(handle, events, payload, seq_counter)
}

fn next_seq(seq_counter: &Arc<AtomicU64>) -> u64 {
    seq_counter.fetch_add(1, Ordering::Relaxed)
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
    /// Audio duration returned by recording.stop, if available.
    audio_duration_ms: Option<u64>,
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
                                    match result.status.as_str() {
                                        "ready" => {
                                            recording_controller.set_model_ready(true).await;
                                            *model_status.write().await = ModelStatus::Ready;
                                        }
                                        "downloading" => {
                                            recording_controller.set_model_ready(false).await;
                                            *model_status.write().await = ModelStatus::Downloading;
                                        }
                                        "loading" | "verifying" => {
                                            recording_controller.set_model_ready(false).await;
                                            *model_status.write().await = ModelStatus::Loading;
                                        }
                                        _ => {
                                            recording_controller.set_model_ready(false).await;
                                            *model_status.write().await = ModelStatus::Missing;
                                        }
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
                            let _ = state_manager.transition(AppState::Idle);
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
                        "downloading" | "loading" | "verifying" => {
                            // Already in progress (maybe from another session)
                            debug_assert!(startup_model_status_requires_loading_state(
                                result.status.as_str()
                            ));
                            let _ = state_manager.transition(AppState::LoadingModel);
                            recording_controller.set_model_ready(false).await;
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
                            log::info!(
                                "Model {} in progress; app state set to loading_model",
                                result.status
                            );
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
            let payload =
                model_status_event_payload(status, model_id, revision, cache_path, progress);

            emit_with_shared_seq(handle, &[EVENT_MODEL_STATUS], json!(payload), seq_counter);
        }
    }

    /// Get current model status.
    pub async fn get_model_status(&self) -> ModelStatus {
        self.model_status.read().await.clone()
    }

    /// Query live model status from sidecar RPC.
    pub async fn query_model_status(
        &self,
        model_id: Option<String>,
    ) -> Result<SidecarModelStatus, String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "E_SIDECAR_IPC: Sidecar not connected".to_string())?;

        let params = model_id.map(|id| json!({ "model_id": id }));
        client
            .call::<SidecarModelStatus>("model.get_status", params)
            .await
            .map_err(|e| format!("E_SIDECAR_IPC: Failed to query model status: {}", e))
    }

    /// Manually trigger model download.
    pub async fn download_model(
        &self,
        model_id: Option<String>,
        force: Option<bool>,
    ) -> Result<(), String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "E_SIDECAR_IPC: Sidecar not connected".to_string())?;

        let _ = self.state_manager.transition(AppState::LoadingModel);
        *self.model_status.write().await = ModelStatus::Downloading;
        self.recording_controller.set_model_ready(false).await;
        Self::emit_model_status(&self.app_handle, ModelStatus::Downloading, &self.event_seq);

        let params = model_download_params(model_id, force);
        let status_result = match client
            .call::<SidecarModelStatus>("model.download", params.clone())
            .await
        {
            Ok(status) => Ok(status),
            Err(RpcError::Remote { kind, .. }) if kind == "E_METHOD_NOT_FOUND" => {
                match client
                    .call::<SidecarModelStatus>("model.install", params.clone())
                    .await
                {
                    Ok(status) => Ok(status),
                    Err(RpcError::Remote { kind, .. }) if kind == "E_METHOD_NOT_FOUND" => {
                        Err(model_download_method_unsupported_message())
                    }
                    Err(err) => Err(map_model_download_rpc_error(err)),
                }
            }
            Err(err) => Err(map_model_download_rpc_error(err)),
        };

        match status_result {
            Ok(status) => {
                let mapped_status = map_download_response_status(&status);
                *self.model_status.write().await = mapped_status.clone();
                self.recording_controller
                    .set_model_ready(matches!(mapped_status, ModelStatus::Ready))
                    .await;

                if matches!(mapped_status, ModelStatus::Ready) {
                    let _ = self.state_manager.transition(AppState::Idle);
                }

                let status_progress = status.progress.as_ref().map(|progress| {
                    status_progress_from_parts(
                        progress.current,
                        progress.total,
                        progress.unit.clone(),
                        None,
                    )
                });

                Self::emit_model_status_with_details(
                    &self.app_handle,
                    mapped_status,
                    &self.event_seq,
                    Some(resolve_model_id(Some(status.model_id.clone()))),
                    status.revision.clone(),
                    status.cache_path.clone(),
                    status_progress.clone(),
                );

                if let (Some(progress), Some(handle)) = (status.progress, self.app_handle.as_ref())
                {
                    let model_progress_data = ModelProgress {
                        current: progress.current,
                        total: progress.total,
                        stage: progress.unit.unwrap_or_else(|| "bytes".to_string()),
                    };
                    *self.model_progress.write().await = Some(model_progress_data.clone());
                    emit_with_shared_seq(
                        handle,
                        &[EVENT_MODEL_PROGRESS],
                        json!(model_progress_data),
                        &self.event_seq,
                    );
                }

                Ok(())
            }
            Err(message) => {
                let error_status = ModelStatus::Error(message.clone());
                *self.model_status.write().await = error_status.clone();
                self.recording_controller.set_model_ready(false).await;
                self.state_manager.transition_to_error(message.clone());
                Self::emit_model_status(&self.app_handle, error_status, &self.event_seq);
                Err(message)
            }
        }
    }

    /// Purge model cache.
    pub async fn purge_model_cache(&self, model_id: Option<String>) -> Result<(), String> {
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

        let purge_model_id = model_id.and_then(|id| {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

        let params = purge_model_id
            .as_ref()
            .map(|requested_model_id| json!({ "model_id": requested_model_id }));

        client
            .call::<PurgeResult>("model.purge_cache", params)
            .await
            .map_err(|e| format!("Failed to purge cache: {}", e))?;

        let configured_model_id = configured_model_id();
        let affects_configured_model = purge_model_id
            .as_deref()
            .map_or(true, |requested| requested == configured_model_id.as_str());

        if affects_configured_model {
            *self.model_status.write().await = ModelStatus::Missing;
            self.recording_controller.set_model_ready(false).await;
        }

        // Emit missing status for the purged model target.
        Self::emit_model_status_with_details(
            &self.app_handle,
            ModelStatus::Missing,
            &self.event_seq,
            Some(
                purge_model_id
                    .as_ref()
                    .cloned()
                    .unwrap_or(configured_model_id),
            ),
            None,
            None,
            None,
        );

        log::info!(
            "Model cache purged{}",
            purge_model_id
                .as_ref()
                .map(|id| format!(" for model {}", id))
                .unwrap_or_default()
        );
        Ok(())
    }

    /// Start microphone level meter via sidecar.
    pub async fn start_mic_test(&self, device_uid: Option<String>) -> Result<(), String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        #[derive(Deserialize)]
        struct MeterStartResult {
            #[allow(dead_code)]
            running: bool,
            #[allow(dead_code)]
            interval_ms: u64,
        }

        let params = json!({
            "device_uid": device_uid,
            "interval_ms": 80u64
        });

        client
            .call::<MeterStartResult>("audio.meter_start", Some(params))
            .await
            .map_err(|e| format!("Failed to start mic test: {}", e))?;
        Ok(())
    }

    /// List available audio input devices via sidecar.
    pub async fn list_audio_devices(&self) -> Result<Vec<SidecarAudioDevice>, String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        #[derive(Deserialize)]
        struct ListDevicesResult {
            #[serde(default)]
            devices: Vec<SidecarAudioDevice>,
        }

        let result = client
            .call::<ListDevicesResult>("audio.list_devices", None)
            .await
            .map_err(|e| format!("Failed to list audio devices: {}", e))?;

        Ok(result.devices)
    }

    /// Set active audio input device via sidecar.
    pub async fn set_audio_device(
        &self,
        device_uid: Option<String>,
    ) -> Result<Option<String>, String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        #[derive(Deserialize)]
        struct SetDeviceResult {
            #[serde(default)]
            active_device_uid: Option<String>,
        }

        let params = json!({
            "device_uid": device_uid
        });

        let result = client
            .call::<SetDeviceResult>("audio.set_device", Some(params))
            .await
            .map_err(|e| format!("Failed to set audio device: {}", e))?;

        Ok(result.active_device_uid)
    }

    /// List available replacement presets via sidecar.
    pub async fn list_replacement_presets(&self) -> Result<Vec<SidecarPresetInfo>, String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        #[derive(Deserialize)]
        struct GetPresetsResult {
            #[serde(default)]
            presets: Vec<SidecarPresetInfo>,
        }

        let result = client
            .call::<GetPresetsResult>("replacements.get_presets", None)
            .await
            .map_err(|e| format!("Failed to list presets: {}", e))?;

        Ok(result.presets)
    }

    /// Get replacement rules for a specific preset via sidecar.
    ///
    /// Returns `Ok(None)` when the preset does not exist (`E_NOT_FOUND`).
    pub async fn get_preset_replacement_rules(
        &self,
        preset_id: String,
    ) -> Result<Option<Vec<ReplacementRule>>, String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        #[derive(Deserialize)]
        struct GetPresetRulesResult {
            #[allow(dead_code)]
            preset: SidecarPresetInfo,
            #[serde(default)]
            rules: Vec<ReplacementRule>,
        }

        let params = json!({ "preset_id": preset_id });

        match client
            .call::<GetPresetRulesResult>("replacements.get_preset_rules", Some(params))
            .await
        {
            Ok(result) => Ok(Some(result.rules)),
            Err(RpcError::Remote { kind, .. }) if kind == "E_NOT_FOUND" => Ok(None),
            Err(e) => Err(format!("Failed to load preset rules: {}", e)),
        }
    }

    /// Get current active replacement rules from sidecar.
    pub async fn get_active_replacement_rules(&self) -> Result<Vec<ReplacementRule>, String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        #[derive(Deserialize)]
        struct GetRulesResult {
            #[serde(default)]
            rules: Vec<ReplacementRule>,
        }

        let result = client
            .call::<GetRulesResult>("replacements.get_rules", None)
            .await
            .map_err(|e| format!("Failed to get active replacement rules: {}", e))?;

        Ok(result.rules)
    }

    /// Replace active replacement rules in sidecar.
    pub async fn set_active_replacement_rules(
        &self,
        rules: Vec<ReplacementRule>,
    ) -> Result<(), String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        #[derive(Deserialize)]
        struct SetRulesResult {
            #[allow(dead_code)]
            count: usize,
        }

        let params = json!({ "rules": rules });
        client
            .call::<SetRulesResult>("replacements.set_rules", Some(params))
            .await
            .map_err(|e| format!("Failed to set active replacement rules: {}", e))?;

        Ok(())
    }

    /// Preview text using sidecar's replacements pipeline.
    pub async fn preview_replacement(
        &self,
        text: String,
        rules: Vec<ReplacementRule>,
    ) -> Result<SidecarReplacementPreviewResult, String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        let params = json!({
            "text": text,
            "rules": rules,
        });

        client
            .call::<SidecarReplacementPreviewResult>("replacements.preview", Some(params))
            .await
            .map_err(|e| format!("Failed to preview replacements: {}", e))
    }

    /// Stop microphone level meter via sidecar.
    pub async fn stop_mic_test(&self) -> Result<(), String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        #[derive(Deserialize)]
        struct MeterStopResult {
            #[allow(dead_code)]
            stopped: bool,
        }

        client
            .call::<MeterStopResult>("audio.meter_stop", None)
            .await
            .map_err(|e| format!("Failed to stop mic test: {}", e))?;
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
                Ok(())
            }
            Err(e) => {
                log::error!("ASR initialization failed: {}", e);
                self.state_manager
                    .transition_to_error(format!("Model initialization failed: {}", e));
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

            loop {
                // Drain global hotkey events and forward them into the hotkey action channel.
                {
                    let hk = hotkey_manager.read().await;
                    while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                        hk.process_event(event);
                    }
                }

                Self::enforce_runtime_limits(
                    &state_manager,
                    &recording_controller,
                    &rpc_client,
                    &recording_context,
                    &current_session_id,
                )
                .await;

                let action =
                    match tokio::time::timeout(Duration::from_millis(25), receiver.recv()).await {
                        Ok(Some(action)) => action,
                        Ok(None) => break,
                        Err(_) => continue,
                    };

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
                                        audio_duration_ms: None,
                                        timing_marks: PipelineTimingMarks::default(),
                                    });
                                    *current_session_id.write().await = Some(session_id.clone());

                                    // Tell sidecar to start recording
                                    if let Some(client) = rpc_client.read().await.as_ref() {
                                        let expected_session_id = session_id.clone();
                                        let params = json!({
                                            "session_id": expected_session_id,
                                            "device_uid": config.audio.device_uid
                                        });
                                        let start_result: Result<Value, _> =
                                            client.call("recording.start", Some(params)).await;
                                        let (start_synchronized, cancel_session_id) =
                                            match start_result {
                                                Ok(result) => {
                                                    match validate_recording_start_response(
                                                        expected_session_id.as_str(),
                                                        &result,
                                                    ) {
                                                        Ok(()) => (true, None),
                                                        Err(mismatch) => {
                                                            log::error!("{}", mismatch);
                                                            (
                                                                false,
                                                                result
                                                                    .get("session_id")
                                                                    .and_then(Value::as_str)
                                                                    .map(ToOwned::to_owned)
                                                                    .or_else(|| {
                                                                        Some(
                                                                            expected_session_id
                                                                                .clone(),
                                                                        )
                                                                    }),
                                                            )
                                                        }
                                                    }
                                                }
                                                Err(err) => {
                                                    log::error!(
                                                        "Failed to call recording.start RPC: {}",
                                                        err
                                                    );
                                                    (false, Some(expected_session_id.clone()))
                                                }
                                            };

                                        if !start_synchronized {
                                            if let Some(cancel_session_id) = cancel_session_id {
                                                let cancel_params =
                                                    json!({ "session_id": cancel_session_id });
                                                let cancel_result: Result<Value, _> = client
                                                    .call("recording.cancel", Some(cancel_params))
                                                    .await;
                                                if let Err(err) = cancel_result {
                                                    log::warn!(
                                                        "Failed to roll back recording.start via recording.cancel: {}",
                                                        err
                                                    );
                                                }
                                            }
                                            let _ = recording_controller
                                                .cancel(CancelReason::UserButton)
                                                .await;
                                            *recording_context.write().await = None;
                                            *current_session_id.write().await = None;
                                            state_manager.transition_to_error(
                                                "Recording session synchronization failed"
                                                    .to_string(),
                                            );
                                        }
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
                Self::complete_stop_recording_flow(
                    result,
                    rpc_client,
                    recording_context,
                    current_session_id,
                )
                .await;
            }
            Err(e) => {
                log::warn!("Failed to stop recording: {}", e);
                *current_session_id.write().await = None;
            }
        }
    }

    async fn enforce_runtime_limits(
        state_manager: &Arc<AppStateManager>,
        recording_controller: &Arc<RecordingController>,
        rpc_client: &Arc<RwLock<Option<RpcClient>>>,
        recording_context: &Arc<RwLock<Option<RecordingContext>>>,
        current_session_id: &Arc<RwLock<Option<String>>>,
    ) {
        if state_manager.get() == AppState::Recording {
            if let Some(stop_result) = recording_controller.check_max_duration().await {
                log::info!("Max recording duration reached; stopping recording");
                Self::complete_stop_recording_flow(
                    stop_result,
                    rpc_client,
                    recording_context,
                    current_session_id,
                )
                .await;
            }
            return;
        }

        if state_manager.get() != AppState::Transcribing {
            return;
        }

        let transcription_timeout = recording_controller
            .get_config()
            .await
            .transcription_timeout;
        let timed_out = {
            let ctx = recording_context.read().await;
            let stop_called_at = ctx.as_ref().and_then(|ctx| ctx.timing_marks.t0_stop_called);
            has_transcription_timed_out(stop_called_at, transcription_timeout, Instant::now())
        };

        if timed_out {
            log::warn!(
                "Transcription timed out after {:?}; transitioning to error",
                transcription_timeout
            );
            recording_controller.on_transcription_timeout().await;
            *current_session_id.write().await = None;
            *recording_context.write().await = None;
        }
    }

    async fn complete_stop_recording_flow(
        result: StopResult,
        rpc_client: &Arc<RwLock<Option<RpcClient>>>,
        recording_context: &Arc<RwLock<Option<RecordingContext>>>,
        current_session_id: &Arc<RwLock<Option<String>>>,
    ) {
        let stop_rpc_method = stop_rpc_method_for_result(&result);
        let too_short = matches!(result, StopResult::TooShort);

        // Tell sidecar to stop/cancel recording.
        if let Some(client) = rpc_client.read().await.as_ref() {
            let stop_called_at = Instant::now();
            let session_id = {
                let mut ctx = recording_context.write().await;
                if let Some(ctx) = ctx.as_mut() {
                    if !too_short {
                        ctx.timing_marks.t0_stop_called = Some(stop_called_at);
                    }
                    Some(ctx.session_id.clone())
                } else {
                    None
                }
            };

            if let Some(session_id) = session_id {
                let params = json!({
                    "session_id": session_id
                });
                if stop_rpc_method == "recording.stop" {
                    #[derive(Deserialize)]
                    struct StopResultPayload {
                        audio_duration_ms: u64,
                    }

                    let stop_result: Result<StopResultPayload, _> =
                        client.call(stop_rpc_method, Some(params)).await;

                    let stop_rpc_returned_at = Instant::now();
                    let mut ctx = recording_context.write().await;
                    if let Some(ctx) = ctx.as_mut() {
                        if ctx.session_id == session_id {
                            ctx.timing_marks.t1_stop_rpc_returned = Some(stop_rpc_returned_at);
                            if let Ok(stop_payload) = &stop_result {
                                ctx.audio_duration_ms = Some(stop_payload.audio_duration_ms);
                            }
                        }
                    }
                    if let Err(err) = stop_result {
                        log::warn!("Failed to call {} RPC: {}", stop_rpc_method, err);
                    }
                } else {
                    let cancel_result: Result<Value, _> =
                        client.call(stop_rpc_method, Some(params)).await;
                    if let Err(err) = cancel_result {
                        log::warn!("Failed to call {} RPC: {}", stop_rpc_method, err);
                    }
                }
            }
        }

        // Too-short recordings don't produce transcription and should clear session context.
        if too_short {
            *current_session_id.write().await = None;
            *recording_context.write().await = None;
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

                    emit_with_shared_seq(
                        handle,
                        &[EVENT_STATE_CHANGED, EVENT_STATE_CHANGED_LEGACY],
                        state_changed_event_payload(&event),
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
                    RecordingEvent::Started {
                        session_id,
                        timestamp,
                    } => {
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_RECORDING_STATUS],
                                recording_status_event_payload(
                                    "recording",
                                    Some(session_id.as_str()),
                                    Some(timestamp.to_rfc3339()),
                                    None,
                                ),
                                &event_seq,
                            );
                        }
                    }
                    RecordingEvent::Stopped {
                        session_id,
                        duration_ms,
                        ..
                    } => {
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_RECORDING_STATUS],
                                recording_status_event_payload(
                                    "transcribing",
                                    Some(session_id.as_str()),
                                    None,
                                    Some(duration_ms),
                                ),
                                &event_seq,
                            );
                        }
                    }
                    RecordingEvent::TooShort { .. } => {
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_RECORDING_STATUS],
                                recording_status_event_payload("idle", None, None, None),
                                &event_seq,
                            );
                        }
                    }
                    RecordingEvent::TranscriptionComplete {
                        session_id,
                        text,
                        audio_duration_ms,
                        processing_duration_ms,
                        timestamp: _,
                    } => {
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_RECORDING_STATUS],
                                recording_status_event_payload("idle", None, None, None),
                                &event_seq,
                            );
                        }

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

                        // Add to history and emit a shared transcript payload.
                        let mut transcript_entry = TranscriptEntry::new(
                            text.clone(),
                            audio_duration_ms as u32,
                            processing_duration_ms as u32,
                            HistoryInjectionResult::from_injection_result(&result),
                        )
                        .with_session_id(Uuid::parse_str(&session_id).ok());
                        if let Some(timings) = pipeline_timings.clone() {
                            transcript_entry = transcript_entry.with_timings(timings);
                        }

                        // Add to history
                        if let Some(ref handle) = app_handle {
                            let history = handle.state::<TranscriptHistory>();
                            history.push(transcript_entry.clone());
                        }

                        // Emit canonical + legacy events with identical payload and shared seq.
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_TRANSCRIPT_COMPLETE, EVENT_TRANSCRIPTION_COMPLETE],
                                transcript_complete_event_payload(&transcript_entry),
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
                            let app_error =
                                transcription_failure_app_error(&session_id, error.as_str());
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_TRANSCRIPT_ERROR, EVENT_TRANSCRIPTION_ERROR],
                                transcription_error_event_payload(&session_id, &app_error),
                                &event_seq,
                            );
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_APP_ERROR],
                                app_error_event_payload(&app_error),
                                &event_seq,
                            );
                        }

                        // Clear context
                        *recording_context.write().await = None;
                        *current_session_id.write().await = None;
                    }
                    RecordingEvent::Cancelled { .. } => {
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_RECORDING_STATUS],
                                recording_status_event_payload("idle", None, None, None),
                                &event_seq,
                            );
                        }
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
            let mut last_meter_audio_emit_at: Option<Instant> = None;
            let mut last_non_meter_audio_emit_at: Option<Instant> = None;

            while let Ok(event) = receiver.recv().await {
                // Any notification means the sidecar is alive
                watchdog.mark_activity().await;

                log::debug!("Sidecar notification: method={}", event.method);

                match event.method.as_str() {
                    "event.transcription_complete" => {
                        let incoming_session_id = extract_session_id(&event.params);
                        let active_session_id = current_session_id.read().await.clone();
                        if is_stale_session(incoming_session_id, active_session_id.as_deref()) {
                            log::warn!(
                                "{}",
                                stale_notification_message(
                                    incoming_session_id,
                                    active_session_id.as_deref()
                                )
                            );
                            continue;
                        }

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
                            let (audio_duration_ms, processing_duration_ms) = {
                                let mut stop_audio_duration_ms = None;
                                let mut ctx = recording_context.write().await;
                                if let Some(ctx) = ctx.as_mut() {
                                    if ctx.session_id == params.session_id {
                                        ctx.timing_marks.t2_transcription_received =
                                            Some(Instant::now());
                                        stop_audio_duration_ms = ctx.audio_duration_ms;
                                    }
                                }
                                map_transcription_complete_durations(
                                    params.duration_ms,
                                    stop_audio_duration_ms,
                                )
                            };

                            let result = TranscriptionResult {
                                session_id: params.session_id,
                                text: params.text,
                                audio_duration_ms,
                                processing_duration_ms,
                            };

                            // Deliver to recording controller (validates session ID)
                            recording_controller.on_transcription_result(result).await;
                        }
                    }
                    "event.transcription_error" => {
                        let incoming_session_id = extract_session_id(&event.params);
                        let active_session_id = current_session_id.read().await.clone();
                        if is_stale_session(incoming_session_id, active_session_id.as_deref()) {
                            log::warn!(
                                "{}",
                                stale_notification_message(
                                    incoming_session_id,
                                    active_session_id.as_deref()
                                )
                            );
                            continue;
                        }

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
                            #[serde(default)]
                            restart_count: Option<u32>,
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

                        let mut canonical_sidecar_payload =
                            sidecar_status_payload_from_status_event(None, None, None);

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
                                let new_status = map_status_event_model_state(
                                    model_state.as_str(),
                                    params.detail.clone(),
                                );
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

                            canonical_sidecar_payload = sidecar_status_payload_from_status_event(
                                params.state.as_deref(),
                                params.detail.clone(),
                                params.restart_count,
                            );
                        }

                        // Emit canonical sidecar status and keep forwarding legacy raw payload.
                        if let Some(ref handle) = app_handle {
                            let seq = next_seq(&event_seq);
                            emit_with_existing_seq_to_all_windows(
                                handle,
                                EVENT_SIDECAR_STATUS,
                                canonical_sidecar_payload,
                                seq,
                            );
                            emit_with_existing_seq_to_all_windows(
                                handle,
                                EVENT_STATUS_CHANGED,
                                event.params,
                                seq,
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

                            if params.source == "recording" {
                                let active_session_id = current_session_id.read().await.clone();
                                let incoming_session_id = params.session_id.as_deref();
                                if is_stale_session(
                                    incoming_session_id,
                                    active_session_id.as_deref(),
                                ) {
                                    log::warn!(
                                        "{}",
                                        stale_notification_message(
                                            incoming_session_id,
                                            active_session_id.as_deref()
                                        )
                                    );
                                    continue;
                                }
                            }

                            let now = Instant::now();
                            let should_emit = if params.source == "meter" {
                                should_emit_audio_level(
                                    now,
                                    &mut last_meter_audio_emit_at,
                                    Duration::from_millis(AUDIO_LEVEL_METER_MIN_INTERVAL_MS),
                                )
                            } else {
                                should_emit_audio_level(
                                    now,
                                    &mut last_non_meter_audio_emit_at,
                                    Duration::from_millis(AUDIO_LEVEL_NON_METER_MIN_INTERVAL_MS),
                                )
                            };
                            if !should_emit {
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
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Clone, Default)]
    struct MockBroadcaster {
        windows: Arc<Vec<String>>,
        received: Arc<Mutex<HashMap<String, Vec<(String, Value)>>>>,
    }

    impl MockBroadcaster {
        fn with_windows(labels: &[&str]) -> Self {
            let mut initial = HashMap::new();
            for label in labels {
                initial.insert((*label).to_string(), Vec::new());
            }
            Self {
                windows: Arc::new(labels.iter().map(|label| (*label).to_string()).collect()),
                received: Arc::new(Mutex::new(initial)),
            }
        }

        fn received_event_names(&self, label: &str) -> Vec<String> {
            self.received
                .lock()
                .expect("mock receiver lock poisoned")
                .get(label)
                .map(|events| {
                    events
                        .iter()
                        .map(|(event, _payload)| event.clone())
                        .collect()
                })
                .unwrap_or_default()
        }

        fn received_payloads(&self, label: &str) -> Vec<Value> {
            self.received
                .lock()
                .expect("mock receiver lock poisoned")
                .get(label)
                .map(|events| {
                    events
                        .iter()
                        .map(|(_event, payload)| payload.clone())
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    impl AppEventBroadcaster for MockBroadcaster {
        fn emit_all(&self, event: &str, payload: Value) {
            let mut guard = self.received.lock().expect("mock receiver lock poisoned");
            for label in self.windows.iter() {
                guard
                    .entry(label.clone())
                    .or_default()
                    .push((event.to_string(), payload.clone()));
            }
        }
    }

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
        let first = next_seq(&counter);
        let second = next_seq(&counter);
        assert_eq!(first, 1);
        assert_eq!(second, 2);
    }

    #[test]
    fn test_emit_all_main_only() {
        let broadcaster = MockBroadcaster::with_windows(&["main"]);
        let seq_counter = Arc::new(AtomicU64::new(1));

        println!("[EMIT_TEST] Emitting state:changed to all windows...");
        emit_with_shared_seq_for_broadcaster(
            &broadcaster,
            &[EVENT_STATE_CHANGED],
            json!({ "state": "idle" }),
            &seq_counter,
        );

        let main_events = broadcaster.received_event_names("main");
        println!(
            "[EMIT_TEST] Main window received: {}",
            if main_events.is_empty() { "✗" } else { "✓" }
        );
        assert_eq!(main_events, vec![EVENT_STATE_CHANGED.to_string()]);
    }

    #[test]
    fn test_emit_all_main_and_overlay() {
        let broadcaster = MockBroadcaster::with_windows(&["main", "overlay"]);
        let seq_counter = Arc::new(AtomicU64::new(1));

        println!("[EMIT_TEST] Emitting state:changed to all windows...");
        emit_with_shared_seq_for_broadcaster(
            &broadcaster,
            &[EVENT_STATE_CHANGED],
            json!({ "state": "idle" }),
            &seq_counter,
        );

        let main_events = broadcaster.received_event_names("main");
        let overlay_events = broadcaster.received_event_names("overlay");
        println!(
            "[EMIT_TEST] Main window received: {}",
            if main_events.is_empty() { "✗" } else { "✓" }
        );
        println!(
            "[EMIT_TEST] Overlay window received: {}",
            if overlay_events.is_empty() {
                "✗"
            } else {
                "✓"
            }
        );

        assert_eq!(main_events, vec![EVENT_STATE_CHANGED.to_string()]);
        assert_eq!(overlay_events, vec![EVENT_STATE_CHANGED.to_string()]);
    }

    #[test]
    fn test_emit_all_overlay_disabled() {
        let broadcaster = MockBroadcaster::with_windows(&["main"]);
        let seq_counter = Arc::new(AtomicU64::new(1));

        println!("[EMIT_TEST] Emitting recording:status to all windows...");
        emit_with_shared_seq_for_broadcaster(
            &broadcaster,
            &[EVENT_RECORDING_STATUS],
            json!({ "phase": "idle" }),
            &seq_counter,
        );

        let main_events = broadcaster.received_event_names("main");
        let overlay_events = broadcaster.received_event_names("overlay");
        println!(
            "[EMIT_TEST] Main window received: {}",
            if main_events.is_empty() { "✗" } else { "✓" }
        );
        println!("[EMIT_TEST] Overlay window received: N/A (disabled)");

        assert_eq!(main_events, vec![EVENT_RECORDING_STATUS.to_string()]);
        assert!(overlay_events.is_empty());
    }

    #[test]
    fn test_emit_all_covers_all_event_types() {
        let broadcaster = MockBroadcaster::with_windows(&["main", "overlay", "future"]);
        let seq_counter = Arc::new(AtomicU64::new(1));

        let events = [
            EVENT_STATE_CHANGED,
            EVENT_RECORDING_STATUS,
            EVENT_MODEL_STATUS,
            EVENT_MODEL_PROGRESS,
            EVENT_TRANSCRIPT_COMPLETE,
            EVENT_TRANSCRIPT_ERROR,
            EVENT_APP_ERROR,
            EVENT_SIDECAR_STATUS,
        ];
        emit_with_shared_seq_for_broadcaster(
            &broadcaster,
            &events,
            json!({ "sample": true }),
            &seq_counter,
        );

        for label in ["main", "overlay", "future"] {
            let received = broadcaster.received_event_names(label);
            assert_eq!(
                received,
                events
                    .iter()
                    .map(|event| (*event).to_string())
                    .collect::<Vec<_>>()
            );
            let payloads = broadcaster.received_payloads(label);
            assert_eq!(payloads.len(), events.len());
            for payload in payloads {
                assert_eq!(payload.get("seq").and_then(Value::as_u64), Some(1));
            }
        }
    }

    #[test]
    fn test_no_window_specific_emit_calls_for_app_events() {
        let source = include_str!("integration.rs");
        let forbidden = [".emit", "_to("].concat();
        assert!(!source.contains(&forbidden));
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
    fn test_validate_recording_start_response_accepts_matching_session_id() {
        let result =
            validate_recording_start_response("session-1", &json!({ "session_id": "session-1" }));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_recording_start_response_rejects_mismatched_session_id() {
        let result = validate_recording_start_response(
            "session-host",
            &json!({ "session_id": "session-sidecar" }),
        );
        let message = result.expect_err("expected mismatch error");
        assert!(message.contains("recording.start session mismatch"));
        assert!(message.contains("session-host"));
        assert!(message.contains("session-sidecar"));
    }

    #[test]
    fn test_validate_recording_start_response_rejects_invalid_payload() {
        let result = validate_recording_start_response("session-1", &json!({}));
        let message = result.expect_err("expected invalid payload error");
        assert!(message.contains("Invalid recording.start response payload"));
    }

    #[test]
    fn test_is_stale_session_logic() {
        assert!(!is_stale_session(None, Some("active")));
        assert!(!is_stale_session(Some("active"), Some("active")));
        assert!(is_stale_session(Some("old"), Some("active")));
        assert!(is_stale_session(Some("old"), None));
    }

    #[test]
    fn test_stale_notification_message_format() {
        assert_eq!(
            stale_notification_message(Some("old-session"), Some("active-session")),
            "Dropping stale notification: session_id=old-session, current=active-session"
        );
    }

    #[test]
    fn test_stale_transcription_dropped() {
        let active_session = Some("session-a");
        let incoming_session = Some("session-b");
        assert!(is_stale_session(incoming_session, active_session));
    }

    #[test]
    fn test_matching_transcription_accepted() {
        let active_session = Some("session-a");
        let incoming_session = Some("session-a");
        assert!(!is_stale_session(incoming_session, active_session));
    }

    #[test]
    fn test_stale_audio_level_dropped() {
        let active_session = Some("session-a");
        let incoming_session = Some("session-b");
        assert!(is_stale_session(incoming_session, active_session));
    }

    #[test]
    fn test_non_session_event_always_forwarded() {
        // Non-session scoped events (for example status updates) do not carry session ids.
        assert!(!is_stale_session(None, Some("session-a")));
        assert!(!is_stale_session(None, None));
    }

    #[test]
    fn test_transition_old_session_dropped() {
        let old_session = Some("session-a");
        let new_session = Some("session-b");
        assert!(is_stale_session(old_session, new_session));
    }

    #[test]
    fn test_between_sessions_dropped() {
        // Late event from previous session while no active session is tracked should be dropped.
        assert!(is_stale_session(Some("session-a"), None));
    }

    #[test]
    fn test_new_session_clears_old() {
        let old_session = Some("session-a");
        let new_session = Some("session-b");
        assert!(is_stale_session(old_session, new_session));
        assert!(!is_stale_session(new_session, new_session));
    }

    #[test]
    fn test_stale_transcription_error_dropped() {
        let active_session = Some("session-a");
        let incoming_session = Some("session-b");
        assert!(is_stale_session(incoming_session, active_session));
    }

    #[test]
    fn test_matching_transcription_error_accepted() {
        let active_session = Some("session-a");
        let incoming_session = Some("session-a");
        assert!(!is_stale_session(incoming_session, active_session));
    }

    #[test]
    fn test_rapid_session_turnover_accepts_only_latest_session_events() {
        let active_session = Some("session-c");
        let incoming_sessions = [
            Some("session-a"),
            Some("session-b"),
            Some("session-c"),
            Some("session-a"),
            Some("session-c"),
        ];
        let accepted: Vec<bool> = incoming_sessions
            .iter()
            .map(|incoming| !is_stale_session(*incoming, active_session))
            .collect();
        assert_eq!(accepted, vec![false, false, true, false, true]);
    }

    #[test]
    fn test_no_session_active_drops_session_scoped_notifications() {
        assert!(is_stale_session(Some("session-a"), None));
    }

    #[test]
    fn test_audio_level_meter_mode_not_session_scoped() {
        // Meter events are not session-scoped; they should not be considered stale.
        assert!(!is_stale_session(None, Some("session-a")));
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
    fn test_model_download_params_omits_empty_model_id_and_optional_force() {
        assert!(model_download_params(None, None).is_none());
        assert_eq!(model_download_params(Some("   ".to_string()), None), None);
        assert_eq!(
            model_download_params(Some("parakeet".to_string()), Some(true)),
            Some(json!({
                "model_id": "parakeet",
                "force": true
            }))
        );
    }

    #[test]
    fn test_map_model_download_rpc_error_network_is_actionable() {
        let error = RpcError::Remote {
            code: -32011,
            message: "download failed".to_string(),
            kind: "E_NETWORK".to_string(),
        };

        let mapped = map_model_download_rpc_error(error);
        assert!(mapped.contains("E_NETWORK"));
        assert!(mapped.contains("network connection"));
    }

    #[test]
    fn test_map_model_download_rpc_error_disk_full_is_actionable() {
        let error = RpcError::Remote {
            code: -32013,
            message: "not enough disk".to_string(),
            kind: "E_DISK_FULL".to_string(),
        };

        let mapped = map_model_download_rpc_error(error);
        assert!(mapped.contains("E_DISK_FULL"));
        assert!(mapped.contains("Free disk space"));
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
    fn test_model_status_event_payload_includes_explicit_model_metadata() {
        let progress = ModelStatusProgress {
            current: 10,
            total: Some(100),
            unit: "bytes".to_string(),
        };

        let payload = model_status_event_payload(
            ModelStatus::Loading,
            Some("custom/model".to_string()),
            Some("r1".to_string()),
            Some("/tmp/model-cache".to_string()),
            Some(progress.clone()),
        );

        assert_eq!(payload.model_id, "custom/model");
        assert_eq!(payload.status, "loading");
        assert_eq!(payload.revision.as_deref(), Some("r1"));
        assert_eq!(payload.cache_path.as_deref(), Some("/tmp/model-cache"));
        assert_eq!(
            payload.progress.as_ref().map(|p| p.current),
            Some(progress.current)
        );
        assert_eq!(
            payload.progress.as_ref().and_then(|p| p.total),
            progress.total
        );
        assert_eq!(
            payload.progress.as_ref().map(|p| p.unit.as_str()),
            Some(progress.unit.as_str())
        );
        assert!(payload.error.is_none());
    }

    #[test]
    fn test_model_status_event_payload_falls_back_to_configured_model_id() {
        let payload = model_status_event_payload(ModelStatus::Ready, None, None, None, None);
        assert_eq!(payload.model_id, configured_model_id());
        assert_eq!(payload.status, "ready");
        assert!(payload.error.is_none());
    }

    #[test]
    fn test_model_status_event_payload_maps_error() {
        let payload = model_status_event_payload(
            ModelStatus::Error("download failed".to_string()),
            Some("custom/model".to_string()),
            None,
            None,
            None,
        );
        assert_eq!(payload.model_id, "custom/model");
        assert_eq!(payload.status, "error");
        assert_eq!(payload.error.as_deref(), Some("download failed"));
    }

    #[test]
    fn test_resolve_model_id_prefers_explicit_value() {
        assert_eq!(
            resolve_model_id(Some("custom/model".to_string())),
            "custom/model"
        );
    }

    #[test]
    fn test_transcription_complete_duration_mapping_prefers_stop_audio_duration() {
        let (audio_duration_ms, processing_duration_ms) =
            map_transcription_complete_durations(420, Some(1337));
        assert_eq!(audio_duration_ms, 1337);
        assert_eq!(processing_duration_ms, 420);
    }

    #[test]
    fn test_transcription_complete_duration_mapping_defaults_audio_duration_to_zero() {
        let (audio_duration_ms, processing_duration_ms) =
            map_transcription_complete_durations(420, None);
        assert_eq!(audio_duration_ms, 0);
        assert_eq!(processing_duration_ms, 420);
    }

    #[test]
    fn test_stop_rpc_method_for_result_routes_too_short_to_cancel() {
        assert_eq!(
            stop_rpc_method_for_result(&StopResult::TooShort),
            "recording.cancel"
        );
        assert_eq!(
            stop_rpc_method_for_result(&StopResult::Transcribing {
                session_id: "session".to_string()
            }),
            "recording.stop"
        );
    }

    #[test]
    fn test_has_transcription_timed_out_when_elapsed_reaches_timeout() {
        let base = Instant::now();
        assert!(has_transcription_timed_out(
            Some(base),
            Duration::from_millis(250),
            base + Duration::from_millis(250)
        ));
    }

    #[test]
    fn test_has_transcription_timed_out_false_without_stop_mark() {
        let now = Instant::now();
        assert!(!has_transcription_timed_out(
            None,
            Duration::from_millis(250),
            now
        ));
    }

    #[test]
    fn test_startup_model_status_requires_loading_state_for_downloading_and_loading() {
        assert!(startup_model_status_requires_loading_state("downloading"));
        assert!(startup_model_status_requires_loading_state("loading"));
        assert!(startup_model_status_requires_loading_state("verifying"));
        assert!(!startup_model_status_requires_loading_state("ready"));
        assert!(!startup_model_status_requires_loading_state("missing"));
    }

    #[test]
    fn test_map_status_event_model_state_maps_verifying_to_loading() {
        assert_eq!(
            map_status_event_model_state("verifying", None),
            ModelStatus::Loading
        );
    }

    #[test]
    fn test_map_status_event_sidecar_state_maps_runtime_states_to_ready() {
        assert_eq!(map_status_event_sidecar_state(Some("idle"), None), "ready");
        assert_eq!(
            map_status_event_sidecar_state(Some("recording"), None),
            "ready"
        );
        assert_eq!(
            map_status_event_sidecar_state(Some("transcribing"), None),
            "ready"
        );
    }

    #[test]
    fn test_map_status_event_sidecar_state_uses_error_and_restart_signals() {
        assert_eq!(
            map_status_event_sidecar_state(Some("error"), Some("decoder crash")),
            "failed"
        );
        assert_eq!(
            map_status_event_sidecar_state(Some("unknown"), Some("restarting sidecar")),
            "restarting"
        );
    }

    #[test]
    fn test_sidecar_status_payload_from_status_event_includes_message_for_failed() {
        let payload = sidecar_status_payload_from_status_event(
            Some("error"),
            Some("Crash loop detected".to_string()),
            Some(3),
        );

        assert_eq!(payload.get("state").and_then(Value::as_str), Some("failed"));
        assert_eq!(
            payload.get("restart_count").and_then(Value::as_u64),
            Some(3)
        );
        assert_eq!(
            payload.get("message").and_then(Value::as_str),
            Some("Crash loop detected")
        );
    }

    #[test]
    fn test_sidecar_status_payload_from_status_event_omits_message_when_ready() {
        let payload =
            sidecar_status_payload_from_status_event(Some("idle"), Some("Ready".to_string()), None);

        assert_eq!(payload.get("state").and_then(Value::as_str), Some("ready"));
        assert_eq!(
            payload.get("restart_count").and_then(Value::as_u64),
            Some(0)
        );
        assert!(payload.get("message").is_none());
    }

    #[test]
    fn test_transcription_error_event_payload_preserves_legacy_error_string() {
        let app_error = AppError::new(
            ErrorKind::TranscriptionFailed.to_sidecar(),
            "Transcription failed",
            Some(json!({
                "session_id": "session-1",
                "error_kind": ErrorKind::TranscriptionFailed.to_sidecar()
            })),
            true,
        );

        let payload = transcription_error_event_payload("session-1", &app_error);
        assert_eq!(
            payload.pointer("/error/code").and_then(Value::as_str),
            Some("E_TRANSCRIPTION_FAILED")
        );
        assert_eq!(
            payload.get("message").and_then(Value::as_str),
            Some("Transcription failed")
        );
        assert_eq!(
            payload.get("recoverable").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload.pointer("/app_error/code").and_then(Value::as_str),
            Some("E_TRANSCRIPTION_FAILED")
        );
    }

    #[test]
    fn test_transcription_error_event_payload_includes_structured_and_legacy_fields() {
        let app_error = AppError::new(
            ErrorKind::SidecarCrash.to_sidecar(),
            "Sidecar crashed",
            Some(json!({
                "restart_count": 2,
                "source": "watchdog"
            })),
            false,
        );

        let payload = transcription_error_event_payload("session-1", &app_error);

        // Legacy flat compatibility fields.
        assert_eq!(
            payload.get("message").and_then(Value::as_str),
            Some("Sidecar crashed")
        );
        assert_eq!(
            payload.get("recoverable").and_then(Value::as_bool),
            Some(false)
        );

        // Structured error shape for canonical consumers.
        assert_eq!(
            payload.pointer("/error/code").and_then(Value::as_str),
            Some("E_SIDECAR_CRASH")
        );
        assert_eq!(
            payload.pointer("/error/message").and_then(Value::as_str),
            Some("Sidecar crashed")
        );
        assert_eq!(
            payload
                .pointer("/error/recoverable")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            payload
                .pointer("/error/details/restart_count")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            payload
                .pointer("/error/details/source")
                .and_then(Value::as_str),
            Some("watchdog")
        );

        // Legacy structured alias retained during migration window.
        assert_eq!(
            payload.pointer("/app_error/code").and_then(Value::as_str),
            Some("E_SIDECAR_CRASH")
        );
    }

    #[test]
    fn test_app_error_event_payload_matches_contract_shape() {
        let app_error = AppError::new(
            ErrorKind::TranscriptionFailed.to_sidecar(),
            "Transcription failed",
            Some(json!({
                "session_id": "session-1",
                "error_kind": ErrorKind::TranscriptionFailed.to_sidecar()
            })),
            true,
        );

        let payload = app_error_event_payload(&app_error);

        assert_eq!(
            payload.pointer("/error/code").and_then(Value::as_str),
            Some("E_TRANSCRIPTION_FAILED")
        );
        assert_eq!(
            payload.get("message").and_then(Value::as_str),
            Some("Transcription failed")
        );
        assert_eq!(
            payload.get("recoverable").and_then(Value::as_bool),
            Some(true)
        );
        assert!(payload.get("session_id").is_none());
        assert!(payload.get("app_error").is_none());
    }

    #[test]
    fn test_state_changed_event_payload_shape() {
        let event = StateEvent {
            state: AppState::Idle,
            enabled: true,
            detail: None,
            timestamp: chrono::Utc::now(),
        };

        let payload = state_changed_event_payload(&event);
        assert_eq!(payload.get("state").and_then(Value::as_str), Some("idle"));
        assert_eq!(payload.get("enabled").and_then(Value::as_bool), Some(true));
        assert!(payload.get("detail").is_some());
        assert!(payload.get("timestamp").and_then(Value::as_str).is_some());
    }

    #[test]
    fn test_transcript_complete_event_payload_wraps_entry() {
        let session_id = Uuid::new_v4();
        let entry = TranscriptEntry::new(
            "hello".to_string(),
            1200,
            340,
            HistoryInjectionResult::Injected,
        )
        .with_session_id(Some(session_id));

        let payload = transcript_complete_event_payload(&entry);
        assert!(payload.get("entry").is_some());
        assert_eq!(
            payload
                .get("entry")
                .and_then(|entry| entry.get("session_id"))
                .and_then(Value::as_str),
            Some(session_id.to_string().as_str())
        );
    }

    #[test]
    fn test_recording_status_event_payload_recording_includes_session_and_started_at() {
        let payload = recording_status_event_payload(
            "recording",
            Some("session-1"),
            Some("2026-02-19T03:00:00Z".to_string()),
            None,
        );

        assert_eq!(
            payload.get("phase").and_then(Value::as_str),
            Some("recording")
        );
        assert_eq!(
            payload.get("session_id").and_then(Value::as_str),
            Some("session-1")
        );
        assert_eq!(
            payload.get("started_at").and_then(Value::as_str),
            Some("2026-02-19T03:00:00Z")
        );
        assert!(payload.get("audio_ms").is_none());
    }

    #[test]
    fn test_recording_status_event_payload_transcribing_includes_audio_ms() {
        let payload =
            recording_status_event_payload("transcribing", Some("session-2"), None, Some(1234));

        assert_eq!(
            payload.get("phase").and_then(Value::as_str),
            Some("transcribing")
        );
        assert_eq!(
            payload.get("session_id").and_then(Value::as_str),
            Some("session-2")
        );
        assert_eq!(payload.get("audio_ms").and_then(Value::as_u64), Some(1234));
        assert!(payload.get("started_at").is_none());
    }

    #[test]
    fn test_recording_status_event_payload_idle_has_no_optional_fields() {
        let payload = recording_status_event_payload("idle", None, None, None);

        assert_eq!(payload.get("phase").and_then(Value::as_str), Some("idle"));
        assert!(payload.get("session_id").is_none());
        assert!(payload.get("started_at").is_none());
        assert!(payload.get("audio_ms").is_none());
    }

    #[test]
    fn test_transcription_failure_app_error_preserves_sidecar_error_kind() {
        let app_error =
            transcription_failure_app_error("session-1", "E_ASR_INIT: model initialization failed");

        assert_eq!(app_error.code, "E_TRANSCRIPTION_FAILED");
        assert_eq!(
            app_error
                .details
                .as_ref()
                .and_then(|d| d.get("error_kind"))
                .and_then(Value::as_str),
            Some("E_ASR_INIT")
        );
        assert_eq!(
            app_error
                .details
                .as_ref()
                .and_then(|d| d.get("sidecar_message"))
                .and_then(Value::as_str),
            Some("model initialization failed")
        );
    }

    #[test]
    fn test_transcription_failure_app_error_falls_back_to_canonical_kind() {
        let app_error = transcription_failure_app_error("session-1", "sidecar timeout");

        assert_eq!(app_error.code, "E_TRANSCRIPTION_FAILED");
        assert_eq!(
            app_error
                .details
                .as_ref()
                .and_then(|d| d.get("error_kind"))
                .and_then(Value::as_str),
            Some("E_TRANSCRIPTION_FAILED")
        );
        assert_eq!(
            app_error
                .details
                .as_ref()
                .and_then(|d| d.get("sidecar_message"))
                .and_then(Value::as_str),
            Some("sidecar timeout")
        );
    }

    #[test]
    fn test_parse_sidecar_transcription_error_maps_asr_failed_to_canonical_code() {
        let (kind, message) = parse_sidecar_transcription_error("asr_failed: decoder crashed");
        assert_eq!(kind, "E_TRANSCRIPTION_FAILED");
        assert_eq!(message, "decoder crashed");
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

    #[test]
    fn test_should_emit_audio_level_throttles_meter_source() {
        let min_interval = Duration::from_millis(AUDIO_LEVEL_METER_MIN_INTERVAL_MS);
        let start = Instant::now();
        let mut last = None;

        assert!(should_emit_audio_level(start, &mut last, min_interval));
        assert!(!should_emit_audio_level(
            start + Duration::from_millis(5),
            &mut last,
            min_interval
        ));
        assert!(should_emit_audio_level(
            start + Duration::from_millis(40),
            &mut last,
            min_interval
        ));
    }

    #[test]
    fn test_should_emit_audio_level_throttles_non_meter_sources_to_15hz() {
        let min_interval = Duration::from_millis(AUDIO_LEVEL_NON_METER_MIN_INTERVAL_MS);
        let start = Instant::now();
        let mut last = None;

        assert!(should_emit_audio_level(start, &mut last, min_interval));
        assert!(!should_emit_audio_level(
            start + Duration::from_millis(40),
            &mut last,
            min_interval
        ));
        assert!(should_emit_audio_level(
            start + Duration::from_millis(90),
            &mut last,
            min_interval
        ));
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

    #[tokio::test]
    async fn test_start_mic_test_requires_sidecar_connection() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        let error = manager
            .start_mic_test(Some("device-1".to_string()))
            .await
            .expect_err("start_mic_test should fail without sidecar");
        assert!(error.contains("Sidecar not connected"));
    }

    #[tokio::test]
    async fn test_list_audio_devices_requires_sidecar_connection() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        let error = manager
            .list_audio_devices()
            .await
            .expect_err("list_audio_devices should fail without sidecar");
        assert!(error.contains("Sidecar not connected"));
    }

    #[tokio::test]
    async fn test_set_audio_device_requires_sidecar_connection() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        let error = manager
            .set_audio_device(Some("device-1".to_string()))
            .await
            .expect_err("set_audio_device should fail without sidecar");
        assert!(error.contains("Sidecar not connected"));
    }

    #[tokio::test]
    async fn test_stop_mic_test_requires_sidecar_connection() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        let error = manager
            .stop_mic_test()
            .await
            .expect_err("stop_mic_test should fail without sidecar");
        assert!(error.contains("Sidecar not connected"));
    }

    #[tokio::test]
    async fn test_query_model_status_requires_sidecar_connection() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        let error = manager
            .query_model_status(None)
            .await
            .expect_err("query_model_status should fail without sidecar");
        assert!(error.contains("E_SIDECAR_IPC"));
    }

    #[tokio::test]
    async fn test_download_model_requires_sidecar_connection() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        let error = manager
            .download_model(None, None)
            .await
            .expect_err("download_model should fail without sidecar");
        assert!(error.contains("E_SIDECAR_IPC"));
    }

    #[tokio::test]
    async fn test_purge_model_cache_requires_sidecar_connection() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        let error = manager
            .purge_model_cache(None)
            .await
            .expect_err("purge_model_cache should fail without sidecar");
        assert!(error.contains("Sidecar not connected"));
    }

    #[tokio::test]
    async fn test_purge_model_cache_rejected_while_loading_or_downloading() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);
        *manager.model_status.write().await = ModelStatus::Downloading;

        let error = manager
            .purge_model_cache(None)
            .await
            .expect_err("purge_model_cache should reject while downloading");
        assert!(
            error.contains("Cannot purge model while download or initialization is in progress")
        );
    }

    #[tokio::test]
    async fn test_list_replacement_presets_requires_sidecar_connection() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        let error = manager
            .list_replacement_presets()
            .await
            .expect_err("list_replacement_presets should fail without sidecar");
        assert!(error.contains("Sidecar not connected"));
    }

    #[tokio::test]
    async fn test_get_preset_replacement_rules_requires_sidecar_connection() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        let error = manager
            .get_preset_replacement_rules("punctuation".to_string())
            .await
            .expect_err("get_preset_replacement_rules should fail without sidecar");
        assert!(error.contains("Sidecar not connected"));
    }

    #[tokio::test]
    async fn test_get_active_replacement_rules_requires_sidecar_connection() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        let error = manager
            .get_active_replacement_rules()
            .await
            .expect_err("get_active_replacement_rules should fail without sidecar");
        assert!(error.contains("Sidecar not connected"));
    }

    #[tokio::test]
    async fn test_set_active_replacement_rules_requires_sidecar_connection() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        let error = manager
            .set_active_replacement_rules(Vec::new())
            .await
            .expect_err("set_active_replacement_rules should fail without sidecar");
        assert!(error.contains("Sidecar not connected"));
    }

    #[tokio::test]
    async fn test_preview_replacement_requires_sidecar_connection() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        let error = manager
            .preview_replacement("hello".to_string(), Vec::new())
            .await
            .expect_err("preview_replacement should fail without sidecar");
        assert!(error.contains("Sidecar not connected"));
    }
}
