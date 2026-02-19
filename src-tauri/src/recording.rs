//! Recording controller with session management.
//!
//! This module manages the recording lifecycle with:
//! - Rust-authoritative session IDs (UUID v4)
//! - Time-bound behaviors (max duration, too-short threshold, timeout)
//! - Double-tap cancel support
//! - Stale notification rejection
//!
//! # Session Management
//!
//! Each recording session gets a unique UUID. This ID is passed to the sidecar
//! and used to correlate transcription results. Stale notifications (from
//! previous sessions) are silently ignored.

#![allow(dead_code)] // Module under construction

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{broadcast, Mutex, RwLock};
use uuid::Uuid;

use crate::state::{AppState, AppStateManager};

/// Default configuration values.
pub mod defaults {
    use std::time::Duration;

    /// Maximum recording duration before auto-stop.
    pub const MAX_RECORDING_DURATION: Duration = Duration::from_secs(60);

    /// Hard limit for max recording duration.
    pub const MAX_RECORDING_HARD_LIMIT: Duration = Duration::from_secs(300);

    /// Minimum recording duration threshold.
    pub const TOO_SHORT_THRESHOLD: Duration = Duration::from_millis(250);

    /// Timeout waiting for transcription result.
    pub const TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(60);

    /// Time window for double-tap cancel detection.
    pub const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(300);
}

/// Recording controller configuration.
#[derive(Debug, Clone)]
pub struct RecordingConfig {
    /// Maximum recording duration before auto-stop.
    pub max_duration: Duration,
    /// Minimum recording duration (below this = no-op).
    pub too_short_threshold: Duration,
    /// Timeout for transcription result.
    pub transcription_timeout: Duration,
    /// Time window for double-tap cancel.
    pub double_tap_window: Duration,
    /// Selected audio device UID (None = default).
    pub device_uid: Option<String>,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            max_duration: defaults::MAX_RECORDING_DURATION,
            too_short_threshold: defaults::TOO_SHORT_THRESHOLD,
            transcription_timeout: defaults::TRANSCRIPTION_TIMEOUT,
            double_tap_window: defaults::DOUBLE_TAP_WINDOW,
            device_uid: None,
        }
    }
}

/// Session identifier (UUID v4).
pub type SessionId = String;

/// Result of stopping a recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum StopResult {
    /// Recording was too short, discarded.
    TooShort,
    /// Recording stopped, now transcribing.
    Transcribing { session_id: SessionId },
}

/// Reason for cancellation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelReason {
    /// User double-tapped hotkey.
    DoubleTap,
    /// User clicked cancel button.
    UserButton,
    /// User pressed escape key.
    EscapeKey,
    /// Max duration auto-stop (not really a cancel, but handled similarly).
    MaxDuration,
}

/// Recording controller errors.
#[derive(Debug, Error)]
pub enum RecordingError {
    #[error("Already recording")]
    AlreadyRecording,

    #[error("Not recording")]
    NotRecording,

    #[error("Model not ready")]
    ModelNotReady,

    #[error("Recording disabled (paused)")]
    Disabled,

    #[error("Invalid state for operation: {0:?}")]
    InvalidState(AppState),

    #[error("Sidecar communication error: {0}")]
    SidecarError(String),

    #[error("State transition error: {0}")]
    StateTransition(String),
}

/// Transcription result from sidecar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    /// Session ID this result belongs to.
    pub session_id: SessionId,
    /// Transcribed text.
    pub text: String,
    /// Audio duration in milliseconds.
    pub audio_duration_ms: u64,
    /// Transcription processing time in milliseconds.
    pub processing_duration_ms: u64,
}

/// Recording session event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum RecordingEvent {
    /// Recording started.
    Started {
        session_id: SessionId,
        timestamp: DateTime<Utc>,
    },
    /// Recording stopped, transcribing.
    Stopped {
        session_id: SessionId,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    /// Recording was too short.
    TooShort {
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    /// Recording cancelled.
    Cancelled {
        session_id: SessionId,
        reason: CancelReason,
        timestamp: DateTime<Utc>,
    },
    /// Transcription completed.
    TranscriptionComplete {
        session_id: SessionId,
        text: String,
        audio_duration_ms: u64,
        processing_duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
    /// Transcription failed.
    TranscriptionFailed {
        session_id: SessionId,
        error: String,
        timestamp: DateTime<Utc>,
    },
    /// Transcription timed out.
    TranscriptionTimeout {
        session_id: SessionId,
        timestamp: DateTime<Utc>,
    },
    /// Max duration auto-stop triggered.
    MaxDurationReached {
        session_id: SessionId,
        duration_ms: u64,
        timestamp: DateTime<Utc>,
    },
}

/// Active recording session.
struct ActiveSession {
    id: SessionId,
    start_time: Instant,
    start_timestamp: DateTime<Utc>,
}

/// Recording controller state.
pub struct RecordingController {
    /// Application state manager.
    state_manager: Arc<AppStateManager>,
    /// Current active session (if recording or transcribing).
    active_session: RwLock<Option<ActiveSession>>,
    /// Last hotkey press time (for double-tap detection).
    last_press_time: RwLock<Option<Instant>>,
    /// Whether model is ready for transcription.
    model_ready: RwLock<bool>,
    /// Configuration.
    config: RwLock<RecordingConfig>,
    /// Event broadcaster.
    event_sender: broadcast::Sender<RecordingEvent>,
    /// Injection mutex (serializes text injection).
    injection_mutex: Mutex<()>,
}

impl RecordingController {
    /// Create a new recording controller.
    pub fn new(state_manager: Arc<AppStateManager>) -> Self {
        let (event_sender, _) = broadcast::channel(32);
        Self {
            state_manager,
            active_session: RwLock::new(None),
            last_press_time: RwLock::new(None),
            model_ready: RwLock::new(false),
            config: RwLock::new(RecordingConfig::default()),
            event_sender,
            injection_mutex: Mutex::new(()),
        }
    }

    /// Update configuration.
    pub async fn set_config(&self, config: RecordingConfig) {
        *self.config.write().await = sanitize_recording_config(config);
    }

    /// Get current configuration.
    pub async fn get_config(&self) -> RecordingConfig {
        self.config.read().await.clone()
    }

    /// Set model ready state.
    pub async fn set_model_ready(&self, ready: bool) {
        *self.model_ready.write().await = ready;
    }

    /// Check if model is ready.
    pub async fn is_model_ready(&self) -> bool {
        *self.model_ready.read().await
    }

    /// Subscribe to recording events.
    pub fn subscribe(&self) -> broadcast::Receiver<RecordingEvent> {
        self.event_sender.subscribe()
    }

    /// Get current session ID (if any).
    pub async fn current_session_id(&self) -> Option<SessionId> {
        self.active_session
            .read()
            .await
            .as_ref()
            .map(|s| s.id.clone())
    }

    /// Start a new recording session.
    ///
    /// Returns the session ID on success.
    pub async fn start(&self) -> Result<SessionId, RecordingError> {
        let session_id = Uuid::new_v4().to_string();
        self.start_with_session_id(session_id).await
    }

    /// Start a new recording session with a caller-provided session ID.
    ///
    /// Returns the same session ID on success.
    pub async fn start_with_session_id(
        &self,
        session_id: SessionId,
    ) -> Result<SessionId, RecordingError> {
        // Check if we can start recording
        self.state_manager
            .can_start_recording()
            .map_err(|reason| match reason {
                crate::state::CannotRecordReason::Paused => RecordingError::Disabled,
                crate::state::CannotRecordReason::ModelLoading => RecordingError::ModelNotReady,
                crate::state::CannotRecordReason::AlreadyRecording => {
                    RecordingError::AlreadyRecording
                }
                crate::state::CannotRecordReason::StillTranscribing => {
                    RecordingError::InvalidState(AppState::Transcribing)
                }
                crate::state::CannotRecordReason::InErrorState => {
                    RecordingError::InvalidState(AppState::Error)
                }
            })?;

        // Check model ready
        if !self.is_model_ready().await {
            return Err(RecordingError::ModelNotReady);
        }

        let now = Instant::now();
        let timestamp = Utc::now();

        // Create session
        {
            let mut session = self.active_session.write().await;
            *session = Some(ActiveSession {
                id: session_id.clone(),
                start_time: now,
                start_timestamp: timestamp,
            });
        }

        // Transition state
        self.state_manager
            .transition(AppState::Recording)
            .map_err(|e| RecordingError::StateTransition(e.to_string()))?;

        // Emit event
        let _ = self.event_sender.send(RecordingEvent::Started {
            session_id: session_id.clone(),
            timestamp,
        });

        log::info!("Recording started: session_id={}", session_id);
        Ok(session_id)
    }

    /// Stop the current recording.
    ///
    /// Returns the stop result (too short or transcribing).
    pub async fn stop(&self) -> Result<StopResult, RecordingError> {
        // Read session info and release lock immediately
        let (session_id, duration) = {
            let session = self.active_session.read().await;
            let session = session.as_ref().ok_or(RecordingError::NotRecording)?;
            (session.id.clone(), session.start_time.elapsed())
        };

        let too_short_threshold = self.config.read().await.too_short_threshold;

        // Check too-short threshold
        if duration < too_short_threshold {
            // Clear session
            *self.active_session.write().await = None;

            // Transition back to idle
            let _ = self.state_manager.transition(AppState::Idle);

            // Emit event
            let _ = self.event_sender.send(RecordingEvent::TooShort {
                duration_ms: duration.as_millis() as u64,
                timestamp: Utc::now(),
            });

            log::info!("Recording too short ({:?}), discarding", duration);
            return Ok(StopResult::TooShort);
        }

        // Transition to transcribing
        self.state_manager
            .transition(AppState::Transcribing)
            .map_err(|e| RecordingError::StateTransition(e.to_string()))?;

        // Emit event
        let _ = self.event_sender.send(RecordingEvent::Stopped {
            session_id: session_id.clone(),
            duration_ms: duration.as_millis() as u64,
            timestamp: Utc::now(),
        });

        log::info!(
            "Recording stopped: session_id={}, duration={:?}",
            session_id,
            duration
        );
        Ok(StopResult::Transcribing { session_id })
    }

    /// Cancel the current recording (discard audio, no transcription).
    pub async fn cancel(&self, reason: CancelReason) -> Result<(), RecordingError> {
        // Get session ID and clear in one operation
        let session_id = {
            let mut session = self.active_session.write().await;
            let session_data = session.as_ref().ok_or(RecordingError::NotRecording)?;
            let id = session_data.id.clone();
            *session = None;
            id
        };

        // Transition to idle
        let _ = self.state_manager.transition(AppState::Idle);

        // Emit event
        let _ = self.event_sender.send(RecordingEvent::Cancelled {
            session_id: session_id.clone(),
            reason: reason.clone(),
            timestamp: Utc::now(),
        });

        log::info!(
            "Recording cancelled: session_id={}, reason={:?}",
            session_id,
            reason
        );
        Ok(())
    }

    /// Handle hotkey press/release for toggle mode.
    ///
    /// Implements double-tap cancel detection.
    pub async fn on_toggle_hotkey(&self) -> Result<HotkeyAction, RecordingError> {
        let now = Instant::now();
        let config = self.config.read().await;

        // Get and update last press time
        let last_press = {
            let mut last = self.last_press_time.write().await;
            let previous = *last;
            *last = Some(now);
            previous
        };

        let state = self.state_manager.get();

        match state {
            AppState::Recording => {
                // Check for double-tap cancel
                if let Some(last) = last_press {
                    let since_last = now.duration_since(last);
                    if since_last < config.double_tap_window {
                        // Double-tap detected: cancel
                        drop(config);
                        self.cancel(CancelReason::DoubleTap).await?;
                        return Ok(HotkeyAction::Cancelled);
                    }
                }
                // Normal stop
                drop(config);
                let result = self.stop().await?;
                match result {
                    StopResult::TooShort => Ok(HotkeyAction::TooShort),
                    StopResult::Transcribing { .. } => Ok(HotkeyAction::Stopped),
                }
            }
            AppState::Idle => {
                drop(config);
                let session_id = self.start().await?;
                Ok(HotkeyAction::Started { session_id })
            }
            _ => {
                // Ignore hotkey in other states
                Ok(HotkeyAction::Ignored)
            }
        }
    }

    /// Handle hotkey press for hold mode (push-to-talk).
    pub async fn on_hold_press(&self) -> Result<HotkeyAction, RecordingError> {
        if self.state_manager.get() != AppState::Idle {
            return Ok(HotkeyAction::Ignored);
        }
        let session_id = self.start().await?;
        Ok(HotkeyAction::Started { session_id })
    }

    /// Handle hotkey release for hold mode (push-to-talk).
    pub async fn on_hold_release(&self) -> Result<HotkeyAction, RecordingError> {
        if self.state_manager.get() != AppState::Recording {
            return Ok(HotkeyAction::Ignored);
        }
        let result = self.stop().await?;
        match result {
            StopResult::TooShort => Ok(HotkeyAction::TooShort),
            StopResult::Transcribing { .. } => Ok(HotkeyAction::Stopped),
        }
    }

    /// Handle max duration auto-stop.
    ///
    /// Returns the same stop result emitted by `stop()` when auto-stop fires.
    pub async fn check_max_duration(&self) -> Option<StopResult> {
        // Check if max duration exceeded
        let (session_id, duration_ms, exceeded) = {
            let session = self.active_session.read().await;
            if let Some(session) = session.as_ref() {
                let config = self.config.read().await;
                let elapsed = session.start_time.elapsed();
                if elapsed >= config.max_duration {
                    (Some(session.id.clone()), elapsed.as_millis() as u64, true)
                } else {
                    (None, 0, false)
                }
            } else {
                (None, 0, false)
            }
        };

        if exceeded {
            if let Some(session_id) = session_id {
                // Emit max duration event
                let _ = self.event_sender.send(RecordingEvent::MaxDurationReached {
                    session_id,
                    duration_ms,
                    timestamp: Utc::now(),
                });
            }

            // Stop recording
            match self.stop().await {
                Ok(result) => return Some(result),
                Err(err) => {
                    log::warn!("Max duration auto-stop failed: {}", err);
                }
            }
        }
        None
    }

    /// Handle transcription result from sidecar.
    ///
    /// Returns true if the result was accepted, false if stale.
    pub async fn on_transcription_result(&self, result: TranscriptionResult) -> bool {
        // Check session ID
        let current = self.current_session_id().await;
        if current.as_ref() != Some(&result.session_id) {
            log::warn!(
                "Ignoring stale transcription: expected {:?}, got {}",
                current,
                result.session_id
            );
            return false;
        }

        // Clear session
        *self.active_session.write().await = None;

        // Transition to idle
        let _ = self.state_manager.transition(AppState::Idle);

        // Emit event
        let _ = self
            .event_sender
            .send(RecordingEvent::TranscriptionComplete {
                session_id: result.session_id,
                text: result.text,
                audio_duration_ms: result.audio_duration_ms,
                processing_duration_ms: result.processing_duration_ms,
                timestamp: Utc::now(),
            });

        true
    }

    /// Handle transcription error from sidecar.
    pub async fn on_transcription_error(&self, session_id: SessionId, error: String) -> bool {
        // Check session ID
        let current = self.current_session_id().await;
        if current.as_ref() != Some(&session_id) {
            log::warn!(
                "Ignoring stale transcription error: expected {:?}, got {}",
                current,
                session_id
            );
            return false;
        }

        // Clear session
        *self.active_session.write().await = None;

        // Transition to error
        self.state_manager.transition_to_error(error.clone());

        // Emit event
        let _ = self.event_sender.send(RecordingEvent::TranscriptionFailed {
            session_id,
            error,
            timestamp: Utc::now(),
        });

        true
    }

    /// Handle transcription timeout.
    pub async fn on_transcription_timeout(&self) {
        let session_id = match self.current_session_id().await {
            Some(id) => id,
            None => return,
        };

        // Check we're actually in transcribing state
        if self.state_manager.get() != AppState::Transcribing {
            return;
        }

        // Clear session
        *self.active_session.write().await = None;

        // Transition to error
        self.state_manager
            .transition_to_error("Transcription timeout - no response from sidecar".to_string());

        // Emit event
        let _ = self
            .event_sender
            .send(RecordingEvent::TranscriptionTimeout {
                session_id,
                timestamp: Utc::now(),
            });

        log::error!("Transcription timeout");
    }

    /// Acquire injection lock for serializing text injection.
    pub async fn acquire_injection_lock(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.injection_mutex.lock().await
    }
}

fn sanitize_recording_config(mut config: RecordingConfig) -> RecordingConfig {
    config.max_duration = config.max_duration.min(defaults::MAX_RECORDING_HARD_LIMIT);
    config
}

/// Result of hotkey action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum HotkeyAction {
    /// Recording started.
    Started { session_id: SessionId },
    /// Recording stopped, transcribing.
    Stopped,
    /// Recording cancelled (double-tap).
    Cancelled,
    /// Recording too short, discarded.
    TooShort,
    /// Hotkey ignored (wrong state).
    Ignored,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Arc<AppStateManager>, RecordingController) {
        let state_manager = Arc::new(AppStateManager::new());
        let controller = RecordingController::new(Arc::clone(&state_manager));
        (state_manager, controller)
    }

    #[tokio::test]
    async fn test_session_id_is_uuid() {
        let (_, controller) = setup();
        controller.set_model_ready(true).await;

        let session_id = controller.start().await.unwrap();

        // Verify it's a valid UUID
        assert!(Uuid::parse_str(&session_id).is_ok());
    }

    #[tokio::test]
    async fn test_start_creates_session() {
        let (state_manager, controller) = setup();
        controller.set_model_ready(true).await;

        let session_id = controller.start().await.unwrap();

        assert_eq!(state_manager.get(), AppState::Recording);
        assert_eq!(controller.current_session_id().await, Some(session_id));
    }

    #[tokio::test]
    async fn test_start_with_session_id_uses_caller_provided_id() {
        let (state_manager, controller) = setup();
        controller.set_model_ready(true).await;

        let fixed = "00000000-0000-4000-8000-000000000123".to_string();
        let returned = controller
            .start_with_session_id(fixed.clone())
            .await
            .expect("start_with_session_id should succeed");

        assert_eq!(returned, fixed);
        assert_eq!(controller.current_session_id().await, Some(returned));
        assert_eq!(state_manager.get(), AppState::Recording);
    }

    #[tokio::test]
    async fn test_double_start_fails() {
        let (_, controller) = setup();
        controller.set_model_ready(true).await;

        controller.start().await.unwrap();
        let result = controller.start().await;

        assert!(matches!(result, Err(RecordingError::AlreadyRecording)));
    }

    #[tokio::test]
    async fn test_stop_without_start_fails() {
        let (_, controller) = setup();

        let result = controller.stop().await;

        assert!(matches!(result, Err(RecordingError::NotRecording)));
    }

    #[tokio::test]
    async fn test_too_short_recording() {
        let (state_manager, controller) = setup();
        controller.set_model_ready(true).await;

        // Set very long threshold for test
        controller
            .set_config(RecordingConfig {
                too_short_threshold: Duration::from_secs(10),
                ..Default::default()
            })
            .await;

        controller.start().await.unwrap();
        let result = controller.stop().await.unwrap();

        assert_eq!(result, StopResult::TooShort);
        assert_eq!(state_manager.get(), AppState::Idle);
        assert!(controller.current_session_id().await.is_none());
    }

    #[tokio::test]
    async fn test_normal_stop_transitions_to_transcribing() {
        let (state_manager, controller) = setup();
        controller.set_model_ready(true).await;

        // Set very short threshold
        controller
            .set_config(RecordingConfig {
                too_short_threshold: Duration::from_millis(0),
                ..Default::default()
            })
            .await;

        let session_id = controller.start().await.unwrap();
        let result = controller.stop().await.unwrap();

        assert!(matches!(result, StopResult::Transcribing { .. }));
        assert_eq!(state_manager.get(), AppState::Transcribing);
        assert_eq!(controller.current_session_id().await, Some(session_id));
    }

    #[tokio::test]
    async fn test_cancel_clears_session() {
        let (state_manager, controller) = setup();
        controller.set_model_ready(true).await;

        controller.start().await.unwrap();
        controller.cancel(CancelReason::UserButton).await.unwrap();

        assert_eq!(state_manager.get(), AppState::Idle);
        assert!(controller.current_session_id().await.is_none());
    }

    #[tokio::test]
    async fn test_stale_transcription_rejected() {
        let (_, controller) = setup();
        controller.set_model_ready(true).await;

        controller.start().await.unwrap();

        // Try to deliver a result with wrong session ID
        let accepted = controller
            .on_transcription_result(TranscriptionResult {
                session_id: "wrong-session-id".to_string(),
                text: "Hello".to_string(),
                audio_duration_ms: 1000,
                processing_duration_ms: 500,
            })
            .await;

        assert!(!accepted);
    }

    #[tokio::test]
    async fn test_correct_transcription_accepted() {
        let (state_manager, controller) = setup();
        controller.set_model_ready(true).await;

        controller
            .set_config(RecordingConfig {
                too_short_threshold: Duration::from_millis(0),
                ..Default::default()
            })
            .await;

        let session_id = controller.start().await.unwrap();
        controller.stop().await.unwrap();

        // Deliver result with correct session ID
        let accepted = controller
            .on_transcription_result(TranscriptionResult {
                session_id: session_id.clone(),
                text: "Hello".to_string(),
                audio_duration_ms: 1000,
                processing_duration_ms: 500,
            })
            .await;

        assert!(accepted);
        assert_eq!(state_manager.get(), AppState::Idle);
        assert!(controller.current_session_id().await.is_none());
    }

    #[tokio::test]
    async fn test_model_not_ready_prevents_start() {
        let (_, controller) = setup();
        // Model not ready (default)

        let result = controller.start().await;

        assert!(matches!(result, Err(RecordingError::ModelNotReady)));
    }

    #[tokio::test]
    async fn test_disabled_prevents_start() {
        let (state_manager, controller) = setup();
        controller.set_model_ready(true).await;
        state_manager.set_enabled(false);

        let result = controller.start().await;

        assert!(matches!(result, Err(RecordingError::Disabled)));
    }

    #[tokio::test]
    async fn test_toggle_hotkey_starts_when_idle() {
        let (_, controller) = setup();
        controller.set_model_ready(true).await;

        let result = controller.on_toggle_hotkey().await.unwrap();

        assert!(matches!(result, HotkeyAction::Started { .. }));
    }

    #[tokio::test]
    async fn test_toggle_hotkey_stops_when_recording() {
        let (_, controller) = setup();
        controller.set_model_ready(true).await;

        controller
            .set_config(RecordingConfig {
                too_short_threshold: Duration::from_millis(0),
                ..Default::default()
            })
            .await;

        // First press: start
        controller.on_toggle_hotkey().await.unwrap();

        // Wait a bit to avoid double-tap detection
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Second press: stop
        let result = controller.on_toggle_hotkey().await.unwrap();

        assert!(matches!(result, HotkeyAction::Stopped));
    }

    #[tokio::test]
    async fn test_double_tap_cancels() {
        let (state_manager, controller) = setup();
        controller.set_model_ready(true).await;

        // First press: start
        controller.on_toggle_hotkey().await.unwrap();
        assert_eq!(state_manager.get(), AppState::Recording);

        // Immediate second press (< 300ms): cancel
        let result = controller.on_toggle_hotkey().await.unwrap();

        assert!(matches!(result, HotkeyAction::Cancelled));
        assert_eq!(state_manager.get(), AppState::Idle);
    }

    #[tokio::test]
    async fn test_transcription_timeout() {
        let (state_manager, controller) = setup();
        controller.set_model_ready(true).await;

        controller
            .set_config(RecordingConfig {
                too_short_threshold: Duration::from_millis(0),
                ..Default::default()
            })
            .await;

        controller.start().await.unwrap();
        controller.stop().await.unwrap();

        assert_eq!(state_manager.get(), AppState::Transcribing);

        // Simulate timeout
        controller.on_transcription_timeout().await;

        assert_eq!(state_manager.get(), AppState::Error);
        assert!(controller.current_session_id().await.is_none());
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let (_, controller) = setup();
        controller.set_model_ready(true).await;

        let mut receiver = controller.subscribe();

        controller.start().await.unwrap();

        // Should receive started event
        let event = receiver.try_recv().unwrap();
        assert!(matches!(event, RecordingEvent::Started { .. }));
    }

    #[tokio::test]
    async fn test_hold_mode_press_and_release() {
        let (state_manager, controller) = setup();
        controller.set_model_ready(true).await;

        controller
            .set_config(RecordingConfig {
                too_short_threshold: Duration::from_millis(0),
                ..Default::default()
            })
            .await;

        // Press
        let result = controller.on_hold_press().await.unwrap();
        assert!(matches!(result, HotkeyAction::Started { .. }));
        assert_eq!(state_manager.get(), AppState::Recording);

        // Release
        let result = controller.on_hold_release().await.unwrap();
        assert!(matches!(result, HotkeyAction::Stopped));
        assert_eq!(state_manager.get(), AppState::Transcribing);
    }

    #[tokio::test]
    async fn test_max_duration_check() {
        let (_, controller) = setup();
        controller.set_model_ready(true).await;

        // Set very short max duration
        controller
            .set_config(RecordingConfig {
                max_duration: Duration::from_millis(1),
                too_short_threshold: Duration::from_millis(0),
                ..Default::default()
            })
            .await;

        controller.start().await.unwrap();

        // Wait for duration to exceed
        tokio::time::sleep(Duration::from_millis(10)).await;

        let auto_stopped_result = controller.check_max_duration().await;
        assert!(matches!(
            auto_stopped_result,
            Some(StopResult::Transcribing { .. })
        ));
    }

    #[tokio::test]
    async fn test_set_config_clamps_max_duration_to_hard_limit() {
        let (_, controller) = setup();
        controller
            .set_config(RecordingConfig {
                max_duration: defaults::MAX_RECORDING_HARD_LIMIT + Duration::from_secs(1),
                ..Default::default()
            })
            .await;

        let config = controller.get_config().await;
        assert_eq!(config.max_duration, defaults::MAX_RECORDING_HARD_LIMIT);
    }

    #[test]
    fn test_sanitize_recording_config_keeps_valid_max_duration() {
        let config = RecordingConfig {
            max_duration: Duration::from_secs(42),
            ..Default::default()
        };

        let sanitized = sanitize_recording_config(config.clone());
        assert_eq!(sanitized.max_duration, config.max_duration);
    }
}
