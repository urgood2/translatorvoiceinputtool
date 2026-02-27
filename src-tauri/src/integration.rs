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

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{cell::RefCell, thread_local};

use global_hotkey::GlobalHotKeyEvent;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{Mutex, Notify, RwLock};
use uuid::Uuid;

use crate::audio_cue::{AudioCueManager, CueType};
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
use crate::overlay::{
    FileOverlayConfigStore, OverlayManager, OverlayWindowBackend, TauriOverlayWindowBackend,
    OVERLAY_WINDOW_LABEL,
};
use crate::recording::{
    CancelReason, RecordingController, RecordingEvent, StopResult, TranscriptionResult,
};
use crate::sidecar::SidecarManager;
use crate::state::{AppState, AppStateManager, CannotRecordReason, StateEvent};
use crate::supervisor::{
    SidecarState as SupervisorState, SidecarSupervisor, SidecarSupervisorConfig,
};
use crate::watchdog::{self, PingCallback, Watchdog, WatchdogConfig, WatchdogEvent};

/// Tray icon event name.
const EVENT_TRAY_UPDATE: &str = "tray:update";
/// Canonical app state change event.
const EVENT_STATE_CHANGED: &str = "state:changed";

/// Model progress event name.
const EVENT_MODEL_PROGRESS: &str = "model:progress";

/// Model status event name.
const EVENT_MODEL_STATUS: &str = "model:status";

/// Canonical sidecar status event name.
const EVENT_SIDECAR_STATUS: &str = "sidecar:status";
/// Canonical recording phase event name.
const EVENT_RECORDING_STATUS: &str = "recording:status";
const DEVICE_HOT_SWAP_POLL_INTERVAL: Duration = Duration::from_millis(1200);
const DEVICE_HOT_SWAP_DEBOUNCE: Duration = Duration::from_millis(750);
const DEVICE_REMOVED_CLIPBOARD_REASON: &str =
    "Audio device disconnected during transcription; transcript copied to clipboard.";

/// Model status tracking.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    /// Model status unknown (not yet queried).
    #[default]
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

/// Download/initialization progress.
#[derive(Debug, Clone, Serialize)]
pub struct ModelProgress {
    /// Model identifier when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Current bytes downloaded or processed.
    pub current: u64,
    /// Total bytes (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Progress unit.
    pub unit: String,
    /// Progress stage description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    /// Active file path when downloading multi-file manifests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_file: Option<String>,
    /// Completed files count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_completed: Option<u64>,
    /// Total files count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_total: Option<u64>,
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
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub current_file: Option<String>,
    #[serde(default)]
    pub files_completed: Option<u64>,
    #[serde(default)]
    pub files_total: Option<u64>,
}

/// Canonical transcription complete event name.
const EVENT_TRANSCRIPT_COMPLETE: &str = "transcript:complete";

/// Canonical transcription error event name.
const EVENT_TRANSCRIPT_ERROR: &str = "transcript:error";

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

fn model_progress_from_parts(
    model_id: Option<String>,
    current: u64,
    total: Option<u64>,
    unit: Option<String>,
    stage: Option<String>,
    current_file: Option<String>,
    files_completed: Option<u64>,
    files_total: Option<u64>,
) -> ModelProgress {
    ModelProgress {
        model_id,
        current,
        total,
        unit: unit.unwrap_or_else(|| "bytes".to_string()),
        stage,
        current_file,
        files_completed,
        files_total,
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

fn purge_affects_configured_model(configured_model_id: &str, status_model_ids: &[String]) -> bool {
    status_model_ids
        .iter()
        .any(|status_model_id| status_model_id == configured_model_id)
}

fn purge_status_model_ids(
    purge_model_id: Option<&str>,
    configured_model_id: &str,
    purged_model_ids: &[String],
) -> Vec<String> {
    let resolved_ids: Vec<String> = normalized_purged_model_ids(purged_model_ids);

    if resolved_ids.is_empty() {
        if purge_model_id.is_some() {
            Vec::new()
        } else {
            vec![configured_model_id.to_string()]
        }
    } else {
        resolved_ids
    }
}

fn normalized_purged_model_ids(purged_model_ids: &[String]) -> Vec<String> {
    let mut resolved_ids: Vec<String> = Vec::new();
    for raw_id in purged_model_ids {
        let trimmed = raw_id.trim();
        if trimmed.is_empty() {
            continue;
        }
        if resolved_ids.iter().any(|existing| existing == trimmed) {
            continue;
        }
        resolved_ids.push(trimmed.to_string());
    }
    resolved_ids
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

fn emit_missing_model_status_for_purged_models_with_broadcaster<B: AppEventBroadcaster>(
    handle: &B,
    event_seq: &Arc<AtomicU64>,
    status_model_ids: &[String],
) {
    for status_model_id in status_model_ids {
        let payload = model_status_event_payload(
            ModelStatus::Missing,
            Some(status_model_id.clone()),
            None,
            None,
            None,
        );
        emit_with_shared_seq_for_broadcaster(
            handle,
            &[EVENT_MODEL_STATUS],
            json!(payload),
            event_seq,
        );
    }
}

fn configured_model_language_hint(config: &config::AppConfig) -> Option<String> {
    config
        .model
        .as_ref()
        .and_then(|model| model.language.as_deref())
        .and_then(|language| {
            let trimmed = language.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
}

fn asr_initialize_params(model_id: &str, device_pref: &str, language: Option<&str>) -> Value {
    let mut params = serde_json::Map::new();
    params.insert("model_id".to_string(), json!(model_id));
    params.insert("device_pref".to_string(), json!(device_pref));

    if let Some(language) = language.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }) {
        params.insert("language".to_string(), json!(language));
    }

    Value::Object(params)
}

fn asr_initialize_language_rejected(error: &RpcError) -> bool {
    match error {
        RpcError::Remote { kind, message, .. } => {
            if kind.eq_ignore_ascii_case("E_METHOD_NOT_FOUND") {
                return true;
            }

            let kind_matches = kind.is_empty()
                || kind.eq_ignore_ascii_case("E_INVALID_PARAMS")
                || kind.eq_ignore_ascii_case("E_INVALID");
            if !kind_matches {
                return false;
            }

            let message = message.to_ascii_lowercase();
            message.contains("language")
                && [
                    "invalid",
                    "unknown",
                    "unexpected",
                    "not allowed",
                    "additional",
                    "property",
                    "parameter",
                    "params",
                ]
                .iter()
                .any(|token| message.contains(token))
        }
        _ => false,
    }
}

#[derive(Debug, Deserialize)]
struct AsrInitializeResult {
    status: String,
}

async fn call_asr_initialize_with_language_fallback(
    client: &RpcClient,
    model_id: &str,
    device_pref: &str,
    language: Option<String>,
) -> Result<AsrInitializeResult, RpcError> {
    let params = asr_initialize_params(model_id, device_pref, language.as_deref());

    match client
        .call::<AsrInitializeResult>("asr.initialize", Some(params))
        .await
    {
        Ok(result) => Ok(result),
        Err(error) => {
            let Some(requested_language) = language.as_deref() else {
                return Err(error);
            };

            if !asr_initialize_language_rejected(&error) {
                return Err(error);
            }

            log::info!(
                "language not supported in this build; retrying asr.initialize without language override (requested='{}')",
                requested_language
            );

            let fallback_params = asr_initialize_params(model_id, device_pref, None);
            client
                .call::<AsrInitializeResult>("asr.initialize", Some(fallback_params))
                .await
        }
    }
}

fn map_download_response_status(status: &SidecarModelStatus) -> ModelStatus {
    match status.status.as_str() {
        "missing" => ModelStatus::Missing,
        "downloading" => ModelStatus::Downloading,
        "loading" | "verifying" | "installing" => ModelStatus::Loading,
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
    #[serde(default)]
    pub applied_presets: Option<Vec<String>>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeviceHotSwapDecision {
    removed_device_uid: Option<String>,
    removed_device_name: Option<String>,
    should_stop_recording: bool,
    should_force_clipboard_only: bool,
    should_emit_error: bool,
}

fn decide_device_hot_swap(
    state: AppState,
    configured_device_uid: Option<&str>,
    previous_devices: &[SidecarAudioDevice],
    current_devices: &[SidecarAudioDevice],
) -> DeviceHotSwapDecision {
    let Some(configured_uid) = configured_device_uid else {
        return DeviceHotSwapDecision {
            removed_device_uid: None,
            removed_device_name: None,
            should_stop_recording: false,
            should_force_clipboard_only: false,
            should_emit_error: false,
        };
    };

    if current_devices
        .iter()
        .any(|device| device.uid.as_str() == configured_uid)
    {
        return DeviceHotSwapDecision {
            removed_device_uid: None,
            removed_device_name: None,
            should_stop_recording: false,
            should_force_clipboard_only: false,
            should_emit_error: false,
        };
    }

    let removed_name = previous_devices
        .iter()
        .find(|device| device.uid.as_str() == configured_uid)
        .map(|device| device.name.clone())
        .unwrap_or_else(|| configured_uid.to_string());

    let should_stop_recording = state == AppState::Recording;
    let should_force_clipboard_only = state == AppState::Transcribing;
    let should_emit_error = should_stop_recording || should_force_clipboard_only;

    DeviceHotSwapDecision {
        removed_device_uid: Some(configured_uid.to_string()),
        removed_device_name: Some(removed_name),
        should_stop_recording,
        should_force_clipboard_only,
        should_emit_error,
    }
}

fn device_removed_app_error(
    removed_device_uid: String,
    removed_device_name: String,
    fallback_device_uid: Option<String>,
    had_active_recording: bool,
    transcript_preserved: bool,
) -> AppError {
    AppError::new(
        ErrorKind::DeviceRemoved.to_sidecar(),
        "Audio device was disconnected. Recording stopped. Using default device.",
        Some(json!({
            "removed_device_uid": removed_device_uid,
            "removed_device_name": removed_device_name,
            "fallback_device_uid": fallback_device_uid,
            "had_active_recording": had_active_recording,
            "transcript_preserved": transcript_preserved,
        })),
        true,
    )
}

async fn cleanup_active_session_for_no_audio_devices(
    manager: &IntegrationManager,
    state_manager: &Arc<AppStateManager>,
    recording_context: &Arc<RwLock<Option<RecordingContext>>>,
    current_session_id: &Arc<RwLock<Option<String>>>,
) {
    if current_session_id.read().await.is_none() {
        return;
    }

    if state_manager.get() == AppState::Transcribing {
        let mut context = recording_context.write().await;
        if let Some(context) = context.as_mut() {
            context.force_clipboard_only = true;
            context.force_clipboard_reason = Some(DEVICE_REMOVED_CLIPBOARD_REASON.to_string());
        }
    }

    if let Err(stop_error) = manager.stop_recording().await {
        log::warn!(
            "Failed to stop active session during no-audio-device transition: {}",
            stop_error
        );
        if let Err(cancel_error) = manager.cancel_recording().await {
            log::warn!(
                "Failed to cancel active session during no-audio-device transition: {}",
                cancel_error
            );
        }
    }
}

fn no_audio_device_app_error() -> AppError {
    AppError::new(
        ErrorKind::NoAudioDevice.to_sidecar(),
        "No audio input device is available. Connect a microphone and try again.",
        Some(json!({
            "reason": "no_available_input_device"
        })),
        true,
    )
}

fn device_uid_snapshot(devices: &[SidecarAudioDevice]) -> Vec<String> {
    let mut snapshot: Vec<String> = devices.iter().map(|device| device.uid.clone()).collect();
    snapshot.sort();
    snapshot
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

fn resolve_transcript_texts(
    text: &str,
    raw_text: Option<&str>,
    final_text: Option<&str>,
) -> (String, String) {
    let resolved_final = final_text.unwrap_or(text).to_string();
    let resolved_raw = raw_text.unwrap_or(&resolved_final).to_string();
    (resolved_raw, resolved_final)
}

fn stop_rpc_method_for_result(result: &StopResult) -> &'static str {
    match result {
        StopResult::TooShort => "recording.cancel",
        StopResult::Transcribing { .. } => "recording.stop",
    }
}

/// Returns `true` when the overlay should remain visible for this recording event.
///
/// The overlay is shown during active recording and the transcription phase
/// (Stopped means recording ended but transcription is in progress). It hides
/// for terminal events: TranscriptionComplete, TranscriptionFailed, Cancelled,
/// TooShort.
fn is_overlay_recording_active(event: &RecordingEvent) -> bool {
    matches!(
        event,
        RecordingEvent::Started { .. } | RecordingEvent::Stopped { .. }
    )
}

fn should_apply_overlay_config_change(last_enabled: Option<bool>, enabled: bool) -> bool {
    last_enabled != Some(enabled)
}

fn overlay_recording_state_for_event(
    overlay_enabled: bool,
    event: &RecordingEvent,
) -> Option<bool> {
    if !overlay_enabled {
        return None;
    }
    Some(is_overlay_recording_active(event))
}

fn recording_event_audio_cue(event: &RecordingEvent) -> Option<CueType> {
    match event {
        // StartRecording cue is played pre-roll in start_recording_flow
        // before mic capture begins, so the event loop skips it.
        RecordingEvent::Started { .. } => None,
        RecordingEvent::Stopped { .. } => Some(CueType::StopRecording),
        RecordingEvent::Cancelled { .. } => Some(CueType::CancelRecording),
        RecordingEvent::TranscriptionFailed { .. } => Some(CueType::Error),
        _ => None,
    }
}

thread_local! {
    static AUDIO_CUE_MANAGER: RefCell<Option<AudioCueManager>> = const { RefCell::new(None) };
}

fn should_play_lifecycle_audio_cues() -> bool {
    !cfg!(test)
}

fn play_lifecycle_audio_cue(cue: CueType) {
    if !should_play_lifecycle_audio_cues() {
        return;
    }

    AUDIO_CUE_MANAGER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let manager = slot.get_or_insert_with(AudioCueManager::new);
        // Re-read config so toggling audio_cues_enabled takes effect
        // immediately without requiring a restart.
        let cfg = config::load_config();
        manager.set_enabled(cfg.audio.audio_cues_enabled);
        manager.play_cue(cue);
    });
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
        // Legacy error field: plain string for backward compatibility.
        "error": app_error.message,
        // Compatibility fields retained for one release cycle.
        "message": app_error.message,
        "recoverable": app_error.recoverable,
        // Canonical structured error for consumers that need code/recoverable.
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
        &sidecar_error_kind,
        "Transcription failed",
        Some(json!({
            "session_id": session_id,
            "error_kind": sidecar_error_kind,
            "sidecar_message": sidecar_message
        })),
        true,
    )
}

fn injection_method_attempted(reason: &str) -> &'static str {
    let normalized = reason.to_ascii_lowercase();

    if normalized.contains("focus") {
        return "focus_guard";
    }

    if normalized.contains("paste")
        || normalized.contains("keystroke")
        || normalized.contains("wayland")
        || normalized.contains("xdotool")
    {
        return "keystroke_injection";
    }

    if normalized.contains("clipboard") {
        return "clipboard";
    }

    "clipboard_paste"
}

fn clipboard_only_requires_app_error(reason: &str) -> bool {
    !reason
        .to_ascii_lowercase()
        .starts_with("app override clipboard-only mode")
}

fn injection_failure_app_error(reason: &str, text_length: usize) -> AppError {
    AppError::new(
        ErrorKind::InjectionFailed.to_sidecar(),
        "Automatic text injection failed. Transcript preserved in history.",
        Some(json!({
            "method_attempted": injection_method_attempted(reason),
            "reason": reason,
            "text_length": text_length
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

fn cannot_record_reason_message(reason: CannotRecordReason) -> &'static str {
    match reason {
        CannotRecordReason::Paused => "Recording disabled (paused)",
        CannotRecordReason::ModelLoading => "Model not ready",
        CannotRecordReason::AlreadyRecording => "Recording already in progress",
        CannotRecordReason::StillTranscribing => "Cannot start recording while transcribing",
        CannotRecordReason::InErrorState => "Cannot start recording while app is in error state",
    }
}

fn recording_start_params(session_id: &str, app_config: &config::AppConfig) -> Value {
    json!({
        "session_id": session_id,
        "device_uid": app_config.audio.device_uid,
        "trim_silence": app_config.audio.trim_silence,
        "vad_enabled": app_config.audio.vad_enabled,
        "vad_silence_ms": app_config.audio.vad_silence_ms,
        "vad_min_speech_ms": app_config.audio.vad_min_speech_ms
    })
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
    /// Raw transcript text from sidecar before post-processing/replacements.
    raw_text: Option<String>,
    /// Final transcript text from sidecar after post-processing/replacements.
    final_text: Option<String>,
    /// Optional language reported by sidecar transcription.
    language: Option<String>,
    /// Optional confidence reported by sidecar transcription.
    confidence: Option<f32>,
    /// When true, skip direct injection and force clipboard preservation.
    force_clipboard_only: bool,
    /// Optional reason associated with forced clipboard preservation.
    force_clipboard_reason: Option<String>,
    /// Pipeline timing marks for stop -> injection latency tracking.
    timing_marks: PipelineTimingMarks,
}

/// Central integration manager that wires everything together.
#[derive(Clone)]
pub struct IntegrationManager {
    /// Application state manager.
    state_manager: Arc<AppStateManager>,
    /// Recording controller.
    recording_controller: Arc<RecordingController>,
    /// Hotkey manager.
    hotkey_manager: Arc<RwLock<HotkeyManager>>,
    /// RPC client (if sidecar is connected).
    rpc_client: Arc<RwLock<Option<RpcClient>>>,
    /// Sidecar lifecycle supervisor.
    supervisor: Arc<Mutex<SidecarSupervisor<SidecarManager>>>,
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
    /// Overlay window lifecycle manager.
    overlay_manager: Arc<Mutex<OverlayManager>>,
    /// Notification channel for overlay config changes (event-driven, no polling).
    overlay_config_notify: Arc<Notify>,
    /// Cached overlay-enabled flag so disabled mode avoids per-event config reads.
    overlay_enabled: Arc<AtomicBool>,
}

impl IntegrationManager {
    /// Create a new integration manager.
    pub fn new(state_manager: Arc<AppStateManager>) -> Self {
        let recording_controller = Arc::new(RecordingController::new(Arc::clone(&state_manager)));
        let watchdog = Arc::new(Watchdog::with_config(WatchdogConfig::default()));
        let config = IntegrationConfig::default();
        let app_config = config::load_config();
        let mut sidecar_manager = SidecarManager::new();
        sidecar_manager.set_python_mode(config.python_path.clone(), config.sidecar_module.clone());
        let supervisor = Arc::new(Mutex::new(SidecarSupervisor::new(
            sidecar_manager,
            SidecarSupervisorConfig {
                captured_log_max_lines: app_config.supervisor.captured_log_max_lines,
                ..SidecarSupervisorConfig::default()
            },
        )));

        Self {
            state_manager,
            recording_controller,
            hotkey_manager: Arc::new(RwLock::new(HotkeyManager::new())),
            rpc_client: Arc::new(RwLock::new(None)),
            supervisor,
            app_handle: None,
            recording_context: Arc::new(RwLock::new(None)),
            current_session_id: Arc::new(RwLock::new(None)),
            config,
            model_status: Arc::new(RwLock::new(ModelStatus::Unknown)),
            model_progress: Arc::new(RwLock::new(None)),
            model_init_attempted: Arc::new(AtomicBool::new(false)),
            watchdog,
            event_seq: Arc::new(AtomicU64::new(1)),
            overlay_manager: Arc::new(Mutex::new(OverlayManager::new())),
            overlay_config_notify: Arc::new(Notify::new()),
            overlay_enabled: Arc::new(AtomicBool::new(app_config.ui.overlay_enabled)),
        }
    }

    /// Set the Tauri app handle.
    pub fn set_app_handle(&mut self, handle: AppHandle) {
        self.app_handle = Some(handle.clone());
        if let Ok(mut supervisor) = self.supervisor.try_lock() {
            supervisor.set_app_handle(handle);
        }
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
        self.start_overlay_window_loop();
        self.start_device_hot_swap_loop();

        // Start watchdog loop
        self.start_watchdog_loop();

        log::info!("Integration manager initialized");
        Ok(())
    }

    /// Start overlay config-gate loop.
    ///
    /// Ensures the overlay window is pre-created when `ui.overlay_enabled=true`
    /// and destroyed when disabled. The actual show/hide is driven by recording
    /// events in `start_recording_event_loop`.
    fn start_overlay_window_loop(&self) {
        let app_handle = self.app_handle.clone();
        let overlay_manager = Arc::clone(&self.overlay_manager);
        let overlay_config_notify = Arc::clone(&self.overlay_config_notify);
        let overlay_enabled = Arc::clone(&self.overlay_enabled);

        tokio::spawn(async move {
            let Some(handle) = app_handle else {
                log::warn!("Overlay window loop skipped: app handle missing");
                return;
            };

            let config_store = FileOverlayConfigStore;
            let mut last_enabled: Option<bool> = None;

            log::info!("Overlay config-gate loop started");

            loop {
                let enabled = config::load_config().ui.overlay_enabled;
                overlay_enabled.store(enabled, Ordering::Release);
                if !should_apply_overlay_config_change(last_enabled, enabled) {
                    overlay_config_notify.notified().await;
                    continue;
                }

                let backend = TauriOverlayWindowBackend::new(&handle);

                if !enabled {
                    // Disabled: hide and destroy any existing window.
                    let mut manager = overlay_manager.lock().await;
                    if let Err(error) = manager.hide(&config_store, &backend) {
                        log::warn!("Overlay hide on disable failed: {error}");
                    }
                } else {
                    // Enabled: pre-create the window (hidden) so show is fast.
                    let backend_ref = &backend;
                    if !backend_ref.window_exists(OVERLAY_WINDOW_LABEL) {
                        if let Err(error) = backend_ref.create_window(OVERLAY_WINDOW_LABEL) {
                            log::warn!("Overlay window pre-creation failed: {error}");
                        }
                    }
                }

                last_enabled = Some(enabled);
                overlay_config_notify.notified().await;
            }
        });
    }

    pub fn notify_overlay_config_changed(&self) {
        self.overlay_config_notify.notify_waiters();
    }

    fn emit_app_error_event(
        app_handle: &Option<AppHandle>,
        event_seq: &Arc<AtomicU64>,
        app_error: &AppError,
    ) {
        if let Some(ref handle) = app_handle {
            emit_with_shared_seq(
                handle,
                &[EVENT_APP_ERROR],
                app_error_event_payload(app_error),
                event_seq,
            );
        }
    }

    fn persist_default_audio_device_selection() -> Result<(), String> {
        let mut app_config = config::load_config();
        app_config.audio.device_uid = None;
        config::save_config(&app_config).map_err(|error| format!("Failed to save config: {error}"))
    }

    /// Start device hot-swap monitor loop.
    ///
    /// Polls audio.list_devices and handles selected-device disappearance with:
    /// - stop active recording/transcription handling
    /// - default device fallback
    /// - structured, recoverable app:error emission when active session is impacted
    fn start_device_hot_swap_loop(&self) {
        let manager = self.clone();
        let state_manager = Arc::clone(&self.state_manager);
        let recording_context = Arc::clone(&self.recording_context);
        let current_session_id = Arc::clone(&self.current_session_id);
        let app_handle = self.app_handle.clone();
        let event_seq = Arc::clone(&self.event_seq);

        tokio::spawn(async move {
            let mut tick = tokio::time::interval(DEVICE_HOT_SWAP_POLL_INTERVAL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut previous_devices: Option<Vec<SidecarAudioDevice>> = None;
            let mut last_change_handled_at: Option<Instant> = None;
            let mut last_device_list_error_at: Option<Instant> = None;

            loop {
                tick.tick().await;

                let devices = match manager.list_audio_devices().await {
                    Ok(devices) => devices,
                    Err(error) => {
                        let now = Instant::now();
                        let should_log = last_device_list_error_at
                            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(5));
                        if should_log {
                            log::debug!("Device hot-swap poll skipped: {}", error);
                            last_device_list_error_at = Some(now);
                        }
                        continue;
                    }
                };

                // Keep the tray device cache up to date on every successful poll.
                if let Some(ref handle) = app_handle {
                    let cache = handle.state::<crate::tray::TrayDeviceCache>();
                    cache.update(
                        &devices
                            .iter()
                            .map(|d| (d.uid.clone(), d.name.clone()))
                            .collect::<Vec<_>>(),
                    );
                }

                let Some(previous) = previous_devices.as_ref() else {
                    previous_devices = Some(devices);
                    continue;
                };

                if device_uid_snapshot(previous) == device_uid_snapshot(&devices) {
                    continue;
                }

                let now = Instant::now();
                if last_change_handled_at
                    .is_some_and(|last| now.duration_since(last) < DEVICE_HOT_SWAP_DEBOUNCE)
                {
                    previous_devices = Some(devices);
                    continue;
                }
                last_change_handled_at = Some(now);

                if let Some(ref handle) = app_handle {
                    emit_with_shared_seq(
                        handle,
                        &[EVENT_TRAY_UPDATE],
                        json!({
                            "reason": "device_list_changed",
                            "device_count": devices.len(),
                        }),
                        &event_seq,
                    );
                }

                if devices.is_empty() {
                    cleanup_active_session_for_no_audio_devices(
                        &manager,
                        &state_manager,
                        &recording_context,
                        &current_session_id,
                    )
                    .await;
                    state_manager.transition_to_error(
                        "No audio input device available. Connect a microphone and try again."
                            .to_string(),
                    );
                    let app_error = no_audio_device_app_error();
                    Self::emit_app_error_event(&app_handle, &event_seq, &app_error);
                    previous_devices = Some(devices);
                    continue;
                }

                let configured_device_uid = config::load_config().audio.device_uid;
                let state = state_manager.get();
                let decision = decide_device_hot_swap(
                    state,
                    configured_device_uid.as_deref(),
                    previous,
                    &devices,
                );

                if let Some(removed_uid) = decision.removed_device_uid.clone() {
                    let removed_name = decision
                        .removed_device_name
                        .clone()
                        .unwrap_or_else(|| removed_uid.clone());
                    let had_active_recording =
                        decision.should_emit_error || current_session_id.read().await.is_some();
                    let mut transcript_preserved = false;

                    if decision.should_stop_recording {
                        if let Err(error) = manager.stop_recording().await {
                            log::warn!(
                                "Failed to stop recording after device removal (partial audio may be lost): {}",
                                error
                            );
                        }
                    }

                    if decision.should_force_clipboard_only {
                        let mut context = recording_context.write().await;
                        if let Some(context) = context.as_mut() {
                            context.force_clipboard_only = true;
                            context.force_clipboard_reason =
                                Some(DEVICE_REMOVED_CLIPBOARD_REASON.to_string());
                            transcript_preserved = true;
                        }
                    }

                    let fallback_device_uid = match manager.set_audio_device(None).await {
                        Ok(uid) => uid,
                        Err(error) => {
                            log::warn!("Failed to switch to default audio device: {}", error);
                            None
                        }
                    };

                    if let Err(error) = Self::persist_default_audio_device_selection() {
                        log::warn!(
                            "Failed to persist default audio device after hot-swap: {}",
                            error
                        );
                    }

                    if let Some(ref handle) = app_handle {
                        emit_with_shared_seq(
                            handle,
                            &[EVENT_TRAY_UPDATE],
                            json!({
                                "reason": "device_changed",
                                "removed_device_uid": removed_uid,
                                "active_device_uid": fallback_device_uid,
                            }),
                            &event_seq,
                        );
                    }

                    if decision.should_emit_error {
                        let app_error = device_removed_app_error(
                            removed_uid,
                            removed_name,
                            fallback_device_uid,
                            had_active_recording,
                            transcript_preserved,
                        );
                        Self::emit_app_error_event(&app_handle, &event_seq, &app_error);
                    } else {
                        log::info!(
                            "Configured audio device removed while idle; switched to default device"
                        );
                    }
                }

                previous_devices = Some(devices);
            }
        });
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
        let manager = self.clone();

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
                    WatchdogEvent::SidecarRecoveryRequested { reason } => {
                        log::error!(
                            "Watchdog requested supervisor recovery for sidecar: {}",
                            reason
                        );
                        // Transition to error state
                        state_manager
                            .transition_to_error("Sidecar hung, restarting...".to_string());

                        // Keep legacy event for frontend compatibility while recovery moves through
                        // supervisor policy.
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &["sidecar:restart"],
                                serde_json::json!({ "reason": reason }),
                                &event_seq,
                            );
                        }

                        if let Err(err) = manager.recover_sidecar_from_watchdog().await {
                            log::error!("Watchdog recovery via supervisor failed: {}", err);
                        }
                    }
                    WatchdogEvent::SidecarHung => {
                        // Legacy event retained in watchdog for observability.
                        // Recovery is handled by the SidecarRecoveryRequested branch above;
                        // calling recover here would trigger a duplicate attempt.
                        log::warn!("Watchdog: legacy SidecarHung event received (no-op)");
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

                                    if let Some(ref handle) = app_handle {
                                        emit_with_shared_seq(
                                            handle,
                                            &[EVENT_TRAY_UPDATE],
                                            json!({
                                                "reason": "device_list_changed",
                                                "device_count": result.devices.len(),
                                            }),
                                            &event_seq,
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
                                client,
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
            "Initializing ASR model: model={}, device={}, language={:?}",
            model_id,
            device_pref,
            configured_model_language_hint(&config)
        );
        let language = configured_model_language_hint(&config);

        match call_asr_initialize_with_language_fallback(client, &model_id, &device_pref, language)
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
        let resolved_model_id = resolve_model_id(
            params
                .as_ref()
                .and_then(|value| value.get("model_id"))
                .and_then(Value::as_str)
                .map(ToString::to_string),
        );
        let status_result = match client
            .call::<SidecarModelStatus>("model.install", params.clone())
            .await
        {
            Ok(status) => Ok(status),
            Err(RpcError::Remote { kind, .. }) if kind == "E_METHOD_NOT_FOUND" => {
                log::info!(
                    "model.install unsupported by sidecar; falling back to legacy model.download + asr.initialize"
                );
                match client
                    .call::<SidecarModelStatus>("model.download", params.clone())
                    .await
                {
                    Ok(mut status) => {
                        let config = config::load_config();
                        let device_pref = config.effective_model_device_pref();
                        let language = configured_model_language_hint(&config);
                        match call_asr_initialize_with_language_fallback(
                            client,
                            &resolved_model_id,
                            &device_pref,
                            language,
                        )
                        .await
                        {
                            Ok(result) => {
                                log::info!(
                                    "Legacy fallback asr.initialize complete: model={}, status={}",
                                    resolved_model_id,
                                    result.status
                                );
                                status.status = "ready".to_string();
                                Ok(status)
                            }
                            Err(error) => Err(format!(
                                "E_MODEL_DOWNLOAD: Legacy sidecar fallback failed during asr.initialize: {}",
                                error
                            )),
                        }
                    }
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
                    let model_progress_data = model_progress_from_parts(
                        Some(resolve_model_id(Some(status.model_id.clone()))),
                        progress.current,
                        progress.total,
                        progress.unit,
                        progress.stage,
                        progress.current_file,
                        progress.files_completed,
                        progress.files_total,
                    );
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

        let purge_model_id = match model_id {
            Some(id) => {
                let trimmed = id.trim().to_string();
                if trimmed.is_empty() {
                    return Err(
                        "Invalid model_id: blank or whitespace-only value".to_string(),
                    );
                }
                Some(trimmed)
            }
            None => None,
        };

        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        #[derive(Deserialize)]
        struct PurgeResult {
            #[allow(dead_code)]
            purged: bool,
            #[serde(default)]
            purged_model_ids: Vec<String>,
        }

        let params = purge_model_id
            .as_ref()
            .map(|requested_model_id| json!({ "model_id": requested_model_id }));

        let result = client
            .call::<PurgeResult>("model.purge_cache", params)
            .await
            .map_err(|e| format!("Failed to purge cache: {}", e))?;

        let configured_model_id = configured_model_id();
        let reported_purged_model_ids = normalized_purged_model_ids(&result.purged_model_ids);
        let status_model_ids = purge_status_model_ids(
            purge_model_id.as_deref(),
            configured_model_id.as_str(),
            &reported_purged_model_ids,
        );
        let removed_model_count = reported_purged_model_ids.len();
        let affects_configured_model =
            purge_affects_configured_model(configured_model_id.as_str(), &status_model_ids);

        if affects_configured_model {
            *self.model_status.write().await = ModelStatus::Missing;
            self.recording_controller.set_model_ready(false).await;
        }

        // Emit model:status Missing for each purged model so UI consumers
        // can track cache state for all models, not just the configured one.
        if let Some(app_handle) = &self.app_handle {
            emit_missing_model_status_for_purged_models_with_broadcaster(
                app_handle,
                &self.event_seq,
                &status_model_ids,
            );
        }

        log::info!(
            "Model cache purged{} ({} models removed)",
            purge_model_id
                .as_ref()
                .map(|id| format!(" for model {}", id))
                .unwrap_or_default(),
            removed_model_count,
        );
        Ok(())
    }

    async fn start_recording_flow(
        state_manager: &Arc<AppStateManager>,
        recording_controller: &Arc<RecordingController>,
        rpc_client: &Arc<RwLock<Option<RpcClient>>>,
        recording_context: &Arc<RwLock<Option<RecordingContext>>>,
        current_session_id: &Arc<RwLock<Option<String>>>,
    ) -> Result<(), String> {
        if current_session_id.read().await.is_some() {
            return Err("Recording already in progress".to_string());
        }

        state_manager
            .can_start_recording()
            .map_err(cannot_record_reason_message)
            .map_err(ToString::to_string)?;

        if !recording_controller.is_model_ready().await {
            return Err("Model not ready".to_string());
        }

        let session_id = Uuid::new_v4().to_string();
        let focus = capture_focus();
        let app_config = config::load_config();
        let params = recording_start_params(session_id.as_str(), &app_config);

        // Play start cue BEFORE mic capture begins and wait for the pre-roll
        // delay so the beep is less likely to be picked up by the microphone.
        play_lifecycle_audio_cue(CueType::StartRecording);
        tokio::time::sleep(crate::audio_cue::START_CUE_PRE_ROLL).await;

        let start_response: Value = {
            let client_guard = rpc_client.read().await;
            let client = client_guard
                .as_ref()
                .ok_or_else(|| "Sidecar not connected".to_string())?;

            client
                .call("recording.start", Some(params))
                .await
                .map_err(|err| format!("Failed to call recording.start RPC: {}", err))?
        };

        if let Err(mismatch) =
            validate_recording_start_response(session_id.as_str(), &start_response)
        {
            if let Some(client) = rpc_client.read().await.as_ref() {
                let cancel_session_id = start_response
                    .get("session_id")
                    .and_then(Value::as_str)
                    .unwrap_or(session_id.as_str())
                    .to_string();
                let cancel_result: Result<Value, _> = client
                    .call(
                        "recording.cancel",
                        Some(json!({ "session_id": cancel_session_id })),
                    )
                    .await;
                if let Err(err) = cancel_result {
                    log::warn!(
                        "Failed to roll back recording.start via recording.cancel: {}",
                        err
                    );
                }
            }
            return Err(mismatch);
        }

        if let Err(err) = recording_controller
            .start_with_session_id(session_id.clone())
            .await
        {
            if let Some(client) = rpc_client.read().await.as_ref() {
                let cancel_result: Result<Value, _> = client
                    .call(
                        "recording.cancel",
                        Some(json!({ "session_id": session_id })),
                    )
                    .await;
                if let Err(cancel_err) = cancel_result {
                    log::warn!(
                        "Failed to cancel sidecar session after host start failure: {}",
                        cancel_err
                    );
                }
            }
            return Err(format!("Failed to start recording: {}", err));
        }

        *recording_context.write().await = Some(RecordingContext {
            focus_before: focus,
            session_id: session_id.clone(),
            audio_duration_ms: None,
            raw_text: None,
            final_text: None,
            language: None,
            confidence: None,
            force_clipboard_only: false,
            force_clipboard_reason: None,
            timing_marks: PipelineTimingMarks::default(),
        });
        *current_session_id.write().await = Some(session_id);

        Ok(())
    }

    /// Unified recording start entry point for commands/UI/hotkey/tray/overlay.
    pub async fn start_recording(&self) -> Result<(), String> {
        Self::start_recording_flow(
            &self.state_manager,
            &self.recording_controller,
            &self.rpc_client,
            &self.recording_context,
            &self.current_session_id,
        )
        .await
    }

    /// Unified recording stop entry point for commands/UI/hotkey/tray/overlay.
    pub async fn stop_recording(&self) -> Result<(), String> {
        Self::stop_recording_flow(
            &self.recording_controller,
            &self.rpc_client,
            &self.recording_context,
            &self.current_session_id,
        )
        .await
    }

    /// Unified recording cancel entry point for commands/UI/hotkey/tray/overlay.
    pub async fn cancel_recording(&self) -> Result<(), String> {
        Self::cancel_recording_flow(
            &self.recording_controller,
            &self.rpc_client,
            &self.recording_context,
            &self.current_session_id,
        )
        .await
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

    async fn apply_supervisor_runtime_config(&self) {
        let mut supervisor = self.supervisor.lock().await;
        if let Some(handle) = self.app_handle.clone() {
            supervisor.set_app_handle(handle);
        }

        // Preserve existing integration config overrides for sidecar spawn.
        supervisor.controller_mut().set_python_mode(
            self.config.python_path.clone(),
            self.config.sidecar_module.clone(),
        );
    }

    async fn reset_rpc_client(&self, request_shutdown: bool) {
        let rpc_client = self.rpc_client.write().await.take();
        if let Some(client) = rpc_client {
            if request_shutdown {
                let _: Result<Value, RpcError> = client.call("system.shutdown", None).await;
            }
            client.shutdown().await;
        }
    }

    async fn attach_rpc_client_to_supervisor_sidecar(&self) {
        let sidecar = {
            let supervisor = self.supervisor.lock().await;
            supervisor.controller().clone()
        };
        let rpc_client = RpcClient::new_with_sidecar_manager(sidecar);
        self.start_notification_loop(rpc_client.subscribe());
        *self.rpc_client.write().await = Some(rpc_client);
    }

    async fn emit_supervisor_failure(&self, message: String, restart_count: u32) {
        self.state_manager.transition_to_error(message.clone());
        self.recording_controller.set_model_ready(false).await;

        if let Some(ref handle) = self.app_handle {
            let lower = message.to_ascii_lowercase();
            let error_kind = if lower.contains("circuit breaker") {
                ErrorKind::SidecarCircuitBreaker.to_sidecar()
            } else if lower.contains("spawn") {
                ErrorKind::SidecarSpawn.to_sidecar()
            } else {
                ErrorKind::SidecarCrash.to_sidecar()
            };

            let app_error = AppError::new(
                error_kind,
                message.clone(),
                Some(json!({ "restart_count": restart_count })),
                !lower.contains("circuit breaker"),
            );
            emit_with_shared_seq(
                handle,
                &[EVENT_APP_ERROR],
                app_error_event_payload(&app_error),
                &self.event_seq,
            );
        }
    }

    /// Start the sidecar process through supervisor and connect RPC client.
    pub async fn start_sidecar(&self) -> Result<(), String> {
        log::info!("Starting sidecar process via supervisor");
        self.reset_rpc_client(false).await;
        self.apply_supervisor_runtime_config().await;

        let (result, state, restart_count) = {
            let mut supervisor = self.supervisor.lock().await;
            let result = supervisor.start().await;
            (result, supervisor.state(), supervisor.restart_count())
        };

        match result {
            Ok(()) if state == SupervisorState::Ready => {
                self.attach_rpc_client_to_supervisor_sidecar().await;
                self.spawn_model_check();
                log::info!("Sidecar connected via supervisor");
                Ok(())
            }
            Ok(()) => {
                let message = format!("Supervisor reported unexpected sidecar state: {:?}", state);
                self.emit_supervisor_failure(message.clone(), restart_count)
                    .await;
                self.watchdog.mark_not_running().await;
                Err(message)
            }
            Err(err) => {
                self.emit_supervisor_failure(err.clone(), restart_count)
                    .await;
                self.watchdog.mark_not_running().await;
                Err(err)
            }
        }
    }

    /// Manually restart the sidecar process.
    ///
    /// This path is used for user-initiated recovery after failures.
    pub async fn restart_sidecar(&self) -> Result<(), String> {
        log::info!("Manual sidecar restart requested");
        self.reset_rpc_client(false).await;
        self.apply_supervisor_runtime_config().await;

        let (result, state, restart_count) = {
            let mut supervisor = self.supervisor.lock().await;
            let result = supervisor.restart().await;
            (result, supervisor.state(), supervisor.restart_count())
        };

        match result {
            Ok(()) if state == SupervisorState::Ready => {
                self.attach_rpc_client_to_supervisor_sidecar().await;
                self.spawn_model_check();
                log::info!("Sidecar restarted via supervisor");
                Ok(())
            }
            Ok(()) => {
                let message = format!("Supervisor reported unexpected sidecar state: {:?}", state);
                self.emit_supervisor_failure(message.clone(), restart_count)
                    .await;
                self.watchdog.mark_not_running().await;
                Err(message)
            }
            Err(err) => {
                self.emit_supervisor_failure(err.clone(), restart_count)
                    .await;
                self.watchdog.mark_not_running().await;
                Err(err)
            }
        }
    }

    async fn recover_sidecar_from_watchdog(&self) -> Result<(), String> {
        log::warn!("Watchdog requested sidecar recovery via supervisor");
        self.reset_rpc_client(false).await;
        self.apply_supervisor_runtime_config().await;

        let (result, state, restart_count) = {
            let mut supervisor = self.supervisor.lock().await;
            let result = supervisor.handle_crash().await;
            (result, supervisor.state(), supervisor.restart_count())
        };

        match result {
            Ok(()) if state == SupervisorState::Ready => {
                self.attach_rpc_client_to_supervisor_sidecar().await;
                self.spawn_model_check();
                Ok(())
            }
            Ok(()) => {
                let message = format!(
                    "Watchdog recovery ended in non-ready supervisor state: {:?}",
                    state
                );
                self.emit_supervisor_failure(message.clone(), restart_count)
                    .await;
                self.watchdog.mark_not_running().await;
                Err(message)
            }
            Err(err) => {
                let message = format!("Watchdog recovery failed: {}", err);
                self.emit_supervisor_failure(message.clone(), restart_count)
                    .await;
                self.watchdog.mark_not_running().await;
                Err(message)
            }
        }
    }

    /// Ping the sidecar to verify connection.
    #[allow(dead_code)]
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

    async fn stop_sidecar_runtime(&self) {
        self.reset_rpc_client(true).await;
        let mut supervisor = self.supervisor.lock().await;
        let _ = supervisor.stop().await;

        self.watchdog.mark_not_running().await;
    }

    /// Initialize ASR model via sidecar.
    pub async fn initialize_asr(&self, model_id: &str, device: &str) -> Result<(), String> {
        let client = self.rpc_client.read().await;
        let client = client
            .as_ref()
            .ok_or_else(|| "Sidecar not connected".to_string())?;

        // Transition to loading state
        let _ = self.state_manager.transition(AppState::LoadingModel);
        let config = config::load_config();
        let language = configured_model_language_hint(&config);

        match call_asr_initialize_with_language_fallback(client, model_id, device, language).await {
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
                            if let Err(err) = Self::start_recording_flow(
                                &state_manager,
                                &recording_controller,
                                &rpc_client,
                                &recording_context,
                                &current_session_id,
                            )
                            .await
                            {
                                log::warn!("Failed to start recording: {}", err);
                                *current_session_id.write().await = None;
                            }
                        } else if let Some(RecordingAction::Stop) = recording_action {
                            // Toggle mode: stop recording
                            if let Err(err) = Self::stop_recording_flow(
                                &recording_controller,
                                &rpc_client,
                                &recording_context,
                                &current_session_id,
                            )
                            .await
                            {
                                log::warn!("Failed to stop recording: {}", err);
                            }
                        }
                    }
                    HotkeyAction::PrimaryUp => {
                        // Only relevant for hold mode
                        if config.hotkeys.mode == HotkeyMode::Hold {
                            let hk = hotkey_manager.read().await;
                            if let Some(RecordingAction::Stop) =
                                hk.handle_primary_up(&state_manager)
                            {
                                if let Err(err) = Self::stop_recording_flow(
                                    &recording_controller,
                                    &rpc_client,
                                    &recording_context,
                                    &current_session_id,
                                )
                                .await
                                {
                                    log::warn!("Failed to stop recording: {}", err);
                                }
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
    ) -> Result<(), String> {
        if current_session_id.read().await.is_none() {
            return Err("No recording in progress".to_string());
        }

        let result = recording_controller
            .stop()
            .await
            .map_err(|e| format!("Failed to stop recording: {}", e))?;
        log::info!("Recording stopped: {:?}", result);
        Self::complete_stop_recording_flow(
            result,
            rpc_client,
            recording_context,
            current_session_id,
        )
        .await
    }

    /// Cancel recording and discard audio without transcription.
    async fn cancel_recording_flow(
        recording_controller: &Arc<RecordingController>,
        rpc_client: &Arc<RwLock<Option<RpcClient>>>,
        recording_context: &Arc<RwLock<Option<RecordingContext>>>,
        current_session_id: &Arc<RwLock<Option<String>>>,
    ) -> Result<(), String> {
        let session_id = current_session_id
            .read()
            .await
            .clone()
            .ok_or_else(|| "No recording in progress".to_string())?;

        recording_controller
            .cancel(CancelReason::UserButton)
            .await
            .map_err(|err| match err {
                crate::recording::RecordingError::NotRecording => {
                    "No recording in progress".to_string()
                }
                other => format!("Failed to cancel recording: {}", other),
            })?;

        // Clear host correlation state immediately so any late sidecar notifications are stale.
        *current_session_id.write().await = None;
        *recording_context.write().await = None;

        let params = json!({ "session_id": session_id });
        if let Some(client) = rpc_client.read().await.as_ref() {
            let cancel_result: Result<Value, RpcError> =
                client.call("recording.cancel", Some(params.clone())).await;
            match cancel_result {
                Ok(_) => {}
                Err(RpcError::Remote { kind, .. }) if kind == "E_METHOD_NOT_FOUND" => {
                    log::warn!(
                        "recording.cancel not supported; falling back to recording.stop and ignoring transcription result"
                    );
                    let fallback: Result<Value, RpcError> =
                        client.call("recording.stop", Some(params)).await;
                    if let Err(err) = fallback {
                        log::error!("Fallback recording.stop after cancel failed: {}", err);
                    }
                }
                Err(err) => {
                    log::error!("Failed to call recording.cancel RPC: {}", err);
                }
            }
        } else {
            log::warn!("Sidecar not connected during cancel; completed host cancellation only");
        }

        Ok(())
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
                if let Err(err) = Self::complete_stop_recording_flow(
                    stop_result,
                    rpc_client,
                    recording_context,
                    current_session_id,
                )
                .await
                {
                    log::warn!("Failed to complete max-duration stop flow: {}", err);
                }
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
    ) -> Result<(), String> {
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

            let session_id =
                session_id.ok_or_else(|| "No recording context for active session".to_string())?;
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
                    return Err(format!("Failed to call {} RPC: {}", stop_rpc_method, err));
                }
            } else {
                let cancel_result: Result<Value, _> =
                    client.call(stop_rpc_method, Some(params)).await;
                if let Err(err) = cancel_result {
                    return Err(format!("Failed to call {} RPC: {}", stop_rpc_method, err));
                }
            }
        } else if !too_short {
            return Err("Sidecar not connected".to_string());
        }

        // Too-short recordings don't produce transcription and should clear session context.
        if too_short {
            *current_session_id.write().await = None;
            *recording_context.write().await = None;
        }

        Ok(())
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
                        &[EVENT_STATE_CHANGED],
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
        let overlay_manager = Arc::clone(&self.overlay_manager);
        let overlay_enabled = Arc::clone(&self.overlay_enabled);

        tokio::spawn(async move {
            let mut receiver = recording_controller.subscribe();

            log::info!("Recording event loop started");

            while let Ok(event) = receiver.recv().await {
                if let Some(cue) = recording_event_audio_cue(&event) {
                    play_lifecycle_audio_cue(cue);
                }

                // Drive overlay show/hide based on recording lifecycle.
                let overlay_enabled = overlay_enabled.load(Ordering::Acquire);
                let recording_active =
                    overlay_recording_state_for_event(overlay_enabled, &event);
                if let (Some(recording_active), Some(ref handle)) = (recording_active, &app_handle)
                {
                    let config_store = FileOverlayConfigStore;
                    let backend = TauriOverlayWindowBackend::new(handle);
                    let mut manager = overlay_manager.lock().await;
                    if let Err(error) =
                        manager.handle_recording_state(recording_active, &config_store, &backend)
                    {
                        log::debug!("Overlay recording state transition failed: {error}");
                    }
                }

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
                        text: sidecar_text,
                        audio_duration_ms,
                        processing_duration_ms,
                        timestamp: _,
                    } => {
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_RECORDING_STATUS],
                                recording_status_event_payload(
                                    "idle",
                                    Some(session_id.as_str()),
                                    None,
                                    None,
                                ),
                                &event_seq,
                            );
                        }

                        // Snapshot focus context and timing marks for this session.
                        let (
                            focus_before,
                            mut timing_marks,
                            raw_text,
                            final_text,
                            language,
                            confidence,
                            force_clipboard_only,
                            force_clipboard_reason,
                        ) = {
                            let ctx = recording_context.read().await;
                            if let Some(ctx) = ctx.as_ref() {
                                (
                                    Some(ctx.focus_before.clone()),
                                    ctx.timing_marks.clone(),
                                    ctx.raw_text.clone(),
                                    ctx.final_text.clone(),
                                    ctx.language.clone(),
                                    ctx.confidence,
                                    ctx.force_clipboard_only,
                                    ctx.force_clipboard_reason.clone(),
                                )
                            } else {
                                (
                                    None,
                                    PipelineTimingMarks::default(),
                                    None,
                                    None,
                                    None,
                                    None,
                                    false,
                                    None,
                                )
                            }
                        };
                        let (raw_text, final_text) = resolve_transcript_texts(
                            &sidecar_text,
                            raw_text.as_deref(),
                            final_text.as_deref(),
                        );
                        log::info!(
                            "Transcription complete: session={}, text_len={}, text_sha256_prefix={}, audio={}ms, processing={}ms",
                            session_id,
                            final_text.len(),
                            sha256_prefix(&final_text),
                            audio_duration_ms,
                            processing_duration_ms
                        );

                        if log::log_enabled!(log::Level::Debug) {
                            log::debug!("Transcription text (debug): {}", final_text);
                        }
                        if timing_marks.t2_transcription_received.is_none() {
                            timing_marks.t2_transcription_received = Some(Instant::now());
                        }
                        let expected_focus = focus_before.as_ref();

                        // Sidecar output is already fully transformed (normalize/macros/replacements).
                        // Never apply replacements again on the Rust side.
                        if final_text.trim().is_empty() {
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

                        let mut result = if force_clipboard_only {
                            let text_with_suffix =
                                format!("{}{}", final_text, injection_config.suffix);
                            let forced_reason = force_clipboard_reason
                                .unwrap_or_else(|| DEVICE_REMOVED_CLIPBOARD_REASON.to_string());
                            let fallback_reason =
                                match crate::injection::set_clipboard_public(&text_with_suffix) {
                                    Ok(()) => forced_reason,
                                    Err(clipboard_error) => format!(
                                        "{}; clipboard fallback failed: {}; transcript preserved in history",
                                        forced_reason, clipboard_error
                                    ),
                                };

                            InjectionResult::ClipboardOnly {
                                reason: fallback_reason,
                                text_length: final_text.len(),
                                timestamp: chrono::Utc::now(),
                            }
                        } else {
                            inject_text(&final_text, expected_focus, &injection_config).await
                        };
                        timing_marks.t4_injection_completed = Some(Instant::now());

                        let pipeline_timings = pipeline_timings_from_marks(&timing_marks);
                        if let Some(timings) = pipeline_timings.as_ref() {
                            log_pipeline_timings(timings);
                        }

                        let mut injection_app_error: Option<AppError> = None;
                        if let InjectionResult::Failed { error, .. } = result.clone() {
                            let text_with_suffix =
                                format!("{}{}", final_text, injection_config.suffix);
                            let fallback_reason =
                                match crate::injection::set_clipboard_public(&text_with_suffix) {
                                    Ok(()) => format!(
                                        "{}; transcript copied to clipboard for manual paste",
                                        error
                                    ),
                                Err(clipboard_error) => format!(
                                    "{}; clipboard fallback failed: {}; transcript preserved in history",
                                    error, clipboard_error
                                ),
                            };

                            result = InjectionResult::ClipboardOnly {
                                reason: fallback_reason.clone(),
                                text_length: final_text.len(),
                                timestamp: chrono::Utc::now(),
                            };
                            injection_app_error = Some(injection_failure_app_error(
                                &fallback_reason,
                                final_text.len(),
                            ));
                        }

                        match &result {
                            InjectionResult::Injected { text_length, .. } => {
                                log::info!("Text injected: {} chars", text_length);
                            }
                            InjectionResult::ClipboardOnly {
                                reason,
                                text_length,
                                ..
                            } => {
                                log::info!("Clipboard-only mode: {}", reason);
                                if clipboard_only_requires_app_error(reason)
                                    && injection_app_error.is_none()
                                {
                                    injection_app_error =
                                        Some(injection_failure_app_error(reason, *text_length));
                                }
                            }
                            InjectionResult::Failed { error, .. } => {
                                log::error!("Injection failed: {}", error);
                            }
                        }

                        // Add to history and emit a shared transcript payload.
                        let mut transcript_entry = TranscriptEntry::new(
                            final_text.clone(),
                            audio_duration_ms as u32,
                            processing_duration_ms as u32,
                            HistoryInjectionResult::from_injection_result(&result),
                        )
                        .with_session_id(Uuid::parse_str(&session_id).ok())
                        .with_asr_metadata(language, confidence);
                        transcript_entry.raw_text = raw_text;
                        transcript_entry.final_text = final_text.clone();
                        transcript_entry.text = final_text;
                        if let Some(timings) = pipeline_timings.clone() {
                            transcript_entry = transcript_entry.with_timings(timings);
                        }

                        // Add to history
                        if let Some(ref handle) = app_handle {
                            let history = handle.state::<TranscriptHistory>();
                            history.push(transcript_entry.clone());
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_TRAY_UPDATE],
                                json!({
                                    "reason": "history_changed",
                                    "entry_id": transcript_entry.id,
                                }),
                                &event_seq,
                            );
                        }

                        // Emit canonical transcript event payload with shared seq.
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_TRANSCRIPT_COMPLETE],
                                transcript_complete_event_payload(&transcript_entry),
                                &event_seq,
                            );
                            if let Some(app_error) = injection_app_error {
                                emit_with_shared_seq(
                                    handle,
                                    &[EVENT_APP_ERROR],
                                    app_error_event_payload(&app_error),
                                    &event_seq,
                                );
                            }
                        }

                        // Clear context
                        *recording_context.write().await = None;
                        *current_session_id.write().await = None;
                    }
                    RecordingEvent::TranscriptionFailed {
                        session_id, error, ..
                    } => {
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_RECORDING_STATUS],
                                recording_status_event_payload(
                                    "idle",
                                    Some(session_id.as_str()),
                                    None,
                                    None,
                                ),
                                &event_seq,
                            );
                        }

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
                                &[EVENT_TRANSCRIPT_ERROR],
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
                    RecordingEvent::TranscriptionTimeout { session_id, .. } => {
                        if let Some(ref handle) = app_handle {
                            emit_with_shared_seq(
                                handle,
                                &[EVENT_RECORDING_STATUS],
                                recording_status_event_payload(
                                    "idle",
                                    Some(session_id.as_str()),
                                    None,
                                    None,
                                ),
                                &event_seq,
                            );
                        }
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
                            #[serde(default)]
                            language: Option<String>,
                            #[serde(default)]
                            raw_text: Option<String>,
                            #[serde(default)]
                            final_text: Option<String>,
                        }

                        if let Ok(params) =
                            serde_json::from_value::<TranscriptionParams>(event.params)
                        {
                            let (raw_text, final_text) = resolve_transcript_texts(
                                &params.text,
                                params.raw_text.as_deref(),
                                params.final_text.as_deref(),
                            );
                            let (audio_duration_ms, processing_duration_ms) = {
                                let mut stop_audio_duration_ms = None;
                                let mut ctx = recording_context.write().await;
                                if let Some(ctx) = ctx.as_mut() {
                                    if ctx.session_id == params.session_id {
                                        ctx.timing_marks.t2_transcription_received =
                                            Some(Instant::now());
                                        stop_audio_duration_ms = ctx.audio_duration_ms;
                                        ctx.raw_text = Some(raw_text.clone());
                                        ctx.final_text = Some(final_text.clone());
                                        ctx.language = params
                                            .language
                                            .as_deref()
                                            .map(str::trim)
                                            .filter(|value| !value.is_empty())
                                            .map(ToString::to_string);
                                        ctx.confidence = params.confidence.map(|v| v as f32);
                                    }
                                }
                                map_transcription_complete_durations(
                                    params.duration_ms,
                                    stop_audio_duration_ms,
                                )
                            };

                            let result = TranscriptionResult {
                                session_id: params.session_id,
                                // Sidecar text is authoritative and already post-processed.
                                text: final_text,
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
                            #[serde(default)]
                            current_file: Option<String>,
                            #[serde(default)]
                            files_completed: Option<u64>,
                            #[serde(default)]
                            files_total: Option<u64>,
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
                                let model_progress_data = model_progress_from_parts(
                                    parsed_model.as_ref().and_then(|m| m.model_id.clone()),
                                    progress.current,
                                    progress.total,
                                    progress.unit.clone(),
                                    progress.stage.clone(),
                                    progress.current_file.clone(),
                                    progress.files_completed,
                                    progress.files_total,
                                );
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

                        // Emit canonical sidecar status payload with shared seq.
                        if let Some(ref handle) = app_handle {
                            let seq = next_seq(&event_seq);
                            emit_with_existing_seq_to_all_windows(
                                handle,
                                EVENT_SIDECAR_STATUS,
                                canonical_sidecar_payload,
                                seq,
                            );
                        }
                    }
                    "event.model_progress" => {
                        if let Ok(params) =
                            serde_json::from_value::<SidecarModelProgress>(event.params)
                        {
                            let model_progress_data = model_progress_from_parts(
                                params.model_id,
                                params.current,
                                params.total,
                                params.unit,
                                params.stage,
                                params.current_file,
                                params.files_completed,
                                params.files_total,
                            );
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
        self.stop_sidecar_runtime().await;

        // Shutdown hotkey manager
        {
            let mut hk = self.hotkey_manager.write().await;
            hk.shutdown();
        }

        log::info!("Integration manager shutdown complete");
    }

    /// Return recent sidecar logs captured by the supervisor.
    pub async fn recent_sidecar_logs(&self, count: usize) -> Vec<String> {
        let mut supervisor = self.supervisor.lock().await;
        supervisor.recent_captured_log_lines(count)
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
    use std::fs;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Compile-time verification that AppHandle implements AppEventBroadcaster.
    /// This ensures the mock-based broadcast tests validate the same trait
    /// contract that the real Tauri AppHandle uses at runtime.
    const _: () = {
        fn assert_impl<T: AppEventBroadcaster>() {}
        fn check() {
            assert_impl::<AppHandle>();
        }
    };

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

    struct ChildProcessGuard {
        child: Option<Child>,
    }

    struct ScopedAudioCueManagerOverride;

    impl ScopedAudioCueManagerOverride {
        fn install_with_missing_sounds(base_dir: &Path) -> Self {
            let missing_sounds_dir = base_dir.join("missing-audio-cues");
            AUDIO_CUE_MANAGER.with(|slot| {
                *slot.borrow_mut() = Some(AudioCueManager::with_sounds_dir(
                    missing_sounds_dir,
                    true,
                ));
            });
            Self
        }
    }

    impl Drop for ScopedAudioCueManagerOverride {
        fn drop(&mut self) {
            AUDIO_CUE_MANAGER.with(|slot| {
                *slot.borrow_mut() = None;
            });
        }
    }

    impl ChildProcessGuard {
        fn new(child: Child) -> Self {
            Self { child: Some(child) }
        }

        fn child_mut(&mut self) -> &mut Child {
            self.child
                .as_mut()
                .expect("child process should be present")
        }

        fn reap_now(&mut self) {
            if let Some(mut child) = self.child.take() {
                if child
                    .try_wait()
                    .expect("try_wait on mock sidecar should not fail")
                    .is_none()
                {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }

    impl Drop for ChildProcessGuard {
        fn drop(&mut self) {
            if let Some(mut child) = self.child.take() {
                if child.try_wait().ok().flatten().is_none() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }

    fn spawn_mock_sidecar_recording_process(call_log_path: &Path) -> Child {
        let script = r#"
import json
import sys

log_path = sys.argv[1]
active_session = None

def append_call(method, params):
    with open(log_path, 'a', encoding='utf-8') as handle:
        handle.write(json.dumps({"method": method, "params": params}) + "\n")
        handle.flush()

for raw in sys.stdin:
    line = raw.strip()
    if not line:
        continue
    request = json.loads(line)
    method = request.get("method")
    req_id = request.get("id")
    params = request.get("params") or {}
    append_call(method, params)

    if method == "recording.start":
        active_session = params.get("session_id")
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {"session_id": active_session}
        }
        print(json.dumps(response), flush=True)
    elif method == "recording.stop":
        session_id = params.get("session_id") or active_session
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {"audio_duration_ms": 4321}
        }
        print(json.dumps(response), flush=True)
        notification = {
            "jsonrpc": "2.0",
            "method": "event.transcription_complete",
            "params": {
                "session_id": session_id,
                "text": "",
                "duration_ms": 987
            }
        }
        print(json.dumps(notification), flush=True)
    elif method == "recording.cancel":
        session_id = params.get("session_id") or active_session
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {"cancelled": True, "session_id": session_id}
        }
        print(json.dumps(response), flush=True)
    elif method == "system.shutdown":
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {"status": "shutting_down"}
        }
        print(json.dumps(response), flush=True)
        break
    else:
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "error": {
                "code": -32601,
                "message": "Method not found",
                "data": {"kind": "E_METHOD_NOT_FOUND"}
            }
        }
        print(json.dumps(response), flush=True)
"#;

        Command::new("python3")
            .arg("-u")
            .arg("-c")
            .arg(script)
            .arg(call_log_path.as_os_str())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn mock recording sidecar")
    }

    fn spawn_mock_sidecar_model_install_fallback_process(call_log_path: &Path) -> Child {
        let script = r#"
import json
import sys

log_path = sys.argv[1]

def append_call(method, params):
    with open(log_path, 'a', encoding='utf-8') as handle:
        handle.write(json.dumps({"method": method, "params": params}) + "\n")
        handle.flush()

for raw in sys.stdin:
    line = raw.strip()
    if not line:
        continue
    request = json.loads(line)
    method = request.get("method")
    req_id = request.get("id")
    params = request.get("params") or {}
    append_call(method, params)

    if method == "model.install":
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "error": {
                "code": -32601,
                "message": "Method not found",
                "data": {"kind": "E_METHOD_NOT_FOUND"}
            }
        }
        print(json.dumps(response), flush=True)
    elif method == "model.download":
        model_id = params.get("model_id") or "nvidia/parakeet-tdt-0.6b-v3"
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "model_id": model_id,
                "revision": "legacy-r1",
                "cache_path": "/tmp/openvoicy-cache/" + model_id.replace("/", "_"),
                "status": "ready"
            }
        }
        print(json.dumps(response), flush=True)
    elif method == "asr.initialize":
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {"status": "ready"}
        }
        print(json.dumps(response), flush=True)
    elif method == "system.shutdown":
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {"status": "shutting_down"}
        }
        print(json.dumps(response), flush=True)
        break
    else:
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "error": {
                "code": -32601,
                "message": "Method not found",
                "data": {"kind": "E_METHOD_NOT_FOUND"}
            }
        }
        print(json.dumps(response), flush=True)
"#;

        Command::new("python3")
            .arg("-u")
            .arg("-c")
            .arg(script)
            .arg(call_log_path.as_os_str())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn mock model-install fallback sidecar")
    }

    fn spawn_mock_sidecar_asr_language_retry_process(
        call_log_path: &Path,
        first_error_kind: &str,
        first_error_message: &str,
    ) -> Child {
        let script = r#"
import json
import sys

log_path = sys.argv[1]
first_error_kind = sys.argv[2]
first_error_message = sys.argv[3]

def append_call(method, params):
    with open(log_path, 'a', encoding='utf-8') as handle:
        handle.write(json.dumps({"method": method, "params": params}) + "\n")
        handle.flush()

for raw in sys.stdin:
    line = raw.strip()
    if not line:
        continue
    request = json.loads(line)
    method = request.get("method")
    req_id = request.get("id")
    params = request.get("params") or {}
    append_call(method, params)

    if method == "asr.initialize":
        if "language" in params:
            response = {
                "jsonrpc": "2.0",
                "id": req_id,
                "error": {
                    "code": -32602,
                    "message": first_error_message,
                    "data": {"kind": first_error_kind}
                }
            }
        else:
            response = {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {"status": "ready"}
            }
        print(json.dumps(response), flush=True)
    elif method == "system.shutdown":
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {"status": "shutting_down"}
        }
        print(json.dumps(response), flush=True)
        break
    else:
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "error": {
                "code": -32601,
                "message": "Method not found",
                "data": {"kind": "E_METHOD_NOT_FOUND"}
            }
        }
        print(json.dumps(response), flush=True)
"#;

        Command::new("python3")
            .arg("-u")
            .arg("-c")
            .arg(script)
            .arg(call_log_path.as_os_str())
            .arg(first_error_kind)
            .arg(first_error_message)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn mock asr-language-retry sidecar")
    }

    fn spawn_mock_sidecar_asr_initialize_error_process(
        call_log_path: &Path,
        error_kind: &str,
        error_message: &str,
    ) -> Child {
        let script = r#"
import json
import sys

log_path = sys.argv[1]
error_kind = sys.argv[2]
error_message = sys.argv[3]

def append_call(method, params):
    with open(log_path, 'a', encoding='utf-8') as handle:
        handle.write(json.dumps({"method": method, "params": params}) + "\n")
        handle.flush()

for raw in sys.stdin:
    line = raw.strip()
    if not line:
        continue
    request = json.loads(line)
    method = request.get("method")
    req_id = request.get("id")
    params = request.get("params") or {}
    append_call(method, params)

    if method == "asr.initialize":
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "error": {
                "code": -32000,
                "message": error_message,
                "data": {"kind": error_kind}
            }
        }
        print(json.dumps(response), flush=True)
    elif method == "system.shutdown":
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {"status": "shutting_down"}
        }
        print(json.dumps(response), flush=True)
        break
    else:
        response = {
            "jsonrpc": "2.0",
            "id": req_id,
            "error": {
                "code": -32601,
                "message": "Method not found",
                "data": {"kind": "E_METHOD_NOT_FOUND"}
            }
        }
        print(json.dumps(response), flush=True)
"#;

        Command::new("python3")
            .arg("-u")
            .arg("-c")
            .arg(script)
            .arg(call_log_path.as_os_str())
            .arg(error_kind)
            .arg(error_message)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn mock asr-initialize-error sidecar")
    }

    fn read_mock_call_log(path: &Path) -> Vec<Value> {
        let raw = fs::read_to_string(path).unwrap_or_default();
        raw.lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .collect()
    }

    async fn wait_until<F>(timeout: Duration, mut condition: F)
    where
        F: FnMut() -> bool,
    {
        let start = tokio::time::Instant::now();
        while !condition() {
            if start.elapsed() > timeout {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn wait_for_recording_event<F>(
        receiver: &mut tokio::sync::broadcast::Receiver<RecordingEvent>,
        timeout: Duration,
        mut predicate: F,
    ) -> RecordingEvent
    where
        F: FnMut(&RecordingEvent) -> bool,
    {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let now = tokio::time::Instant::now();
            assert!(
                now < deadline,
                "timed out waiting for expected recording event"
            );
            let remaining = deadline.saturating_duration_since(now);
            match tokio::time::timeout(remaining, receiver.recv()).await {
                Ok(Ok(event)) => {
                    println!(
                        "[RECORDING_FLOW][EVENT] {} {}",
                        chrono::Utc::now().to_rfc3339(),
                        serde_json::to_string(&event).unwrap_or_else(|_| "<serialize-error>".into())
                    );
                    if predicate(&event) {
                        return event;
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped))) => {
                    println!("[RECORDING_FLOW][EVENT] skipped {skipped} lagged events");
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                    panic!("recording event channel closed unexpectedly");
                }
                Err(_) => {
                    panic!("timed out waiting for recording event");
                }
            }
        }
    }

    async fn wait_for_state_event<F>(
        receiver: &mut tokio::sync::broadcast::Receiver<crate::state::StateEvent>,
        timeout: Duration,
        mut predicate: F,
    ) -> crate::state::StateEvent
    where
        F: FnMut(&crate::state::StateEvent) -> bool,
    {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let now = tokio::time::Instant::now();
            assert!(now < deadline, "timed out waiting for expected state event");
            let remaining = deadline.saturating_duration_since(now);
            match tokio::time::timeout(remaining, receiver.recv()).await {
                Ok(Ok(event)) => {
                    println!(
                        "[RECORDING_FLOW][STATE_EVENT] {} state={:?} enabled={} detail={:?}",
                        event.timestamp.to_rfc3339(),
                        event.state,
                        event.enabled,
                        event.detail
                    );
                    if predicate(&event) {
                        return event;
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped))) => {
                    println!("[RECORDING_FLOW][STATE_EVENT] skipped {skipped} lagged events");
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                    panic!("state event channel closed unexpectedly");
                }
                Err(_) => {
                    panic!("timed out waiting for state event");
                }
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
    fn test_state_changed_emits_canonical_only_with_shared_seq() {
        let broadcaster = MockBroadcaster::with_windows(&["main"]);
        let seq_counter = Arc::new(AtomicU64::new(1));
        let event = StateEvent {
            state: AppState::Recording,
            enabled: true,
            detail: Some("capturing".to_string()),
            timestamp: chrono::Utc::now(),
        };

        println!("[EVENT_TEST] Emitting canonical state event");
        let emitted_seq = emit_with_shared_seq_for_broadcaster(
            &broadcaster,
            &[EVENT_STATE_CHANGED],
            state_changed_event_payload(&event),
            &seq_counter,
        );
        println!("[EVENT_TEST] state event emitted with seq={emitted_seq}");

        let main_events = broadcaster.received_event_names("main");
        assert_eq!(
            main_events,
            vec![EVENT_STATE_CHANGED.to_string()]
        );
        assert!(
            !main_events.contains(&"state_changed".to_string()),
            "legacy state_changed alias must not be emitted"
        );

        let payloads = broadcaster.received_payloads("main");
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0].get("seq").and_then(Value::as_u64), Some(1));
        assert_eq!(
            payloads[0].get("detail").and_then(Value::as_str),
            Some("capturing")
        );
        assert!(payloads[0].get("error_detail").is_none());
    }

    #[test]
    fn test_transcript_events_emit_canonical_only_with_shared_seq() {
        let broadcaster = MockBroadcaster::with_windows(&["main"]);
        let seq_counter = Arc::new(AtomicU64::new(1));
        let session_id = Uuid::new_v4();
        let entry = TranscriptEntry::new(
            "hello world".to_string(),
            800,
            120,
            HistoryInjectionResult::Injected,
        )
        .with_session_id(Some(session_id))
        .with_asr_metadata(Some("en".to_string()), Some(0.91));

        println!("[EVENT_TEST] Emitting canonical transcript complete event");
        emit_with_shared_seq_for_broadcaster(
            &broadcaster,
            &[EVENT_TRANSCRIPT_COMPLETE],
            transcript_complete_event_payload(&entry),
            &seq_counter,
        );

        let app_error = AppError::new(
            ErrorKind::SidecarCrash.to_sidecar(),
            "Sidecar crashed",
            Some(json!({ "restart_count": 1 })),
            false,
        );
        println!("[EVENT_TEST] Emitting canonical transcript error event");
        emit_with_shared_seq_for_broadcaster(
            &broadcaster,
            &[EVENT_TRANSCRIPT_ERROR],
            transcription_error_event_payload("session-1", &app_error),
            &seq_counter,
        );

        let events = broadcaster.received_event_names("main");
        assert_eq!(
            events,
            vec![
                EVENT_TRANSCRIPT_COMPLETE.to_string(),
                EVENT_TRANSCRIPT_ERROR.to_string(),
            ]
        );
        assert!(
            !events.contains(&"transcription:complete".to_string()),
            "legacy transcription:complete alias must not be emitted"
        );
        assert!(
            !events.contains(&"transcription:error".to_string()),
            "legacy transcription:error alias must not be emitted"
        );

        let payloads = broadcaster.received_payloads("main");
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0].get("seq").and_then(Value::as_u64), Some(1));
        assert_eq!(payloads[1].get("seq").and_then(Value::as_u64), Some(2));

        let complete_entry = &payloads[0]["entry"];
        assert_eq!(
            complete_entry.get("session_id").and_then(Value::as_str),
            Some(session_id.to_string().as_str())
        );
        assert_eq!(
            complete_entry.get("text").and_then(Value::as_str),
            complete_entry.get("final_text").and_then(Value::as_str)
        );
        assert_eq!(
            complete_entry.get("raw_text").and_then(Value::as_str),
            complete_entry.get("final_text").and_then(Value::as_str)
        );
        assert_eq!(
            complete_entry.get("language").and_then(Value::as_str),
            Some("en")
        );

        assert!(payloads[1].get("error").is_some());
        assert_eq!(
            payloads[1].get("message").and_then(Value::as_str),
            Some("Sidecar crashed")
        );
    }

    #[test]
    fn test_sidecar_status_state_mapping_covers_all_canonical_states() {
        println!("[EVENT_TEST] Validating sidecar status state normalization");
        let states = [
            ("starting", "starting"),
            ("ready", "ready"),
            ("failed", "failed"),
            ("restarting", "restarting"),
            ("stopped", "stopped"),
            ("recording", "ready"),
        ];

        for (input_state, expected_state) in states {
            let payload =
                sidecar_status_payload_from_status_event(Some(input_state), None, Some(2));
            assert_eq!(
                payload.get("state").and_then(Value::as_str),
                Some(expected_state),
                "state mapping mismatch for input={input_state}"
            );
            assert_eq!(
                payload.get("restart_count").and_then(Value::as_u64),
                Some(2)
            );
        }
    }

    #[test]
    fn test_recording_status_transition_payloads_cover_recording_lifecycle() {
        println!("[EVENT_TEST] Validating recording lifecycle payload transitions");
        let idle = recording_status_event_payload("idle", None, None, None);
        let recording = recording_status_event_payload(
            "recording",
            Some("session-1"),
            Some("2026-02-19T03:00:00Z".to_string()),
            None,
        );
        let transcribing =
            recording_status_event_payload("transcribing", Some("session-1"), None, Some(1200));
        let cancelled = recording_status_event_payload("idle", None, None, None);

        assert_eq!(idle.get("phase").and_then(Value::as_str), Some("idle"));
        assert!(idle.get("session_id").is_none());

        assert_eq!(
            recording.get("phase").and_then(Value::as_str),
            Some("recording")
        );
        assert_eq!(
            recording.get("session_id").and_then(Value::as_str),
            Some("session-1")
        );
        assert!(recording.get("audio_ms").is_none());

        assert_eq!(
            transcribing.get("phase").and_then(Value::as_str),
            Some("transcribing")
        );
        assert_eq!(
            transcribing.get("session_id").and_then(Value::as_str),
            Some("session-1")
        );
        assert_eq!(
            transcribing.get("audio_ms").and_then(Value::as_u64),
            Some(1200)
        );

        assert_eq!(cancelled.get("phase").and_then(Value::as_str), Some("idle"));
        assert!(cancelled.get("session_id").is_none());
    }

    #[test]
    fn test_seq_shared_counter_across_event_types_starts_at_one() {
        let broadcaster = MockBroadcaster::with_windows(&["main"]);
        let seq_counter = Arc::new(AtomicU64::new(1));

        println!("[EVENT_TEST] Emitting state event with shared counter");
        let first_seq = emit_with_shared_seq_for_broadcaster(
            &broadcaster,
            &[EVENT_STATE_CHANGED],
            json!({ "state": "idle", "enabled": true }),
            &seq_counter,
        );
        println!("[EVENT_TEST] Emitting model event with shared counter");
        let second_seq = emit_with_shared_seq_for_broadcaster(
            &broadcaster,
            &[EVENT_MODEL_STATUS],
            json!({ "status": "ready", "model_id": "parakeet" }),
            &seq_counter,
        );

        assert_eq!(first_seq, 1);
        assert_eq!(second_seq, 2);

        let payloads = broadcaster.received_payloads("main");
        assert_eq!(payloads[0].get("seq").and_then(Value::as_u64), Some(1));
        assert_eq!(payloads[1].get("seq").and_then(Value::as_u64), Some(2));
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
    fn test_stale_drop_with_realistic_notification_payloads() {
        let active = Some("session-active");

        // Transcription complete from active session → accepted
        let params = json!({ "session_id": "session-active", "text": "hello", "duration_ms": 100 });
        assert!(!is_stale_session(extract_session_id(&params), active));

        // Transcription complete from stale session → dropped
        let params = json!({ "session_id": "session-old", "text": "stale", "duration_ms": 50 });
        assert!(is_stale_session(extract_session_id(&params), active));

        // Transcription error from stale session → dropped
        let params = json!({ "session_id": "session-old", "error": "timeout" });
        assert!(is_stale_session(extract_session_id(&params), active));

        // Transcription error from active session → accepted
        let params = json!({ "session_id": "session-active", "error": "decoder crash" });
        assert!(!is_stale_session(extract_session_id(&params), active));

        // Audio level event (no session_id) → always forwarded, never stale
        let params = json!({ "level": 0.5 });
        assert!(!is_stale_session(extract_session_id(&params), active));
        assert!(!is_stale_session(extract_session_id(&params), None));

        // Status event (no session_id) → always forwarded
        let params = json!({ "state": "ready", "restart_count": 0 });
        assert!(!is_stale_session(extract_session_id(&params), active));

        // Stale notification message format is meaningful
        let msg = stale_notification_message(Some("session-old"), active);
        assert!(msg.contains("session-old"));
        assert!(msg.contains("session-active"));
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
    fn test_model_progress_from_parts_includes_canonical_fields() {
        let progress = model_progress_from_parts(
            Some("nvidia/parakeet-tdt-0.6b-v3".to_string()),
            128,
            Some(1024),
            Some("bytes".to_string()),
            Some("downloading".to_string()),
            Some("model.nemo".to_string()),
            Some(1),
            Some(2),
        );

        assert_eq!(
            progress.model_id,
            Some("nvidia/parakeet-tdt-0.6b-v3".to_string())
        );
        assert_eq!(progress.current, 128);
        assert_eq!(progress.total, Some(1024));
        assert_eq!(progress.unit, "bytes");
        assert_eq!(progress.stage.as_deref(), Some("downloading"));
        assert_eq!(progress.current_file.as_deref(), Some("model.nemo"));
        assert_eq!(progress.files_completed, Some(1));
        assert_eq!(progress.files_total, Some(2));
    }

    #[test]
    fn test_model_progress_from_parts_defaults_unit() {
        let progress = model_progress_from_parts(None, 1, None, None, None, None, None, None);

        assert_eq!(progress.unit, "bytes");
        assert!(progress.model_id.is_none());
        assert!(progress.stage.is_none());
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
    fn test_configured_model_language_hint_trims_and_preserves_supported_values() {
        let mut config = config::AppConfig::default();
        config.model = Some(config::ModelConfig {
            model_id: None,
            device: None,
            preferred_device: "auto".to_string(),
            language: Some(" ja ".to_string()),
        });

        assert_eq!(
            configured_model_language_hint(&config),
            Some("ja".to_string())
        );
    }

    #[test]
    fn test_configured_model_language_hint_omits_blank_values() {
        let mut config = config::AppConfig::default();
        config.model = Some(config::ModelConfig {
            model_id: None,
            device: None,
            preferred_device: "auto".to_string(),
            language: Some("   ".to_string()),
        });

        assert_eq!(configured_model_language_hint(&config), None);
    }

    #[test]
    fn test_asr_initialize_params_includes_optional_language_when_present() {
        let params = asr_initialize_params("parakeet", "cuda", Some("en"));
        assert_eq!(
            params,
            json!({
                "model_id": "parakeet",
                "device_pref": "cuda",
                "language": "en"
            })
        );
    }

    #[test]
    fn test_asr_initialize_params_omits_language_when_absent_or_blank() {
        let no_language = asr_initialize_params("parakeet", "auto", None);
        assert_eq!(
            no_language,
            json!({
                "model_id": "parakeet",
                "device_pref": "auto"
            })
        );

        let blank_language = asr_initialize_params("parakeet", "auto", Some("   "));
        assert_eq!(blank_language, no_language);
    }

    #[test]
    fn test_asr_initialize_language_rejected_detects_invalid_language_param_errors() {
        let error = RpcError::Remote {
            code: -32602,
            kind: "E_INVALID_PARAMS".to_string(),
            message: "Invalid params: unknown field 'language'".to_string(),
        };

        assert!(asr_initialize_language_rejected(&error));
    }

    #[test]
    fn test_asr_initialize_language_rejected_detects_method_not_found_for_compatibility_retry() {
        let error = RpcError::Remote {
            code: -32601,
            kind: "E_METHOD_NOT_FOUND".to_string(),
            message: "Method not found".to_string(),
        };

        assert!(asr_initialize_language_rejected(&error));
    }

    #[test]
    fn test_asr_initialize_language_rejected_ignores_unrelated_errors() {
        let error = RpcError::Remote {
            code: -32001,
            kind: "E_NOT_READY".to_string(),
            message: "ASR backend not initialized".to_string(),
        };

        assert!(!asr_initialize_language_rejected(&error));
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
    fn test_purge_affects_configured_model_when_status_ids_include_configured() {
        assert!(purge_affects_configured_model(
            "configured/model",
            &["configured/model".to_string()]
        ));
        assert!(purge_affects_configured_model(
            "configured/model",
            &[
                "openai/whisper-large".to_string(),
                "configured/model".to_string(),
            ]
        ));
    }

    #[test]
    fn test_purge_affects_configured_model_false_for_empty_or_unrelated_status_ids() {
        assert!(!purge_affects_configured_model("configured/model", &[]));
        assert!(!purge_affects_configured_model(
            "configured/model",
            &["other/model".to_string()]
        ));
    }

    #[test]
    fn test_purge_status_model_ids_uses_reported_ids_for_targeted_purge() {
        let ids = purge_status_model_ids(
            Some("openai/whisper-large"),
            "nvidia/parakeet-tdt-0.6b-v3",
            &[
                " openai/whisper-large ".to_string(),
                "".to_string(),
                "openai/whisper-large".to_string(),
            ],
        );
        assert_eq!(ids, vec!["openai/whisper-large".to_string()]);
    }

    #[test]
    fn test_normalized_purged_model_ids_trims_dedupes_and_drops_empty_ids() {
        let ids = normalized_purged_model_ids(&[
            " openai/whisper-large ".to_string(),
            "".to_string(),
            "openai/whisper-large".to_string(),
            "nvidia/parakeet-tdt-0.6b-v3".to_string(),
        ]);
        assert_eq!(
            ids,
            vec![
                "openai/whisper-large".to_string(),
                "nvidia/parakeet-tdt-0.6b-v3".to_string(),
            ]
        );
    }

    #[test]
    fn test_purge_all_removed_count_uses_reported_ids_not_fallback_status_ids() {
        let configured = "nvidia/parakeet-tdt-0.6b-v3";
        let reported = normalized_purged_model_ids(&[]);
        let status_model_ids = purge_status_model_ids(None, configured, &reported);
        assert_eq!(reported.len(), 0);
        assert_eq!(status_model_ids, vec![configured.to_string()]);
    }

    #[test]
    fn test_purge_status_model_ids_targeted_returns_empty_when_sidecar_reports_no_purges() {
        let ids = purge_status_model_ids(
            Some("openai/whisper-large"),
            "nvidia/parakeet-tdt-0.6b-v3",
            &[],
        );
        assert!(ids.is_empty());
    }

    #[test]
    fn test_targeted_purge_with_no_reported_ids_does_not_affect_configured_model() {
        let configured = "nvidia/parakeet-tdt-0.6b-v3";
        let status_model_ids = purge_status_model_ids(Some("openai/whisper-large"), configured, &[]);
        assert!(status_model_ids.is_empty());
        assert!(!purge_affects_configured_model(
            configured,
            &status_model_ids
        ));
    }

    #[test]
    fn test_purge_status_model_ids_uses_reported_ids_for_purge_all() {
        let ids = purge_status_model_ids(
            None,
            "nvidia/parakeet-tdt-0.6b-v3",
            &[
                " openai/whisper-large ".to_string(),
                "".to_string(),
                "openai/whisper-large".to_string(),
                "nvidia/parakeet-tdt-0.6b-v3".to_string(),
            ],
        );
        assert_eq!(
            ids,
            vec![
                "openai/whisper-large".to_string(),
                "nvidia/parakeet-tdt-0.6b-v3".to_string(),
            ]
        );
    }

    #[test]
    fn test_purge_status_model_ids_falls_back_to_configured_model_when_missing() {
        let ids = purge_status_model_ids(None, "nvidia/parakeet-tdt-0.6b-v3", &[]);
        assert_eq!(ids, vec!["nvidia/parakeet-tdt-0.6b-v3".to_string()]);
    }

    /// Purging an unrelated model must NOT update global model state or
    /// recording readiness. The model:status event IS still emitted (for the
    /// purged model) so UI consumers can observe cache transitions, but the
    /// configured model's internal status stays untouched.
    #[test]
    fn test_purge_unrelated_model_does_not_affect_configured_state() {
        let configured = "nvidia/parakeet-tdt-0.6b-v3";

        // Purging a different model should not affect configured model status.
        assert!(!purge_affects_configured_model(
            configured,
            &["openai/whisper-large".to_string()]
        ));
        // Exact match should affect status.
        assert!(purge_affects_configured_model(
            configured,
            &["nvidia/parakeet-tdt-0.6b-v3".to_string()]
        ));
    }

    /// Regression (10l6): purging a non-configured model must still produce a
    /// model:status payload targeting the purged model ID so UI consumers can
    /// observe cache transitions for any model.
    #[test]
    fn test_purge_non_configured_model_event_payload_carries_purged_model_id() {
        let purged = "openai/whisper-large";
        let configured = "nvidia/parakeet-tdt-0.6b-v3";

        // The purge does not affect global state…
        assert!(!purge_affects_configured_model(
            configured,
            &[purged.to_string()]
        ));

        // …but the event payload must carry the purged model's ID.
        let payload = model_status_event_payload(
            ModelStatus::Missing,
            Some(purged.to_string()),
            None,
            None,
            None,
        );
        assert_eq!(payload.model_id, purged);
        assert_eq!(payload.status, "missing");
    }

    #[test]
    fn test_purge_success_path_emits_model_status_for_non_configured_model() {
        let configured = "nvidia/parakeet-tdt-0.6b-v3";
        let purged = "openai/whisper-large";
        let status_model_ids = purge_status_model_ids(
            Some(purged),
            configured,
            &[purged.to_string(), "".to_string()],
        );

        assert_eq!(status_model_ids, vec![purged.to_string()]);
        assert!(
            !purge_affects_configured_model(configured, &status_model_ids),
            "targeted purge for a different model must not mutate configured-model state"
        );

        let broadcaster = MockBroadcaster::with_windows(&["main"]);
        let seq_counter = Arc::new(AtomicU64::new(1));

        emit_missing_model_status_for_purged_models_with_broadcaster(
            &broadcaster,
            &seq_counter,
            &status_model_ids,
        );

        let events = broadcaster.received_event_names("main");
        assert_eq!(events, vec![EVENT_MODEL_STATUS.to_string()]);

        let payloads = broadcaster.received_payloads("main");
        assert_eq!(payloads.len(), 1);
        assert_eq!(
            payloads[0].get("model_id").and_then(Value::as_str),
            Some("openai/whisper-large")
        );
        assert_eq!(payloads[0].get("status").and_then(Value::as_str), Some("missing"));
        assert_eq!(payloads[0].get("seq").and_then(Value::as_u64), Some(1));
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
    fn test_resolve_transcript_texts_defaults_to_single_processed_text() {
        let (raw_text, final_text) = resolve_transcript_texts("hello", None, None);
        assert_eq!(raw_text, "hello");
        assert_eq!(final_text, "hello");
    }

    #[test]
    fn test_resolve_transcript_texts_prefers_sidecar_raw_and_final_fields() {
        let (raw_text, final_text) =
            resolve_transcript_texts("compat-text", Some("raw asr"), Some("post processed"));
        assert_eq!(raw_text, "raw asr");
        assert_eq!(final_text, "post processed");
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
    fn test_recording_event_audio_cue_mapping_for_lifecycle_events() {
        let now = chrono::Utc::now();
        let session_id = "session-1".to_string();

        // Started is None because start cue is played pre-roll in
        // start_recording_flow before mic capture begins.
        assert_eq!(
            recording_event_audio_cue(&RecordingEvent::Started {
                session_id: session_id.clone(),
                timestamp: now,
            }),
            None
        );

        assert_eq!(
            recording_event_audio_cue(&RecordingEvent::Stopped {
                session_id: session_id.clone(),
                duration_ms: 1200,
                timestamp: now,
            }),
            Some(CueType::StopRecording)
        );

        assert_eq!(
            recording_event_audio_cue(&RecordingEvent::Cancelled {
                session_id: session_id.clone(),
                reason: CancelReason::UserButton,
                timestamp: now,
            }),
            Some(CueType::CancelRecording)
        );

        assert_eq!(
            recording_event_audio_cue(&RecordingEvent::TranscriptionFailed {
                session_id,
                error: "decoder crash".to_string(),
                timestamp: now,
            }),
            Some(CueType::Error)
        );
    }

    #[test]
    fn test_recording_event_audio_cue_mapping_ignores_non_cue_events() {
        let now = chrono::Utc::now();

        assert_eq!(
            recording_event_audio_cue(&RecordingEvent::TooShort {
                duration_ms: 100,
                timestamp: now,
            }),
            None
        );

        assert_eq!(
            recording_event_audio_cue(&RecordingEvent::TranscriptionTimeout {
                session_id: "session-1".to_string(),
                timestamp: now,
            }),
            None
        );
    }

    #[test]
    fn test_lifecycle_audio_cues_disabled_in_test_builds() {
        assert!(!should_play_lifecycle_audio_cues());
    }

    #[test]
    fn test_overlay_active_during_recording_and_transcribing() {
        let now = chrono::Utc::now();
        let session_id = "session-1".to_string();

        // Active during recording
        assert!(is_overlay_recording_active(&RecordingEvent::Started {
            session_id: session_id.clone(),
            timestamp: now,
        }));

        // Active during transcription (Stopped = mic off, transcription in progress)
        assert!(is_overlay_recording_active(&RecordingEvent::Stopped {
            session_id: session_id.clone(),
            duration_ms: 1200,
            timestamp: now,
        }));
    }

    #[test]
    fn test_overlay_inactive_on_terminal_recording_events() {
        let now = chrono::Utc::now();
        let session_id = "session-1".to_string();

        assert!(!is_overlay_recording_active(
            &RecordingEvent::TranscriptionComplete {
                session_id: session_id.clone(),
                text: "hello".to_string(),
                audio_duration_ms: 1200,
                processing_duration_ms: 300,
                timestamp: now,
            }
        ));

        assert!(!is_overlay_recording_active(
            &RecordingEvent::TranscriptionFailed {
                session_id: session_id.clone(),
                error: "decoder crash".to_string(),
                timestamp: now,
            }
        ));

        assert!(!is_overlay_recording_active(&RecordingEvent::Cancelled {
            session_id,
            reason: CancelReason::UserButton,
            timestamp: now,
        }));

        assert!(!is_overlay_recording_active(&RecordingEvent::TooShort {
            duration_ms: 50,
            timestamp: now,
        }));
    }

    #[test]
    fn test_overlay_config_gate_skips_work_when_disabled_state_unchanged() {
        assert!(should_apply_overlay_config_change(None, false));
        assert!(!should_apply_overlay_config_change(Some(false), false));
        assert!(should_apply_overlay_config_change(Some(false), true));
    }

    #[test]
    fn test_overlay_recording_events_not_routed_when_overlay_disabled() {
        let now = chrono::Utc::now();
        let session_id = "session-1".to_string();

        assert_eq!(
            overlay_recording_state_for_event(
                false,
                &RecordingEvent::Started {
                    session_id: session_id.clone(),
                    timestamp: now,
                }
            ),
            None
        );
        assert_eq!(
            overlay_recording_state_for_event(
                false,
                &RecordingEvent::TranscriptionComplete {
                    session_id,
                    text: "hello".to_string(),
                    audio_duration_ms: 500,
                    processing_duration_ms: 100,
                    timestamp: now,
                }
            ),
            None
        );
    }

    #[test]
    fn test_overlay_recording_events_route_when_overlay_enabled() {
        let now = chrono::Utc::now();
        let session_id = "session-1".to_string();

        assert_eq!(
            overlay_recording_state_for_event(
                true,
                &RecordingEvent::Started {
                    session_id: session_id.clone(),
                    timestamp: now,
                }
            ),
            Some(true)
        );
        assert_eq!(
            overlay_recording_state_for_event(
                true,
                &RecordingEvent::TranscriptionFailed {
                    session_id,
                    error: "decoder failed".to_string(),
                    timestamp: now,
                }
            ),
            Some(false)
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
    fn test_sidecar_status_bridge_emits_canonical_event_with_seq() {
        // Regression (qf3m): the event.status_changed bridge emits only
        // canonical sidecar:status with a sequence marker.
        let broadcaster = MockBroadcaster::with_windows(&["main"]);
        let seq_counter = Arc::new(AtomicU64::new(100));

        // Simulate the bridge logic from the event loop
        let canonical_payload = sidecar_status_payload_from_status_event(
            Some("idle"),
            Some("Sidecar ready".to_string()),
            Some(0),
        );
        let seq = next_seq(&seq_counter);
        emit_with_existing_seq_to_all_windows(
            &broadcaster,
            EVENT_SIDECAR_STATUS,
            canonical_payload,
            seq,
        );

        let event_names = broadcaster.received_event_names("main");
        let payloads = broadcaster.received_payloads("main");
        assert_eq!(event_names.len(), 1, "Bridge must emit exactly 1 canonical event");

        assert_eq!(event_names[0], "sidecar:status");
        assert_eq!(
            payloads[0].get("state").and_then(Value::as_str),
            Some("ready")
        );
        let canonical_seq = payloads[0].get("seq").and_then(Value::as_u64);
        assert!(canonical_seq.is_some(), "canonical event must have seq");
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
        // error field is now a plain string (legacy compatible)
        assert_eq!(
            payload.get("error").and_then(Value::as_str),
            Some("Transcription failed")
        );
        assert_eq!(
            payload.get("message").and_then(Value::as_str),
            Some("Transcription failed")
        );
        assert_eq!(
            payload.get("recoverable").and_then(Value::as_bool),
            Some(true)
        );
        // Structured error is in app_error
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

        // error field is now a plain string (legacy compatible).
        assert_eq!(
            payload.get("error").and_then(Value::as_str),
            Some("Sidecar crashed")
        );

        // Structured error details available via app_error.
        assert_eq!(
            payload.pointer("/app_error/code").and_then(Value::as_str),
            Some("E_SIDECAR_CRASH")
        );
        assert_eq!(
            payload
                .pointer("/app_error/message")
                .and_then(Value::as_str),
            Some("Sidecar crashed")
        );
        assert_eq!(
            payload
                .pointer("/app_error/recoverable")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            payload
                .pointer("/app_error/details/restart_count")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            payload
                .pointer("/app_error/details/source")
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
    fn test_app_error_event_mic_permission_code_and_shape() {
        let app_error = AppError::new(
            ErrorKind::MicPermission.to_sidecar(),
            "Microphone access denied",
            Some(json!({"reason": "os_denied"})),
            false,
        );

        let payload = app_error_event_payload(&app_error);

        assert_eq!(
            payload.pointer("/error/code").and_then(Value::as_str),
            Some("E_MIC_PERMISSION")
        );
        assert_eq!(
            payload.get("message").and_then(Value::as_str),
            Some("Microphone access denied")
        );
        assert_eq!(
            payload.get("recoverable").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            payload.pointer("/error/details/reason").and_then(Value::as_str),
            Some("os_denied")
        );
    }

    #[test]
    fn test_app_error_event_device_removed_code_and_shape() {
        let app_error = AppError::new(
            ErrorKind::DeviceRemoved.to_sidecar(),
            "Audio device disconnected",
            Some(json!({"device_uid": "usb-mic-1"})),
            true,
        );

        let payload = app_error_event_payload(&app_error);

        assert_eq!(
            payload.pointer("/error/code").and_then(Value::as_str),
            Some("E_DEVICE_REMOVED")
        );
        assert_eq!(
            payload.get("message").and_then(Value::as_str),
            Some("Audio device disconnected")
        );
        assert_eq!(
            payload.get("recoverable").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn test_transcription_error_event_mic_permission_preserves_legacy_fields() {
        let app_error = AppError::new(
            ErrorKind::MicPermission.to_sidecar(),
            "Microphone permission denied by OS",
            None,
            false,
        );

        let payload = transcription_error_event_payload("session-mic-1", &app_error);

        // Legacy flat fields
        assert_eq!(
            payload.get("error").and_then(Value::as_str),
            Some("Microphone permission denied by OS")
        );
        assert_eq!(
            payload.get("message").and_then(Value::as_str),
            Some("Microphone permission denied by OS")
        );
        assert_eq!(
            payload.get("recoverable").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            payload.get("session_id").and_then(Value::as_str),
            Some("session-mic-1")
        );

        // Structured app_error
        assert_eq!(
            payload.pointer("/app_error/code").and_then(Value::as_str),
            Some("E_MIC_PERMISSION")
        );
        assert_eq!(
            payload
                .pointer("/app_error/recoverable")
                .and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn test_clipboard_only_requires_app_error_ignores_app_override_mode() {
        assert!(!clipboard_only_requires_app_error(
            "App override clipboard-only mode (slack)"
        ));
        assert!(clipboard_only_requires_app_error(
            "Focus changed from Terminal to Browser"
        ));
    }

    #[test]
    fn test_injection_method_attempted_classifies_reasons() {
        assert_eq!(
            injection_method_attempted("Focus changed from A to B"),
            "focus_guard"
        );
        assert_eq!(
            injection_method_attempted("Wayland does not support keystroke injection"),
            "keystroke_injection"
        );
        assert_eq!(
            injection_method_attempted("Clipboard error: wl-copy failed"),
            "clipboard"
        );
    }

    #[test]
    fn test_injection_failure_app_error_payload_shape() {
        let app_error = injection_failure_app_error(
            "Focus changed from Terminal to Browser",
            "hello world".len(),
        );

        assert_eq!(app_error.code, ErrorKind::InjectionFailed.to_sidecar());
        assert!(app_error.recoverable);
        assert_eq!(
            app_error
                .details
                .as_ref()
                .and_then(|d| d.get("method_attempted"))
                .and_then(Value::as_str),
            Some("focus_guard")
        );
        assert_eq!(
            app_error
                .details
                .as_ref()
                .and_then(|d| d.get("reason"))
                .and_then(Value::as_str),
            Some("Focus changed from Terminal to Browser")
        );
        assert_eq!(
            app_error
                .details
                .as_ref()
                .and_then(|d| d.get("text_length"))
                .and_then(Value::as_u64),
            Some(11)
        );
    }

    #[test]
    fn test_injection_failed_to_clipboard_only_recovery_emits_app_error() {
        // Simulate the full pipeline: injection fails → clipboard-only fallback → app:error payload.
        let injection_error = "Focus changed from Terminal to Browser";
        let text = "Hello world from dictation";
        let fallback_reason = format!(
            "{}; transcript copied to clipboard for manual paste",
            injection_error
        );

        // Step 1: The injection result transitions to ClipboardOnly.
        let result = crate::injection::InjectionResult::ClipboardOnly {
            reason: fallback_reason.clone(),
            text_length: text.len(),
            timestamp: chrono::Utc::now(),
        };

        // Step 2: clipboard_only_requires_app_error determines if app:error should fire.
        // Focus-change reasons (not app-override) should trigger app:error.
        assert!(clipboard_only_requires_app_error(&fallback_reason));

        // Step 3: Build the injection failure AppError.
        let app_error = injection_failure_app_error(&fallback_reason, text.len());
        assert_eq!(app_error.code, ErrorKind::InjectionFailed.to_sidecar());
        assert!(app_error.recoverable);

        // Step 4: Build the app:error event payload that would be emitted.
        let payload = app_error_event_payload(&app_error);
        assert_eq!(
            payload.pointer("/error/code").and_then(Value::as_str),
            Some("E_INJECTION_FAILED")
        );
        assert_eq!(
            payload.get("message").and_then(Value::as_str),
            Some("Automatic text injection failed. Transcript preserved in history.")
        );
        assert_eq!(
            payload.get("recoverable").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .pointer("/error/details/method_attempted")
                .and_then(Value::as_str),
            Some("focus_guard")
        );
        assert_eq!(
            payload
                .pointer("/error/details/text_length")
                .and_then(Value::as_u64),
            Some(text.len() as u64)
        );

        // Step 5: The history entry reflects ClipboardOnly status.
        let history_result = HistoryInjectionResult::from_injection_result(&result);
        assert!(matches!(
            history_result,
            HistoryInjectionResult::ClipboardOnly { reason } if reason.contains("Focus changed")
        ));
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
    fn test_recording_status_event_payload_idle_can_include_session_id() {
        let payload = recording_status_event_payload("idle", Some("session-3"), None, None);

        assert_eq!(payload.get("phase").and_then(Value::as_str), Some("idle"));
        assert_eq!(
            payload.get("session_id").and_then(Value::as_str),
            Some("session-3")
        );
        assert!(payload.get("started_at").is_none());
        assert!(payload.get("audio_ms").is_none());
    }

    #[test]
    fn test_transcription_failure_app_error_preserves_sidecar_error_kind() {
        let app_error =
            transcription_failure_app_error("session-1", "E_ASR_INIT: model initialization failed");

        // code should reflect the canonical mapping of the sidecar kind, not hardcode E_TRANSCRIPTION_FAILED
        assert_eq!(app_error.code, "E_ASR_INIT");
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
    fn test_transcription_failure_app_error_uses_timeout_code_for_timeout_errors() {
        let app_error =
            transcription_failure_app_error("session-1", "asr_timeout: model timed out after 30s");

        // code should reflect the canonical timeout mapping, not hardcode E_TRANSCRIPTION_FAILED
        assert_eq!(app_error.code, "E_TRANSCRIPTION_TIMEOUT");
        assert_eq!(
            app_error
                .details
                .as_ref()
                .and_then(|d| d.get("error_kind"))
                .and_then(Value::as_str),
            Some("E_TRANSCRIPTION_TIMEOUT")
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

    fn test_device(uid: &str, name: &str) -> SidecarAudioDevice {
        SidecarAudioDevice {
            uid: uid.to_string(),
            name: name.to_string(),
            is_default: false,
            default_sample_rate: 48_000,
            channels: 1,
        }
    }

    #[test]
    fn test_device_hot_swap_decision_during_recording_requests_stop_and_fallback() {
        let previous = vec![
            test_device("mic-1", "USB Mic"),
            test_device("mic-2", "Built-in Mic"),
        ];
        let current = vec![test_device("mic-2", "Built-in Mic")];

        let decision =
            decide_device_hot_swap(AppState::Recording, Some("mic-1"), &previous, &current);

        assert_eq!(decision.removed_device_uid.as_deref(), Some("mic-1"));
        assert_eq!(decision.removed_device_name.as_deref(), Some("USB Mic"));
        assert!(decision.should_stop_recording);
        assert!(!decision.should_force_clipboard_only);
        assert!(decision.should_emit_error);
    }

    #[test]
    fn test_device_hot_swap_decision_mid_transcription_forces_clipboard_preservation() {
        let previous = vec![
            test_device("mic-1", "USB Mic"),
            test_device("mic-2", "Built-in Mic"),
        ];
        let current = vec![test_device("mic-2", "Built-in Mic")];

        let decision =
            decide_device_hot_swap(AppState::Transcribing, Some("mic-1"), &previous, &current);

        assert_eq!(decision.removed_device_uid.as_deref(), Some("mic-1"));
        assert!(!decision.should_stop_recording);
        assert!(decision.should_force_clipboard_only);
        assert!(decision.should_emit_error);
    }

    #[test]
    fn test_device_hot_swap_decision_idle_missing_device_updates_without_error() {
        let previous = vec![
            test_device("mic-1", "USB Mic"),
            test_device("mic-2", "Built-in Mic"),
        ];
        let current = vec![test_device("mic-2", "Built-in Mic")];

        let decision = decide_device_hot_swap(AppState::Idle, Some("mic-1"), &previous, &current);

        assert_eq!(decision.removed_device_uid.as_deref(), Some("mic-1"));
        assert!(!decision.should_stop_recording);
        assert!(!decision.should_force_clipboard_only);
        assert!(!decision.should_emit_error);
    }

    #[test]
    fn test_device_removed_app_error_includes_required_recovery_details() {
        let error = device_removed_app_error(
            "mic-1".to_string(),
            "USB Mic".to_string(),
            Some("mic-default".to_string()),
            true,
            false,
        );

        assert_eq!(error.code, ErrorKind::DeviceRemoved.to_sidecar());
        assert_eq!(
            error.message,
            "Audio device was disconnected. Recording stopped. Using default device."
        );
        assert!(error.recoverable);
        assert_eq!(
            error
                .details
                .as_ref()
                .and_then(|details| details.get("removed_device_uid"))
                .and_then(Value::as_str),
            Some("mic-1")
        );
        assert_eq!(
            error
                .details
                .as_ref()
                .and_then(|details| details.get("had_active_recording"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            error
                .details
                .as_ref()
                .and_then(|details| details.get("transcript_preserved"))
                .and_then(Value::as_bool),
            Some(false)
        );
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
    async fn test_restart_sidecar_reports_spawn_failure_and_clears_runtime_handles() {
        let state_manager = Arc::new(AppStateManager::new());
        let mut manager = IntegrationManager::new(state_manager);
        manager.config.python_path = "__missing_python_binary__".to_string();
        manager.config.sidecar_module = "openvoicy_sidecar".to_string();

        let error = manager
            .restart_sidecar()
            .await
            .expect_err("restart_sidecar should fail when sidecar cannot spawn");
        assert!(error.contains("Failed to spawn"));
        assert!(manager.rpc_client.read().await.is_none());
        assert_eq!(
            manager.supervisor.lock().await.state(),
            SupervisorState::Failed
        );
        assert_eq!(
            manager.watchdog.get_status().await,
            crate::watchdog::HealthStatus::NotRunning
        );
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

    #[test]
    fn test_recording_start_params_include_vad_fields() {
        let mut app_config = config::AppConfig::default();
        app_config.audio.device_uid = Some("mic-1".to_string());
        app_config.audio.trim_silence = false;
        app_config.audio.vad_enabled = true;
        app_config.audio.vad_silence_ms = 1500;
        app_config.audio.vad_min_speech_ms = 350;

        let params = recording_start_params("session-1", &app_config);

        assert_eq!(params["session_id"], "session-1");
        assert_eq!(params["device_uid"], "mic-1");
        assert_eq!(params["trim_silence"], false);
        assert_eq!(params["vad_enabled"], true);
        assert_eq!(params["vad_silence_ms"], 1500);
        assert_eq!(params["vad_min_speech_ms"], 350);
    }

    #[tokio::test]
    async fn test_start_recording_requires_sidecar_connection_without_state_transition() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(Arc::clone(&state_manager));
        manager.recording_controller.set_model_ready(true).await;

        let error = manager
            .start_recording()
            .await
            .expect_err("start_recording should fail without sidecar");
        assert!(error.contains("Sidecar not connected"));
        assert_eq!(state_manager.get(), AppState::Idle);
        assert!(manager.current_session_id.read().await.is_none());
        assert!(manager
            .recording_controller
            .current_session_id()
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_stop_recording_requires_active_session() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(Arc::clone(&state_manager));

        let error = manager
            .stop_recording()
            .await
            .expect_err("stop_recording should fail without an active session");
        assert!(error.contains("No recording in progress"));
        assert_eq!(state_manager.get(), AppState::Idle);
        assert!(manager.current_session_id.read().await.is_none());
    }

    #[tokio::test]
    async fn test_cancel_recording_requires_active_session() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(Arc::clone(&state_manager));

        let error = manager
            .cancel_recording()
            .await
            .expect_err("cancel_recording should fail without an active session");
        assert!(error.contains("No recording in progress"));
        assert_eq!(state_manager.get(), AppState::Idle);
        assert!(manager.current_session_id.read().await.is_none());
    }

    #[tokio::test]
    async fn test_cancel_recording_without_sidecar_is_best_effort_and_clears_state() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(Arc::clone(&state_manager));
        manager.recording_controller.set_model_ready(true).await;

        let session_id = "session-cancel-1".to_string();
        manager
            .recording_controller
            .start_with_session_id(session_id.clone())
            .await
            .expect("host recording should start for cancel flow test");

        *manager.current_session_id.write().await = Some(session_id.clone());
        *manager.recording_context.write().await = Some(RecordingContext {
            focus_before: capture_focus(),
            session_id,
            audio_duration_ms: None,
            raw_text: None,
            final_text: None,
            language: None,
            confidence: None,
            force_clipboard_only: false,
            force_clipboard_reason: None,
            timing_marks: PipelineTimingMarks::default(),
        });

        manager
            .cancel_recording()
            .await
            .expect("cancel_recording should succeed even without sidecar connection");

        assert_eq!(state_manager.get(), AppState::Idle);
        assert!(manager.current_session_id.read().await.is_none());
        assert!(manager.recording_context.read().await.is_none());
        assert!(manager
            .recording_controller
            .current_session_id()
            .await
            .is_none());
    }

    #[tokio::test]
    async fn test_no_audio_device_cleanup_clears_active_recording_session() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(Arc::clone(&state_manager));
        manager.recording_controller.set_model_ready(true).await;

        let session_id = "session-no-device-recording".to_string();
        manager
            .recording_controller
            .start_with_session_id(session_id.clone())
            .await
            .expect("recording controller should accept test session");
        *manager.current_session_id.write().await = Some(session_id.clone());
        *manager.recording_context.write().await = Some(RecordingContext {
            focus_before: capture_focus(),
            session_id,
            audio_duration_ms: None,
            raw_text: None,
            final_text: None,
            language: None,
            confidence: None,
            force_clipboard_only: false,
            force_clipboard_reason: None,
            timing_marks: PipelineTimingMarks::default(),
        });
        let _ = state_manager.transition(AppState::Recording);

        cleanup_active_session_for_no_audio_devices(
            &manager,
            &state_manager,
            &manager.recording_context,
            &manager.current_session_id,
        )
        .await;

        assert!(manager.current_session_id.read().await.is_none());
        assert!(manager.recording_context.read().await.is_none());
        assert_eq!(state_manager.get(), AppState::Idle);
    }

    #[tokio::test]
    async fn test_no_audio_device_cleanup_clears_active_transcribing_session() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(Arc::clone(&state_manager));
        manager.recording_controller.set_model_ready(true).await;

        let session_id = "session-no-device-transcribing".to_string();
        manager
            .recording_controller
            .start_with_session_id(session_id.clone())
            .await
            .expect("recording controller should accept test session");
        *manager.current_session_id.write().await = Some(session_id.clone());
        *manager.recording_context.write().await = Some(RecordingContext {
            focus_before: capture_focus(),
            session_id,
            audio_duration_ms: Some(1200),
            raw_text: Some("partial".to_string()),
            final_text: Some("partial".to_string()),
            language: None,
            confidence: None,
            force_clipboard_only: false,
            force_clipboard_reason: None,
            timing_marks: PipelineTimingMarks::default(),
        });
        let _ = state_manager.transition(AppState::Transcribing);

        cleanup_active_session_for_no_audio_devices(
            &manager,
            &state_manager,
            &manager.recording_context,
            &manager.current_session_id,
        )
        .await;

        assert!(manager.current_session_id.read().await.is_none());
        assert!(manager.recording_context.read().await.is_none());
        assert_eq!(state_manager.get(), AppState::Idle);
    }

    #[tokio::test]
    async fn test_full_recording_flow_with_mock_sidecar_and_cancel_branch() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir should be created");
        let _audio_cue_guard =
            ScopedAudioCueManagerOverride::install_with_missing_sounds(temp_dir.path());
        let call_log_path = temp_dir.path().join("mock_sidecar_calls.jsonl");
        fs::write(&call_log_path, "").expect("call log file should be initialized");

        let mut mock_sidecar =
            ChildProcessGuard::new(spawn_mock_sidecar_recording_process(&call_log_path));
        let stdin = mock_sidecar
            .child_mut()
            .stdin
            .take()
            .expect("mock sidecar stdin should be piped");
        let stdout = mock_sidecar
            .child_mut()
            .stdout
            .take()
            .expect("mock sidecar stdout should be piped");

        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(Arc::clone(&state_manager));

        let rpc_client = RpcClient::new(stdin, stdout);
        manager.start_notification_loop(rpc_client.subscribe());
        manager.start_recording_event_loop();
        *manager.rpc_client.write().await = Some(rpc_client);
        manager.recording_controller.set_model_ready(true).await;
        let mut recording_config = manager.recording_controller.get_config().await;
        recording_config.too_short_threshold = Duration::from_millis(0);
        manager
            .recording_controller
            .set_config(recording_config)
            .await;
        let mut recording_events = manager.recording_controller.subscribe();
        let mut state_events = state_manager.subscribe();

        println!(
            "[RECORDING_FLOW][STATE] {} start_recording invoked",
            chrono::Utc::now().to_rfc3339()
        );
        manager
            .start_recording()
            .await
            .expect("start_recording should succeed");
        let first_session_id = manager
            .current_session_id
            .read()
            .await
            .clone()
            .expect("session id should be present after start");
        assert!(
            Uuid::parse_str(first_session_id.as_str()).is_ok(),
            "session id should be UUID v4-like"
        );
        assert_eq!(state_manager.get(), AppState::Recording);
        println!(
            "[RECORDING_FLOW][STATE] {} transitioned to {:?}",
            chrono::Utc::now().to_rfc3339(),
            state_manager.get()
        );
        wait_for_state_event(&mut state_events, Duration::from_secs(2), |event| {
            event.state == AppState::Recording
        })
        .await;

        let started = wait_for_recording_event(&mut recording_events, Duration::from_secs(2), |event| {
            matches!(
                event,
                RecordingEvent::Started { session_id, .. } if session_id == &first_session_id
            )
        })
        .await;
        assert!(matches!(started, RecordingEvent::Started { .. }));

        wait_until(Duration::from_secs(2), || {
            read_mock_call_log(&call_log_path)
                .iter()
                .any(|call| call.get("method").and_then(Value::as_str) == Some("recording.start"))
        })
        .await;

        let first_calls = read_mock_call_log(&call_log_path);
        let start_call = first_calls
            .iter()
            .find(|call| call.get("method").and_then(Value::as_str) == Some("recording.start"))
            .expect("recording.start should be called");
        let start_params = start_call
            .get("params")
            .and_then(Value::as_object)
            .expect("recording.start params should be object");
        assert_eq!(
            start_params.get("session_id").and_then(Value::as_str),
            Some(first_session_id.as_str())
        );
        assert!(
            start_params.contains_key("device_uid"),
            "recording.start should include device_uid field"
        );
        assert!(
            start_params.contains_key("vad_enabled"),
            "recording.start should include vad fields"
        );
        println!(
            "[RECORDING_FLOW][RPC] {} calls={}",
            chrono::Utc::now().to_rfc3339(),
            serde_json::to_string(&first_calls).unwrap_or_else(|_| "[]".to_string())
        );
        assert!(
            !first_calls.iter().any(|call| {
                matches!(
                    call.get("method").and_then(Value::as_str),
                    Some("replacements.preview")
                        | Some("replacements.get_rules")
                        | Some("replacements.set_rules")
                )
            }),
            "host flow must not re-run replacements for already-final sidecar transcript text"
        );

        println!(
            "[RECORDING_FLOW][STATE] {} stop_recording invoked",
            chrono::Utc::now().to_rfc3339()
        );
        manager
            .stop_recording()
            .await
            .expect("stop_recording should succeed");
        let stopped = wait_for_recording_event(&mut recording_events, Duration::from_secs(2), |event| {
            matches!(
                event,
                RecordingEvent::Stopped { session_id, .. } if session_id == &first_session_id
            )
        })
        .await;
        assert!(matches!(stopped, RecordingEvent::Stopped { .. }));
        wait_for_state_event(&mut state_events, Duration::from_secs(2), |event| {
            event.state == AppState::Transcribing
        })
        .await;

        wait_until(Duration::from_secs(3), || {
            state_manager.get() == AppState::Idle
        })
        .await;
        assert_eq!(
            state_manager.get(),
            AppState::Idle,
            "state should return to idle after transcription completes"
        );
        println!(
            "[RECORDING_FLOW][STATE] {} transitioned to {:?}",
            chrono::Utc::now().to_rfc3339(),
            state_manager.get()
        );
        wait_for_state_event(&mut state_events, Duration::from_secs(2), |event| {
            event.state == AppState::Idle
        })
        .await;

        let completed = wait_for_recording_event(&mut recording_events, Duration::from_secs(2), |event| {
            matches!(
                event,
                RecordingEvent::TranscriptionComplete { session_id, .. } if session_id == &first_session_id
            )
        })
        .await;
        match completed {
            RecordingEvent::TranscriptionComplete { text, .. } => {
                // Keep this flow test host-portable by skipping live injection work.
                assert!(text.is_empty());
            }
            _ => panic!("expected transcription complete event"),
        }

        let stop_call = read_mock_call_log(&call_log_path)
            .into_iter()
            .find(|call| call.get("method").and_then(Value::as_str) == Some("recording.stop"))
            .expect("recording.stop should be called");
        assert_eq!(
            stop_call
                .get("params")
                .and_then(|params| params.get("session_id"))
                .and_then(Value::as_str),
            Some(first_session_id.as_str())
        );
        wait_until(Duration::from_secs(2), || {
            manager
                .current_session_id
                .try_read()
                .map(|guard| guard.is_none())
                .unwrap_or(false)
        })
        .await;
        if manager.current_session_id.read().await.is_some() {
            println!(
                "[RECORDING_FLOW][STATE] {} forcing session cleanup for next branch",
                chrono::Utc::now().to_rfc3339()
            );
            *manager.current_session_id.write().await = None;
            *manager.recording_context.write().await = None;
        }
        println!(
            "[RECORDING_FLOW][STATE] {} start second recording for cancel branch",
            chrono::Utc::now().to_rfc3339()
        );
        manager
            .start_recording()
            .await
            .expect("second start_recording should succeed");
        let cancel_session_id = manager
            .current_session_id
            .read()
            .await
            .clone()
            .expect("session id should exist for cancel flow");
        let started_cancel =
            wait_for_recording_event(&mut recording_events, Duration::from_secs(2), |event| {
                matches!(
                    event,
                    RecordingEvent::Started { session_id, .. } if session_id == &cancel_session_id
                )
            })
            .await;
        assert!(matches!(started_cancel, RecordingEvent::Started { .. }));
        wait_for_state_event(&mut state_events, Duration::from_secs(2), |event| {
            event.state == AppState::Recording
        })
        .await;
        manager
            .cancel_recording()
            .await
            .expect("cancel_recording should succeed");

        wait_until(Duration::from_secs(2), || {
            state_manager.get() == AppState::Idle
        })
        .await;
        assert_eq!(state_manager.get(), AppState::Idle);
        assert!(manager.current_session_id.read().await.is_none());
        wait_for_state_event(&mut state_events, Duration::from_secs(2), |event| {
            event.state == AppState::Idle
        })
        .await;
        let cancelled =
            wait_for_recording_event(&mut recording_events, Duration::from_secs(2), |event| {
                matches!(
                    event,
                    RecordingEvent::Cancelled { session_id, reason, .. }
                        if session_id == &cancel_session_id && reason == &CancelReason::UserButton
                )
            })
            .await;
        assert!(matches!(cancelled, RecordingEvent::Cancelled { .. }));
        assert!(manager.recording_context.read().await.is_none());

        let calls_after_cancel = read_mock_call_log(&call_log_path);
        let cancel_call = calls_after_cancel
            .iter()
            .find(|call| call.get("method").and_then(Value::as_str) == Some("recording.cancel"))
            .expect("recording.cancel should be called");
        assert_eq!(
            cancel_call
                .get("params")
                .and_then(|params| params.get("session_id"))
                .and_then(Value::as_str),
            Some(cancel_session_id.as_str())
        );
        let final_calls = calls_after_cancel;
        println!(
            "[RECORDING_FLOW][RPC] {} calls={}",
            chrono::Utc::now().to_rfc3339(),
            serde_json::to_string(&final_calls).unwrap_or_else(|_| "[]".to_string())
        );
        let stop_calls: Vec<&Value> = final_calls
            .iter()
            .filter(|call| call.get("method").and_then(Value::as_str) == Some("recording.stop"))
            .collect();
        assert_eq!(
            stop_calls.len(),
            1,
            "exactly one recording.stop call is expected for the completed branch"
        );
        assert_eq!(
            stop_calls[0]
                .get("params")
                .and_then(|params| params.get("session_id"))
                .and_then(Value::as_str),
            Some(first_session_id.as_str())
        );
        assert!(
            !final_calls.iter().any(|call| {
                call.get("method").and_then(Value::as_str) == Some("recording.stop")
                    && call
                        .get("params")
                        .and_then(|params| params.get("session_id"))
                        .and_then(Value::as_str)
                        == Some(cancel_session_id.as_str())
            }),
            "cancel flow should not invoke recording.stop for cancelled session"
        );

        let no_transcription_deadline = tokio::time::Instant::now() + Duration::from_millis(300);
        while tokio::time::Instant::now() < no_transcription_deadline {
            let remaining = no_transcription_deadline
                .saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(remaining, recording_events.recv()).await {
                Ok(Ok(RecordingEvent::TranscriptionComplete { session_id, .. }))
                    if session_id == cancel_session_id =>
                {
                    panic!(
                        "cancel flow must not emit transcription_complete for session {}",
                        session_id
                    );
                }
                Ok(Ok(event)) => {
                    println!(
                        "[RECORDING_FLOW][EVENT] {} trailing={}",
                        chrono::Utc::now().to_rfc3339(),
                        serde_json::to_string(&event)
                            .unwrap_or_else(|_| "<serialize-error>".to_string())
                    );
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
                Err(_) => break,
            }
        }

        if let Some(client) = manager.rpc_client.write().await.take() {
            client.shutdown().await;
        }
        mock_sidecar.reap_now();
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
    async fn test_download_model_falls_back_to_legacy_methods_when_model_install_is_unsupported() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir should be created");
        let call_log_path = temp_dir.path().join("mock_model_install_fallback_calls.jsonl");
        fs::write(&call_log_path, "").expect("call log file should be initialized");

        let mut mock_sidecar = spawn_mock_sidecar_model_install_fallback_process(&call_log_path);
        let stdin = mock_sidecar
            .stdin
            .take()
            .expect("mock sidecar stdin should be piped");
        let stdout = mock_sidecar
            .stdout
            .take()
            .expect("mock sidecar stdout should be piped");

        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(Arc::clone(&state_manager));

        let rpc_client = RpcClient::new(stdin, stdout);
        manager.start_notification_loop(rpc_client.subscribe());
        *manager.rpc_client.write().await = Some(rpc_client);

        manager
            .download_model(Some("nvidia/parakeet-tdt-0.6b-v3".to_string()), Some(false))
            .await
            .expect("download_model should succeed via legacy fallback path");

        wait_until(Duration::from_secs(2), || read_mock_call_log(&call_log_path).len() >= 3).await;

        let calls = read_mock_call_log(&call_log_path);
        let methods: Vec<String> = calls
            .iter()
            .filter_map(|call| {
                call.get("method")
                    .and_then(Value::as_str)
                    .map(ToString::to_string)
            })
            .collect();

        assert_eq!(
            methods.get(0),
            Some(&"model.install".to_string()),
            "model.install should be attempted first"
        );
        assert_eq!(
            methods.get(1),
            Some(&"model.download".to_string()),
            "legacy fallback should call model.download when model.install is unavailable"
        );
        assert_eq!(
            methods.get(2),
            Some(&"asr.initialize".to_string()),
            "legacy fallback should initialize ASR after model.download succeeds"
        );

        assert_eq!(manager.get_model_status().await, ModelStatus::Ready);
        assert!(manager.recording_controller.is_model_ready().await);
        assert_eq!(state_manager.get(), AppState::Idle);

        if let Some(client) = manager.rpc_client.write().await.take() {
            client.shutdown().await;
        }
        let _ = mock_sidecar.kill();
        let _ = mock_sidecar.wait();
    }

    #[tokio::test]
    async fn test_asr_initialize_language_fallback_retries_when_language_param_is_rejected() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir should be created");
        let call_log_path = temp_dir.path().join("mock_asr_language_retry_calls.jsonl");
        fs::write(&call_log_path, "").expect("call log file should be initialized");

        let mut mock_sidecar = spawn_mock_sidecar_asr_language_retry_process(
            &call_log_path,
            "E_INVALID_PARAMS",
            "Invalid params: unknown field 'language'",
        );
        let stdin = mock_sidecar
            .stdin
            .take()
            .expect("mock sidecar stdin should be piped");
        let stdout = mock_sidecar
            .stdout
            .take()
            .expect("mock sidecar stdout should be piped");
        let rpc_client = RpcClient::new(stdin, stdout);

        let result = call_asr_initialize_with_language_fallback(
            &rpc_client,
            "openai/whisper-small",
            "cpu",
            Some("en".to_string()),
        )
        .await
        .expect("fallback initialize should succeed");
        assert_eq!(result.status, "ready");

        wait_until(Duration::from_secs(2), || read_mock_call_log(&call_log_path).len() >= 2).await;

        let asr_calls: Vec<Value> = read_mock_call_log(&call_log_path)
            .into_iter()
            .filter(|call| call.get("method").and_then(Value::as_str) == Some("asr.initialize"))
            .collect();

        assert_eq!(asr_calls.len(), 2, "host should retry asr.initialize once");
        assert_eq!(
            asr_calls[0]
                .get("params")
                .and_then(|params| params.get("language"))
                .and_then(Value::as_str),
            Some("en")
        );
        assert!(
            asr_calls[1]
                .get("params")
                .and_then(|params| params.get("language"))
                .is_none(),
            "fallback retry should remove language parameter"
        );

        rpc_client.shutdown().await;
        let _ = mock_sidecar.kill();
        let _ = mock_sidecar.wait();
    }

    #[tokio::test]
    async fn test_asr_initialize_language_fallback_retries_when_method_is_missing() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir should be created");
        let call_log_path = temp_dir.path().join("mock_asr_method_missing_calls.jsonl");
        fs::write(&call_log_path, "").expect("call log file should be initialized");

        let mut mock_sidecar = spawn_mock_sidecar_asr_language_retry_process(
            &call_log_path,
            "E_METHOD_NOT_FOUND",
            "Method not found",
        );
        let stdin = mock_sidecar
            .stdin
            .take()
            .expect("mock sidecar stdin should be piped");
        let stdout = mock_sidecar
            .stdout
            .take()
            .expect("mock sidecar stdout should be piped");
        let rpc_client = RpcClient::new(stdin, stdout);

        let result = call_asr_initialize_with_language_fallback(
            &rpc_client,
            "openai/whisper-small",
            "cpu",
            Some("de".to_string()),
        )
        .await
        .expect("fallback initialize should succeed on method-not-found");
        assert_eq!(result.status, "ready");

        wait_until(Duration::from_secs(2), || read_mock_call_log(&call_log_path).len() >= 2).await;

        let asr_calls: Vec<Value> = read_mock_call_log(&call_log_path)
            .into_iter()
            .filter(|call| call.get("method").and_then(Value::as_str) == Some("asr.initialize"))
            .collect();
        assert_eq!(asr_calls.len(), 2, "host should retry after method-not-found");
        assert_eq!(
            asr_calls[0]
                .get("params")
                .and_then(|params| params.get("language"))
                .and_then(Value::as_str),
            Some("de")
        );
        assert!(
            asr_calls[1]
                .get("params")
                .and_then(|params| params.get("language"))
                .is_none(),
            "retry should omit unsupported language parameter"
        );

        rpc_client.shutdown().await;
        let _ = mock_sidecar.kill();
        let _ = mock_sidecar.wait();
    }

    #[tokio::test]
    async fn test_asr_initialize_language_fallback_surfaces_whisper_unavailable_message() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir should be created");
        let call_log_path = temp_dir.path().join("mock_asr_whisper_unavailable_calls.jsonl");
        fs::write(&call_log_path, "").expect("call log file should be initialized");

        let mut mock_sidecar = spawn_mock_sidecar_asr_initialize_error_process(
            &call_log_path,
            "E_MODEL_LOAD",
            "Language not supported in this build; install faster-whisper",
        );
        let stdin = mock_sidecar
            .stdin
            .take()
            .expect("mock sidecar stdin should be piped");
        let stdout = mock_sidecar
            .stdout
            .take()
            .expect("mock sidecar stdout should be piped");
        let rpc_client = RpcClient::new(stdin, stdout);

        let error = call_asr_initialize_with_language_fallback(
            &rpc_client,
            "openai/whisper-small",
            "cpu",
            Some("en".to_string()),
        )
        .await
        .expect_err("initialize should fail when whisper backend is unavailable");
        assert!(error
            .to_string()
            .contains("Language not supported in this build"));
        assert!(error.to_string().contains("faster-whisper"));

        rpc_client.shutdown().await;
        let _ = mock_sidecar.kill();
        let _ = mock_sidecar.wait();
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
    async fn test_purge_model_cache_rejects_blank_model_id() {
        let state_manager = Arc::new(AppStateManager::new());
        let manager = IntegrationManager::new(state_manager);

        for blank in ["", " ", "  ", "\t", "\n", " \t\n "] {
            let error = manager
                .purge_model_cache(Some(blank.to_string()))
                .await
                .expect_err(&format!(
                    "purge_model_cache should reject blank model_id {:?}",
                    blank
                ));
            assert!(
                error.contains("Invalid model_id"),
                "expected validation error for blank model_id {:?}, got: {}",
                blank,
                error,
            );
        }
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
