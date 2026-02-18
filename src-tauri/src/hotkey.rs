//! Global hotkey handling with hold/toggle modes and audio feedback.
//!
//! This module provides cross-platform global hotkey support with:
//! - Hold mode: press to start, release to stop
//! - Toggle mode: press to start, press again to stop
//! - Auto-repeat debouncing
//! - Audio cues for start/stop/error
//! - Copy last transcript hotkey

#![allow(dead_code)] // Module under construction

use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::config::{self, HotkeyMode};
use crate::history::TranscriptHistory;
use crate::state::AppStateManager;

/// Sound types for audio cues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Start,
    Stop,
    Error,
}

/// Hotkey event types.
#[derive(Debug, Clone)]
pub enum HotkeyAction {
    /// Primary hotkey pressed (start or toggle recording).
    PrimaryDown,
    /// Primary hotkey released (stop recording in hold mode).
    PrimaryUp,
    /// Copy last transcript hotkey pressed.
    CopyLast,
}

/// Hotkey registration errors.
#[derive(Debug, Error)]
pub enum HotkeyError {
    #[error("Failed to parse hotkey: {0}")]
    ParseError(String),

    #[error("Hotkey already in use: {0}")]
    AlreadyInUse(String),

    #[error("Platform error: {0}")]
    PlatformError(String),

    #[error("Hotkey manager not initialized")]
    NotInitialized,
}

/// Hotkey registration status.
#[derive(Debug, Clone, Serialize)]
pub struct HotkeyStatus {
    /// Primary hotkey string.
    pub primary: String,
    /// Copy last hotkey string.
    pub copy_last: String,
    /// Current mode (hold or toggle).
    pub mode: String,
    /// Whether primary hotkey is registered.
    pub primary_registered: bool,
    /// Whether copy last hotkey is registered.
    pub copy_last_registered: bool,
    /// Registration error if any.
    pub error: Option<String>,
}

/// Internal state for hotkey handling.
struct HotkeyState {
    /// Whether the primary key is currently held down.
    key_is_down: AtomicBool,
    /// Whether recording is active (for toggle mode).
    recording: AtomicBool,
    /// Whether audio cues are enabled.
    audio_cues_enabled: AtomicBool,
    /// Current hotkey mode.
    mode: HotkeyMode,
}

impl HotkeyState {
    fn new(mode: HotkeyMode, audio_cues_enabled: bool) -> Self {
        Self {
            key_is_down: AtomicBool::new(false),
            recording: AtomicBool::new(false),
            audio_cues_enabled: AtomicBool::new(audio_cues_enabled),
            mode,
        }
    }
}

/// Global hotkey manager.
pub struct HotkeyManager {
    /// The global hotkey manager (platform-specific).
    manager: Option<GlobalHotKeyManager>,
    /// Primary hotkey ID.
    primary_id: Option<u32>,
    /// Exact primary hotkey value registered with the OS.
    primary_hotkey: Option<HotKey>,
    /// Copy last hotkey ID.
    copy_last_id: Option<u32>,
    /// Exact copy-last hotkey value registered with the OS.
    copy_last_hotkey: Option<HotKey>,
    /// Internal state.
    state: Arc<HotkeyState>,
    /// Event sender for hotkey actions.
    action_tx: mpsc::Sender<HotkeyAction>,
    /// Event receiver for hotkey actions.
    action_rx: Option<mpsc::Receiver<HotkeyAction>>,
}

impl HotkeyManager {
    /// Create a new hotkey manager.
    pub fn new() -> Self {
        let config = config::load_config();
        let (action_tx, action_rx) = mpsc::channel(32);

        Self {
            manager: None,
            primary_id: None,
            primary_hotkey: None,
            copy_last_id: None,
            copy_last_hotkey: None,
            state: Arc::new(HotkeyState::new(
                config.hotkeys.mode,
                config.audio.audio_cues_enabled,
            )),
            action_tx,
            action_rx: Some(action_rx),
        }
    }

    /// Take the action receiver (can only be called once).
    pub fn take_action_receiver(&mut self) -> Option<mpsc::Receiver<HotkeyAction>> {
        self.action_rx.take()
    }

    /// Initialize and register hotkeys.
    pub fn initialize(&mut self) -> Result<HotkeyStatus, HotkeyError> {
        let manager =
            GlobalHotKeyManager::new().map_err(|e| HotkeyError::PlatformError(e.to_string()))?;

        let config = config::load_config();
        self.primary_id = None;
        self.primary_hotkey = None;
        self.copy_last_id = None;
        self.copy_last_hotkey = None;

        // Parse and register primary hotkey
        let (primary_registered, primary_error) = match parse_hotkey(&config.hotkeys.primary) {
            Ok(hk) => match manager.register(hk) {
                Ok(()) => {
                    self.primary_id = Some(hk.id());
                    self.primary_hotkey = Some(hk);
                    (true, None)
                }
                Err(e) => (false, Some(e.to_string())),
            },
            Err(e) => (false, Some(e.to_string())),
        };

        // Parse and register copy last hotkey
        let (copy_last_registered, copy_last_error) = match parse_hotkey(&config.hotkeys.copy_last)
        {
            Ok(hk) => match manager.register(hk) {
                Ok(()) => {
                    self.copy_last_id = Some(hk.id());
                    self.copy_last_hotkey = Some(hk);
                    (true, None)
                }
                Err(e) => (false, Some(e.to_string())),
            },
            Err(e) => (false, Some(e.to_string())),
        };

        self.manager = Some(manager);

        // Update state
        self.state = Arc::new(HotkeyState::new(
            config.hotkeys.mode,
            config.audio.audio_cues_enabled,
        ));

        let error = primary_error.or(copy_last_error);

        Ok(HotkeyStatus {
            primary: config.hotkeys.primary,
            copy_last: config.hotkeys.copy_last,
            mode: format!("{:?}", config.hotkeys.mode).to_lowercase(),
            primary_registered,
            copy_last_registered,
            error,
        })
    }

    /// Process a hotkey event from the global event channel.
    pub fn process_event(&self, event: GlobalHotKeyEvent) {
        let action = if Some(event.id) == self.primary_id {
            match event.state {
                HotKeyState::Pressed => Some(HotkeyAction::PrimaryDown),
                HotKeyState::Released => Some(HotkeyAction::PrimaryUp),
            }
        } else if Some(event.id) == self.copy_last_id {
            match event.state {
                HotKeyState::Pressed => Some(HotkeyAction::CopyLast),
                HotKeyState::Released => None, // Ignore release for copy last
            }
        } else {
            None
        };

        if let Some(action) = action {
            let _ = self.action_tx.try_send(action);
        }
    }

    /// Handle primary key down event.
    ///
    /// Returns true if recording should start.
    pub fn handle_primary_down(&self, state_manager: &AppStateManager) -> Option<RecordingAction> {
        // Check if enabled
        if !state_manager.is_enabled() {
            return None; // Paused, ignore
        }

        match self.state.mode {
            HotkeyMode::Hold => {
                // Debounce auto-repeat
                if self.state.key_is_down.swap(true, Ordering::SeqCst) {
                    return None; // Already down, this is auto-repeat
                }

                // Check if we can start recording
                if state_manager.can_start_recording().is_err() {
                    play_sound(
                        Sound::Error,
                        self.state.audio_cues_enabled.load(Ordering::Relaxed),
                    );
                    return None;
                }

                play_sound(
                    Sound::Start,
                    self.state.audio_cues_enabled.load(Ordering::Relaxed),
                );
                Some(RecordingAction::Start)
            }
            HotkeyMode::Toggle => {
                // Debounce auto-repeat
                if self.state.key_is_down.swap(true, Ordering::SeqCst) {
                    return None; // Already down, this is auto-repeat
                }

                if self.state.recording.load(Ordering::SeqCst) {
                    // Currently recording, stop
                    self.state.recording.store(false, Ordering::SeqCst);
                    play_sound(
                        Sound::Stop,
                        self.state.audio_cues_enabled.load(Ordering::Relaxed),
                    );
                    Some(RecordingAction::Stop)
                } else {
                    // Not recording, start
                    if state_manager.can_start_recording().is_err() {
                        play_sound(
                            Sound::Error,
                            self.state.audio_cues_enabled.load(Ordering::Relaxed),
                        );
                        return None;
                    }

                    self.state.recording.store(true, Ordering::SeqCst);
                    play_sound(
                        Sound::Start,
                        self.state.audio_cues_enabled.load(Ordering::Relaxed),
                    );
                    Some(RecordingAction::Start)
                }
            }
        }
    }

    /// Handle primary key up event.
    ///
    /// Returns true if recording should stop.
    pub fn handle_primary_up(&self) -> Option<RecordingAction> {
        // Clear the key-down state
        if !self.state.key_is_down.swap(false, Ordering::SeqCst) {
            return None; // Was not down (shouldn't happen)
        }

        match self.state.mode {
            HotkeyMode::Hold => {
                play_sound(
                    Sound::Stop,
                    self.state.audio_cues_enabled.load(Ordering::Relaxed),
                );
                Some(RecordingAction::Stop)
            }
            HotkeyMode::Toggle => {
                // No action on key up for toggle mode
                None
            }
        }
    }

    /// Handle copy last transcript hotkey.
    pub fn handle_copy_last(&self, history: &TranscriptHistory) -> CopyLastResult {
        if let Some(text) = history.copy_last() {
            CopyLastResult::Copied(truncate(&text, 50))
        } else if history.is_empty() {
            CopyLastResult::Empty
        } else {
            CopyLastResult::ClipboardError
        }
    }

    /// Update audio cues setting.
    pub fn set_audio_cues_enabled(&self, enabled: bool) {
        self.state
            .audio_cues_enabled
            .store(enabled, Ordering::Relaxed);
    }

    /// Unregister hotkeys.
    pub fn shutdown(&mut self) {
        let (primary_hotkey, copy_last_hotkey) = self.take_registered_hotkeys();
        if let Some(manager) = &self.manager {
            if let Some(hk) = primary_hotkey {
                let _ = manager.unregister(hk);
            }
            if let Some(hk) = copy_last_hotkey {
                let _ = manager.unregister(hk);
            }
        }
        self.manager = None;
    }

    fn take_registered_hotkeys(&mut self) -> (Option<HotKey>, Option<HotKey>) {
        self.primary_id = None;
        self.copy_last_id = None;
        (self.primary_hotkey.take(), self.copy_last_hotkey.take())
    }
}

impl Default for HotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Recording action to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingAction {
    Start,
    Stop,
}

/// Result of copy last transcript action.
#[derive(Debug, Clone)]
pub enum CopyLastResult {
    /// Copied successfully, contains truncated preview.
    Copied(String),
    /// History is empty.
    Empty,
    /// Clipboard error.
    ClipboardError,
}

/// Parse a hotkey string like "Ctrl+Shift+Space" into a HotKey.
pub fn parse_hotkey(s: &str) -> Result<HotKey, HotkeyError> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return Err(HotkeyError::ParseError("Empty hotkey".to_string()));
    }

    let mut modifiers = Modifiers::empty();
    let mut key_code: Option<Code> = None;

    for part in parts {
        let lower = part.to_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" => modifiers |= Modifiers::ALT,
            "meta" | "super" | "cmd" | "command" | "win" => modifiers |= Modifiers::META,
            _ => {
                // This is the key
                key_code = Some(parse_key_code(part)?);
            }
        }
    }

    let code = key_code.ok_or_else(|| HotkeyError::ParseError("No key specified".to_string()))?;

    Ok(HotKey::new(Some(modifiers), code))
}

/// Parse a key code string into a Code enum.
fn parse_key_code(s: &str) -> Result<Code, HotkeyError> {
    let code = match s.to_lowercase().as_str() {
        "space" => Code::Space,
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" | "digit0" => Code::Digit0,
        "1" | "digit1" => Code::Digit1,
        "2" | "digit2" => Code::Digit2,
        "3" | "digit3" => Code::Digit3,
        "4" | "digit4" => Code::Digit4,
        "5" | "digit5" => Code::Digit5,
        "6" | "digit6" => Code::Digit6,
        "7" | "digit7" => Code::Digit7,
        "8" | "digit8" => Code::Digit8,
        "9" | "digit9" => Code::Digit9,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        "escape" | "esc" => Code::Escape,
        "enter" | "return" => Code::Enter,
        "tab" => Code::Tab,
        "backspace" => Code::Backspace,
        "delete" | "del" => Code::Delete,
        "insert" | "ins" => Code::Insert,
        "home" => Code::Home,
        "end" => Code::End,
        "pageup" | "pgup" => Code::PageUp,
        "pagedown" | "pgdn" | "pgdown" => Code::PageDown,
        "up" | "arrowup" => Code::ArrowUp,
        "down" | "arrowdown" => Code::ArrowDown,
        "left" | "arrowleft" => Code::ArrowLeft,
        "right" | "arrowright" => Code::ArrowRight,
        _ => return Err(HotkeyError::ParseError(format!("Unknown key: {}", s))),
    };
    Ok(code)
}

/// Play an audio cue sound.
fn play_sound(sound: Sound, enabled: bool) {
    if !enabled {
        return;
    }

    // TODO: Implement actual audio playback with rodio
    // For now, just log
    match sound {
        Sound::Start => log::debug!("Audio cue: start"),
        Sound::Stop => log::debug!("Audio cue: stop"),
        Sound::Error => log::debug!("Audio cue: error"),
    }
}

/// Truncate a string to a maximum length with ellipsis.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hotkey_simple() {
        let hk = parse_hotkey("Ctrl+Space").unwrap();
        assert!(hk.mods.contains(Modifiers::CONTROL));
    }

    #[test]
    fn test_parse_hotkey_multiple_modifiers() {
        let hk = parse_hotkey("Ctrl+Shift+Space").unwrap();
        assert!(hk.mods.contains(Modifiers::CONTROL));
        assert!(hk.mods.contains(Modifiers::SHIFT));
    }

    #[test]
    fn test_parse_hotkey_case_insensitive() {
        let hk1 = parse_hotkey("ctrl+shift+space").unwrap();
        let hk2 = parse_hotkey("CTRL+SHIFT+SPACE").unwrap();
        assert_eq!(hk1.mods, hk2.mods);
    }

    #[test]
    fn test_parse_hotkey_letter() {
        let hk = parse_hotkey("Ctrl+Shift+V").unwrap();
        assert!(hk.mods.contains(Modifiers::CONTROL));
        assert!(hk.mods.contains(Modifiers::SHIFT));
    }

    #[test]
    fn test_parse_hotkey_function_key() {
        let hk = parse_hotkey("Alt+F1").unwrap();
        assert!(hk.mods.contains(Modifiers::ALT));
    }

    #[test]
    fn test_parse_hotkey_no_key() {
        let result = parse_hotkey("Ctrl+Shift");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_hotkey_empty() {
        let result = parse_hotkey("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_hotkey_unknown_key() {
        let result = parse_hotkey("Ctrl+FooBar");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_key_code() {
        assert!(parse_key_code("space").is_ok());
        assert!(parse_key_code("a").is_ok());
        assert!(parse_key_code("f12").is_ok());
        assert!(parse_key_code("escape").is_ok());
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
        assert_eq!(truncate("hi", 2), "hi");
    }

    #[test]
    fn test_hotkey_state_creation() {
        let state = HotkeyState::new(HotkeyMode::Hold, true);
        assert!(!state.key_is_down.load(Ordering::Relaxed));
        assert!(!state.recording.load(Ordering::Relaxed));
        assert!(state.audio_cues_enabled.load(Ordering::Relaxed));
    }

    #[test]
    fn test_auto_repeat_debounce() {
        let state = HotkeyState::new(HotkeyMode::Hold, false);

        // First key down
        let was_down = state.key_is_down.swap(true, Ordering::SeqCst);
        assert!(!was_down); // First press

        // Simulated auto-repeat (second key down without release)
        let was_down = state.key_is_down.swap(true, Ordering::SeqCst);
        assert!(was_down); // Already down, this is auto-repeat

        // Key up
        let was_down = state.key_is_down.swap(false, Ordering::SeqCst);
        assert!(was_down); // Was down
    }

    #[test]
    fn test_toggle_mode_state() {
        let state = HotkeyState::new(HotkeyMode::Toggle, false);

        // First press: start recording
        assert!(!state.recording.load(Ordering::SeqCst));
        state.recording.store(true, Ordering::SeqCst);
        assert!(state.recording.load(Ordering::SeqCst));

        // Second press: stop recording
        state.recording.store(false, Ordering::SeqCst);
        assert!(!state.recording.load(Ordering::SeqCst));
    }

    #[test]
    fn test_sound_enum() {
        assert_eq!(Sound::Start, Sound::Start);
        assert_ne!(Sound::Start, Sound::Stop);
    }

    #[test]
    fn test_recording_action_enum() {
        assert_eq!(RecordingAction::Start, RecordingAction::Start);
        assert_ne!(RecordingAction::Start, RecordingAction::Stop);
    }

    #[test]
    fn test_hotkey_status_serialization() {
        let status = HotkeyStatus {
            primary: "Ctrl+Shift+Space".to_string(),
            copy_last: "Ctrl+Shift+V".to_string(),
            mode: "hold".to_string(),
            primary_registered: true,
            copy_last_registered: true,
            error: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("Ctrl+Shift+Space"));
        assert!(json.contains("hold"));
    }

    #[test]
    fn test_shutdown_uses_actual_registered_hotkeys() {
        let mut manager = HotkeyManager::new();
        let custom_primary = parse_hotkey("Ctrl+Shift+X").unwrap();
        let custom_copy_last = parse_hotkey("Alt+F2").unwrap();

        manager.primary_id = Some(custom_primary.id());
        manager.primary_hotkey = Some(custom_primary);
        manager.copy_last_id = Some(custom_copy_last.id());
        manager.copy_last_hotkey = Some(custom_copy_last);

        let (shutdown_primary, shutdown_copy_last) = manager.take_registered_hotkeys();
        let shutdown_primary = shutdown_primary.unwrap();
        let shutdown_copy_last = shutdown_copy_last.unwrap();

        assert_eq!(shutdown_primary.id(), custom_primary.id());
        assert_eq!(shutdown_copy_last.id(), custom_copy_last.id());
        assert!(manager.primary_id.is_none());
        assert!(manager.primary_hotkey.is_none());
        assert!(manager.copy_last_id.is_none());
        assert!(manager.copy_last_hotkey.is_none());
    }

    #[test]
    fn test_process_event_routes_registered_hotkeys_to_actions() {
        let mut manager = HotkeyManager::new();
        let mut rx = manager.take_action_receiver().unwrap();

        manager.primary_id = Some(10);
        manager.copy_last_id = Some(20);

        manager.process_event(GlobalHotKeyEvent {
            id: 10,
            state: HotKeyState::Pressed,
        });
        manager.process_event(GlobalHotKeyEvent {
            id: 10,
            state: HotKeyState::Released,
        });
        manager.process_event(GlobalHotKeyEvent {
            id: 20,
            state: HotKeyState::Pressed,
        });
        manager.process_event(GlobalHotKeyEvent {
            id: 20,
            state: HotKeyState::Released,
        });

        assert!(matches!(rx.try_recv(), Ok(HotkeyAction::PrimaryDown)));
        assert!(matches!(rx.try_recv(), Ok(HotkeyAction::PrimaryUp)));
        assert!(matches!(rx.try_recv(), Ok(HotkeyAction::CopyLast)));
        assert!(rx.try_recv().is_err());
    }
}
