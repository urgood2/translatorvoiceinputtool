//! Application state machine implementation.
//!
//! This module provides the central state management for the voice input tool.
//! It ensures thread-safe state transitions, prevents invalid states, and
//! broadcasts state changes to subscribers.
//!
//! # State Diagram
//!
//! ```text
//!                    ┌──────────────────┐
//!                    │   LoadingModel   │
//!                    └────────┬─────────┘
//!                           ▲ │
//!               model init  │ │ model ready
//!                           │ ▼
//!     ┌─────────────────────────────────────────┐
//!     │                  Idle                   │
//!     └────────┬───────────────────────┬────────┘
//!              │ hotkey pressed        │ error
//!              │ (if enabled & ready)  │
//!              ▼                       │
//!     ┌─────────────────┐              │
//!     │    Recording    │──────────────┼───────┐
//!     └────────┬────────┘              │       │
//!              │ stopped               │       │
//!              ▼                       │       │
//!     ┌─────────────────┐              │       │
//!     │  Transcribing   │──────────────┼───────┤
//!     └────────┬────────┘              │       │
//!              │ complete              ▼       │
//!              │                  ┌─────────┐  │
//!              └─────────────────►│  Error  │◄─┘
//!                                 └────┬────┘
//!                                      │ retry
//!                                      ▼
//!                                    Idle
//! ```

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use thiserror::Error;
use tokio::sync::broadcast;

/// Application state values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppState {
    /// Ready to record (default state).
    Idle,
    /// Model is being downloaded or initialized.
    LoadingModel,
    /// Actively recording audio.
    Recording,
    /// Processing transcription.
    Transcribing,
    /// Error state (recoverable via retry).
    Error,
}

impl Default for AppState {
    fn default() -> Self {
        Self::Idle
    }
}

/// Event emitted when state changes.
#[derive(Clone, Debug, Serialize)]
pub struct StateEvent {
    /// Current application state.
    pub state: AppState,
    /// Whether hotkey listening is enabled (false = paused).
    pub enabled: bool,
    /// Error detail (only set when state is Error).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Timestamp of the state change.
    pub timestamp: DateTime<Utc>,
}

/// Error for invalid state transitions.
#[derive(Debug, Error)]
#[error("Invalid state transition from {from:?} to {to:?}")]
pub struct InvalidTransition {
    /// Current state.
    pub from: AppState,
    /// Attempted target state.
    pub to: AppState,
}

/// Reason why recording cannot start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CannotRecordReason {
    /// Hotkey listening is paused.
    Paused,
    /// Model is still loading.
    ModelLoading,
    /// Already recording.
    AlreadyRecording,
    /// Still transcribing previous recording.
    StillTranscribing,
    /// In error state (needs recovery).
    InErrorState,
}

/// Thread-safe application state manager.
pub struct AppStateManager {
    /// Current application state.
    state: RwLock<AppState>,
    /// Whether hotkey listening is enabled.
    enabled: AtomicBool,
    /// Error detail (when in Error state).
    error_detail: RwLock<Option<String>>,
    /// Broadcast sender for state events.
    event_sender: broadcast::Sender<StateEvent>,
}

impl AppStateManager {
    /// Create a new state manager starting in Idle state.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(16);
        Self {
            state: RwLock::new(AppState::Idle),
            enabled: AtomicBool::new(true),
            error_detail: RwLock::new(None),
            event_sender: tx,
        }
    }

    /// Get the current state.
    pub fn get(&self) -> AppState {
        *self.state.read().unwrap()
    }

    /// Check if hotkey listening is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Enable or disable hotkey listening (pause/resume).
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        self.emit_event();
    }

    /// Get the current error detail (if any).
    pub fn get_error_detail(&self) -> Option<String> {
        self.error_detail.read().unwrap().clone()
    }

    /// Attempt a state transition.
    ///
    /// Returns `Ok(())` if the transition is valid, or an error if not.
    pub fn transition(&self, new_state: AppState) -> Result<(), InvalidTransition> {
        let mut state = self.state.write().unwrap();
        let current = *state;

        // Guard recording start while paused: direct transitions must respect enabled state.
        if current == AppState::Idle && new_state == AppState::Recording && !self.is_enabled() {
            return Err(InvalidTransition {
                from: current,
                to: new_state,
            });
        }

        if !Self::is_valid_transition(current, new_state) {
            return Err(InvalidTransition {
                from: current,
                to: new_state,
            });
        }

        *state = new_state;

        // Clear error detail when transitioning away from error
        if new_state != AppState::Error {
            *self.error_detail.write().unwrap() = None;
        }

        drop(state);
        self.emit_event();
        Ok(())
    }

    /// Transition to Error state with a detail message.
    ///
    /// This always succeeds (any state can transition to Error).
    pub fn transition_to_error(&self, detail: String) {
        let mut state = self.state.write().unwrap();
        *state = AppState::Error;
        *self.error_detail.write().unwrap() = Some(detail);
        drop(state);
        self.emit_event();
    }

    /// Check if recording can start.
    pub fn can_start_recording(&self) -> Result<(), CannotRecordReason> {
        if !self.is_enabled() {
            return Err(CannotRecordReason::Paused);
        }

        match self.get() {
            AppState::Idle => Ok(()),
            AppState::LoadingModel => Err(CannotRecordReason::ModelLoading),
            AppState::Recording => Err(CannotRecordReason::AlreadyRecording),
            AppState::Transcribing => Err(CannotRecordReason::StillTranscribing),
            AppState::Error => Err(CannotRecordReason::InErrorState),
        }
    }

    /// Subscribe to state change events.
    pub fn subscribe(&self) -> broadcast::Receiver<StateEvent> {
        self.event_sender.subscribe()
    }

    /// Get a snapshot of the current state event.
    pub fn get_event(&self) -> StateEvent {
        StateEvent {
            state: self.get(),
            enabled: self.is_enabled(),
            detail: self.get_error_detail(),
            timestamp: Utc::now(),
        }
    }

    /// Check if a state transition is valid.
    fn is_valid_transition(from: AppState, to: AppState) -> bool {
        use AppState::*;

        // Any state can stay the same (no-op transition)
        if from == to {
            return true;
        }

        matches!(
            (from, to),
            // From Idle
            (Idle, LoadingModel)
                | (Idle, Recording)
                | (Idle, Error)
                // From LoadingModel
                | (LoadingModel, Idle)
                | (LoadingModel, Error)
                // From Recording
                | (Recording, Transcribing)
                | (Recording, Idle)
                | (Recording, Error)
                // From Transcribing
                | (Transcribing, Idle)
                | (Transcribing, Error)
                // From Error
                | (Error, Idle)
                | (Error, LoadingModel)
        )
    }

    /// Emit a state event to all subscribers.
    fn emit_event(&self) {
        let event = self.get_event();
        // Ignore send errors (no receivers is fine)
        let _ = self.event_sender.send(event);
    }
}

impl Default for AppStateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_initial_state_is_idle() {
        let manager = AppStateManager::new();
        assert_eq!(manager.get(), AppState::Idle);
        assert!(manager.is_enabled());
    }

    #[test]
    fn test_valid_transitions_from_idle() {
        let manager = AppStateManager::new();

        // Idle -> LoadingModel
        assert!(manager.transition(AppState::LoadingModel).is_ok());
        assert_eq!(manager.get(), AppState::LoadingModel);

        // Reset
        manager.transition(AppState::Idle).unwrap();

        // Idle -> Recording
        assert!(manager.transition(AppState::Recording).is_ok());
        assert_eq!(manager.get(), AppState::Recording);

        // Reset
        manager.transition(AppState::Idle).unwrap();

        // Idle -> Error
        assert!(manager.transition(AppState::Error).is_ok());
        assert_eq!(manager.get(), AppState::Error);
    }

    #[test]
    fn test_valid_transitions_from_loading_model() {
        let manager = AppStateManager::new();
        manager.transition(AppState::LoadingModel).unwrap();

        // LoadingModel -> Idle
        assert!(manager.transition(AppState::Idle).is_ok());

        manager.transition(AppState::LoadingModel).unwrap();

        // LoadingModel -> Error
        assert!(manager.transition(AppState::Error).is_ok());
    }

    #[test]
    fn test_valid_transitions_from_recording() {
        let manager = AppStateManager::new();
        manager.transition(AppState::Recording).unwrap();

        // Recording -> Transcribing
        assert!(manager.transition(AppState::Transcribing).is_ok());

        manager.transition(AppState::Idle).unwrap();
        manager.transition(AppState::Recording).unwrap();

        // Recording -> Idle
        assert!(manager.transition(AppState::Idle).is_ok());

        manager.transition(AppState::Recording).unwrap();

        // Recording -> Error
        assert!(manager.transition(AppState::Error).is_ok());
    }

    #[test]
    fn test_valid_transitions_from_transcribing() {
        let manager = AppStateManager::new();
        manager.transition(AppState::Recording).unwrap();
        manager.transition(AppState::Transcribing).unwrap();

        // Transcribing -> Idle
        assert!(manager.transition(AppState::Idle).is_ok());

        manager.transition(AppState::Recording).unwrap();
        manager.transition(AppState::Transcribing).unwrap();

        // Transcribing -> Error
        assert!(manager.transition(AppState::Error).is_ok());
    }

    #[test]
    fn test_valid_transitions_from_error() {
        let manager = AppStateManager::new();
        manager.transition(AppState::Error).unwrap();

        // Error -> Idle
        assert!(manager.transition(AppState::Idle).is_ok());

        manager.transition(AppState::Error).unwrap();

        // Error -> LoadingModel
        assert!(manager.transition(AppState::LoadingModel).is_ok());
    }

    #[test]
    fn test_invalid_transitions() {
        let manager = AppStateManager::new();

        // Idle cannot go directly to Transcribing
        assert!(manager.transition(AppState::Transcribing).is_err());

        // LoadingModel cannot go to Recording
        manager.transition(AppState::LoadingModel).unwrap();
        assert!(manager.transition(AppState::Recording).is_err());

        // Transcribing cannot go back to Recording
        manager.transition(AppState::Idle).unwrap();
        manager.transition(AppState::Recording).unwrap();
        manager.transition(AppState::Transcribing).unwrap();
        assert!(manager.transition(AppState::Recording).is_err());
    }

    #[test]
    fn test_same_state_transition_allowed() {
        let manager = AppStateManager::new();

        // Transitioning to the same state is allowed (no-op)
        assert!(manager.transition(AppState::Idle).is_ok());
        assert_eq!(manager.get(), AppState::Idle);
    }

    #[test]
    fn test_enabled_toggle() {
        let manager = AppStateManager::new();

        assert!(manager.is_enabled());
        manager.set_enabled(false);
        assert!(!manager.is_enabled());
        manager.set_enabled(true);
        assert!(manager.is_enabled());
    }

    #[test]
    fn test_can_start_recording() {
        let manager = AppStateManager::new();

        // Can record when Idle and enabled
        assert!(manager.can_start_recording().is_ok());

        // Cannot record when paused
        manager.set_enabled(false);
        assert_eq!(
            manager.can_start_recording(),
            Err(CannotRecordReason::Paused)
        );
        manager.set_enabled(true);

        // Cannot record when LoadingModel
        manager.transition(AppState::LoadingModel).unwrap();
        assert_eq!(
            manager.can_start_recording(),
            Err(CannotRecordReason::ModelLoading)
        );

        // Cannot record when already Recording
        manager.transition(AppState::Idle).unwrap();
        manager.transition(AppState::Recording).unwrap();
        assert_eq!(
            manager.can_start_recording(),
            Err(CannotRecordReason::AlreadyRecording)
        );

        // Cannot record when Transcribing
        manager.transition(AppState::Transcribing).unwrap();
        assert_eq!(
            manager.can_start_recording(),
            Err(CannotRecordReason::StillTranscribing)
        );

        // Cannot record when in Error
        manager.transition(AppState::Error).unwrap();
        assert_eq!(
            manager.can_start_recording(),
            Err(CannotRecordReason::InErrorState)
        );
    }

    #[test]
    fn test_transition_idle_to_recording_rejected_when_paused() {
        let manager = AppStateManager::new();
        manager.set_enabled(false);

        let err = manager.transition(AppState::Recording).unwrap_err();
        assert_eq!(err.from, AppState::Idle);
        assert_eq!(err.to, AppState::Recording);
        assert_eq!(manager.get(), AppState::Idle);
    }

    #[test]
    fn test_error_detail() {
        let manager = AppStateManager::new();

        assert!(manager.get_error_detail().is_none());

        // Set error with detail
        manager.transition_to_error("Test error message".to_string());
        assert_eq!(manager.get(), AppState::Error);
        assert_eq!(
            manager.get_error_detail(),
            Some("Test error message".to_string())
        );

        // Detail cleared when leaving error state
        manager.transition(AppState::Idle).unwrap();
        assert!(manager.get_error_detail().is_none());
    }

    #[test]
    fn test_event_subscription() {
        let manager = AppStateManager::new();
        let mut receiver = manager.subscribe();

        // Trigger a state change
        manager.transition(AppState::LoadingModel).unwrap();

        // Should receive the event
        let event = receiver.try_recv().unwrap();
        assert_eq!(event.state, AppState::LoadingModel);
        assert!(event.enabled);
    }

    #[test]
    fn test_get_event() {
        let manager = AppStateManager::new();

        let event = manager.get_event();
        assert_eq!(event.state, AppState::Idle);
        assert!(event.enabled);
        assert!(event.detail.is_none());
    }

    #[test]
    fn test_thread_safety() {
        let manager = Arc::new(AppStateManager::new());
        let mut handles = vec![];

        // Spawn multiple threads that read state concurrently
        for _ in 0..10 {
            let m = Arc::clone(&manager);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let _ = m.get();
                    let _ = m.is_enabled();
                }
            }));
        }

        // Spawn a thread that toggles enabled
        let m = Arc::clone(&manager);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                m.set_enabled(i % 2 == 0);
            }
        }));

        // Wait for all threads
        for h in handles {
            h.join().unwrap();
        }

        // Manager should still be in valid state
        let _ = manager.get();
    }

    #[test]
    fn test_cannot_record_reason_wire_snapshot_parity() {
        let snapshot: Value =
            serde_json::from_str(include_str!("../../shared/contracts/tauri_wire.v1.json")).unwrap();
        let expected = snapshot.get("cannot_record_reason").unwrap();

        let actual = serde_json::json!([
            CannotRecordReason::Paused,
            CannotRecordReason::ModelLoading,
            CannotRecordReason::AlreadyRecording,
            CannotRecordReason::StillTranscribing,
            CannotRecordReason::InErrorState
        ]);

        assert_eq!(&actual, expected);
    }
}
