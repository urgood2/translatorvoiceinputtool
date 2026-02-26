//! Overlay window management with CPU-safe update throttling.
//!
//! This module provides:
//! - Show/hide behavior for the `overlay` window based on recording state.
//! - Positioning helpers with multi-monitor awareness.
//! - Click-through + always-on-top window configuration.
//! - Auto-disable behavior after repeated window-operation failures.
//! - Meter/timer throttling guards to keep overlay CPU usage within budget.

use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use tauri::{PhysicalPosition, Position};
use thiserror::Error;

use crate::config;

pub const OVERLAY_WINDOW_LABEL: &str = "overlay";
pub const OVERLAY_METER_MAX_HZ: u64 = 15;
pub const OVERLAY_TIMER_MAX_HZ: u64 = 2;

const DEFAULT_FAILURE_THRESHOLD: u32 = 3;
const DEFAULT_MARGIN_X: i32 = 24;
const DEFAULT_MARGIN_Y: i32 = 24;

/// Anchor point for overlay placement within a monitor work area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    BottomCenter,
}

impl Default for OverlayAnchor {
    fn default() -> Self {
        Self::BottomCenter
    }
}

/// Overlay placement configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayPositionConfig {
    pub anchor: OverlayAnchor,
    pub margin_x: i32,
    pub margin_y: i32,
}

impl Default for OverlayPositionConfig {
    fn default() -> Self {
        Self {
            anchor: OverlayAnchor::BottomCenter,
            margin_x: DEFAULT_MARGIN_X,
            margin_y: DEFAULT_MARGIN_Y,
        }
    }
}

/// Overlay window dimensions in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayWindowSize {
    pub width: u32,
    pub height: u32,
}

impl Default for OverlayWindowSize {
    fn default() -> Self {
        Self {
            width: 300,
            height: 80,
        }
    }
}

/// Monitor bounds (including work area) in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub work_x: i32,
    pub work_y: i32,
    pub work_width: u32,
    pub work_height: u32,
}

impl MonitorBounds {
    fn from_tauri_monitor(monitor: &tauri::Monitor) -> Self {
        let position = monitor.position();
        let size = monitor.size();
        let work_area = monitor.work_area();
        Self {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
            work_x: work_area.position.x,
            work_y: work_area.position.y,
            work_width: work_area.size.width,
            work_height: work_area.size.height,
        }
    }
}

/// Errors returned by overlay window operations.
#[derive(Debug, Error)]
pub enum OverlayError {
    #[error("overlay window '{0}' not found")]
    MissingWindow(String),
    #[error("no monitor available for overlay positioning")]
    MonitorUnavailable,
    #[error("overlay window operation failed: {0}")]
    Window(String),
    #[error("overlay config operation failed: {0}")]
    Config(String),
}

/// Configuration storage used by the overlay manager.
pub trait OverlayConfigStore {
    fn is_overlay_enabled(&self) -> Result<bool, String>;
    fn set_overlay_enabled(&self, enabled: bool) -> Result<(), String>;
}

/// Default config store backed by `config.rs`.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileOverlayConfigStore;

impl OverlayConfigStore for FileOverlayConfigStore {
    fn is_overlay_enabled(&self) -> Result<bool, String> {
        Ok(config::load_config().ui.overlay_enabled)
    }

    fn set_overlay_enabled(&self, enabled: bool) -> Result<(), String> {
        let mut cfg = config::load_config();
        cfg.ui.overlay_enabled = enabled;
        config::save_config(&cfg).map_err(|error| error.to_string())
    }
}

/// Window operations required by [`OverlayManager`].
pub trait OverlayWindowBackend {
    fn window_exists(&self, label: &str) -> bool;
    fn available_monitors(&self, label: &str) -> Result<Vec<MonitorBounds>, String>;
    fn current_monitor(&self, label: &str) -> Result<Option<MonitorBounds>, String>;
    fn primary_monitor(&self, label: &str) -> Result<Option<MonitorBounds>, String>;
    fn set_always_on_top(&self, label: &str, always_on_top: bool) -> Result<(), String>;
    fn set_click_through(&self, label: &str, click_through: bool) -> Result<(), String>;
    fn set_position(&self, label: &str, x: i32, y: i32) -> Result<(), String>;
    fn show(&self, label: &str) -> Result<(), String>;
    fn hide(&self, label: &str) -> Result<(), String>;
}

/// Tauri backend implementation for [`OverlayWindowBackend`].
pub struct TauriOverlayWindowBackend<'a> {
    app_handle: &'a AppHandle,
}

impl<'a> TauriOverlayWindowBackend<'a> {
    pub fn new(app_handle: &'a AppHandle) -> Self {
        Self { app_handle }
    }

    fn get_overlay_window(&self, label: &str) -> Result<tauri::WebviewWindow, String> {
        self.app_handle
            .get_webview_window(label)
            .ok_or_else(|| format!("overlay window '{label}' not found"))
    }
}

impl OverlayWindowBackend for TauriOverlayWindowBackend<'_> {
    fn window_exists(&self, label: &str) -> bool {
        self.app_handle.get_webview_window(label).is_some()
    }

    fn available_monitors(&self, label: &str) -> Result<Vec<MonitorBounds>, String> {
        let window = self.get_overlay_window(label)?;
        window
            .available_monitors()
            .map(|monitors| {
                monitors
                    .iter()
                    .map(MonitorBounds::from_tauri_monitor)
                    .collect()
            })
            .map_err(|error| error.to_string())
    }

    fn current_monitor(&self, label: &str) -> Result<Option<MonitorBounds>, String> {
        let window = self.get_overlay_window(label)?;
        window
            .current_monitor()
            .map(|monitor| monitor.as_ref().map(MonitorBounds::from_tauri_monitor))
            .map_err(|error| error.to_string())
    }

    fn primary_monitor(&self, label: &str) -> Result<Option<MonitorBounds>, String> {
        let window = self.get_overlay_window(label)?;
        window
            .primary_monitor()
            .map(|monitor| monitor.as_ref().map(MonitorBounds::from_tauri_monitor))
            .map_err(|error| error.to_string())
    }

    fn set_always_on_top(&self, label: &str, always_on_top: bool) -> Result<(), String> {
        let window = self.get_overlay_window(label)?;
        window
            .set_always_on_top(always_on_top)
            .map_err(|error| error.to_string())
    }

    fn set_click_through(&self, label: &str, click_through: bool) -> Result<(), String> {
        let window = self.get_overlay_window(label)?;
        window
            .set_ignore_cursor_events(click_through)
            .map_err(|error| error.to_string())
    }

    fn set_position(&self, label: &str, x: i32, y: i32) -> Result<(), String> {
        let window = self.get_overlay_window(label)?;
        window
            .set_position(Position::Physical(PhysicalPosition { x, y }))
            .map_err(|error| error.to_string())
    }

    fn show(&self, label: &str) -> Result<(), String> {
        let window = self.get_overlay_window(label)?;
        window.show().map_err(|error| error.to_string())
    }

    fn hide(&self, label: &str) -> Result<(), String> {
        let window = self.get_overlay_window(label)?;
        window.hide().map_err(|error| error.to_string())
    }
}

/// CPU-budget guard for overlay meter/timer updates.
#[derive(Debug, Clone)]
pub struct OverlayRateLimiter {
    enabled: bool,
    visible: bool,
    last_meter_emit_at: Option<Instant>,
    last_timer_emit_at: Option<Instant>,
}

impl Default for OverlayRateLimiter {
    fn default() -> Self {
        Self {
            enabled: true,
            visible: false,
            last_meter_emit_at: None,
            last_timer_emit_at: None,
        }
    }
}

impl OverlayRateLimiter {
    fn meter_interval() -> Duration {
        Duration::from_millis(1_000 / OVERLAY_METER_MAX_HZ)
    }

    fn timer_interval() -> Duration {
        Duration::from_millis(1_000 / OVERLAY_TIMER_MAX_HZ)
    }

    fn reset(&mut self) {
        self.last_meter_emit_at = None;
        self.last_timer_emit_at = None;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.visible = false;
            self.reset();
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if !visible {
            self.reset();
        }
    }

    pub fn allow_meter_emit(&mut self, now: Instant) -> bool {
        if !self.enabled || !self.visible {
            return false;
        }

        if self
            .last_meter_emit_at
            .is_some_and(|last| now.duration_since(last) < Self::meter_interval())
        {
            return false;
        }

        self.last_meter_emit_at = Some(now);
        true
    }

    pub fn allow_timer_emit(&mut self, now: Instant, recording_active: bool) -> bool {
        if !recording_active || !self.enabled || !self.visible {
            return false;
        }

        if self
            .last_timer_emit_at
            .is_some_and(|last| now.duration_since(last) < Self::timer_interval())
        {
            return false;
        }

        self.last_timer_emit_at = Some(now);
        true
    }
}

/// Overlay window lifecycle and failure management.
#[derive(Debug, Clone)]
pub struct OverlayManager {
    window_label: String,
    failure_threshold: u32,
    consecutive_failures: u32,
    auto_disabled: bool,
    visible: bool,
    position_config: OverlayPositionConfig,
    window_size: OverlayWindowSize,
    rate_limiter: OverlayRateLimiter,
}

impl Default for OverlayManager {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlayManager {
    pub fn new() -> Self {
        Self {
            window_label: OVERLAY_WINDOW_LABEL.to_string(),
            failure_threshold: DEFAULT_FAILURE_THRESHOLD,
            consecutive_failures: 0,
            auto_disabled: false,
            visible: false,
            position_config: OverlayPositionConfig::default(),
            window_size: OverlayWindowSize::default(),
            rate_limiter: OverlayRateLimiter::default(),
        }
    }

    pub fn with_failure_threshold(mut self, threshold: u32) -> Self {
        self.failure_threshold = threshold.max(1);
        self
    }

    pub fn set_position_config(&mut self, position_config: OverlayPositionConfig) {
        self.position_config = position_config;
    }

    pub fn set_window_size(&mut self, window_size: OverlayWindowSize) {
        self.window_size = window_size;
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    pub fn auto_disabled(&self) -> bool {
        self.auto_disabled
    }

    pub fn allow_meter_emit(&mut self, now: Instant) -> bool {
        self.rate_limiter.allow_meter_emit(now)
    }

    pub fn allow_timer_emit(&mut self, now: Instant, recording_active: bool) -> bool {
        self.rate_limiter.allow_timer_emit(now, recording_active)
    }

    pub fn handle_recording_state<C: OverlayConfigStore, W: OverlayWindowBackend>(
        &mut self,
        recording_active: bool,
        config_store: &C,
        window_backend: &W,
    ) -> Result<(), OverlayError> {
        if recording_active {
            self.show(config_store, window_backend)
        } else {
            self.hide(config_store, window_backend)
        }
    }

    pub fn show<C: OverlayConfigStore, W: OverlayWindowBackend>(
        &mut self,
        config_store: &C,
        window_backend: &W,
    ) -> Result<(), OverlayError> {
        let enabled = config_store
            .is_overlay_enabled()
            .map_err(OverlayError::Config)?;
        self.rate_limiter.set_enabled(enabled);

        if enabled && self.auto_disabled {
            // User manually re-enabled overlay; clear failure guard and retry normally.
            self.clear_failures();
        }

        if !enabled {
            self.mark_hidden();
            return Ok(());
        }

        if !window_backend.window_exists(&self.window_label) {
            let error = OverlayError::MissingWindow(self.window_label.clone());
            return Err(self.register_failure(config_store, error));
        }

        let monitor = match self.resolve_target_monitor(window_backend) {
            Ok(monitor) => monitor,
            Err(error) => return Err(self.register_failure(config_store, error)),
        };

        let (x, y) = compute_overlay_position(&monitor, self.window_size, self.position_config);

        for operation in [
            window_backend
                .set_always_on_top(&self.window_label, true)
                .map_err(OverlayError::Window),
            window_backend
                .set_click_through(&self.window_label, true)
                .map_err(OverlayError::Window),
            window_backend
                .set_position(&self.window_label, x, y)
                .map_err(OverlayError::Window),
            window_backend
                .show(&self.window_label)
                .map_err(OverlayError::Window),
        ] {
            if let Err(error) = operation {
                return Err(self.register_failure(config_store, error));
            }
        }

        self.visible = true;
        self.rate_limiter.set_visible(true);
        self.clear_failures();
        Ok(())
    }

    pub fn hide<C: OverlayConfigStore, W: OverlayWindowBackend>(
        &mut self,
        config_store: &C,
        window_backend: &W,
    ) -> Result<(), OverlayError> {
        let enabled = config_store
            .is_overlay_enabled()
            .map_err(OverlayError::Config)?;
        self.rate_limiter.set_enabled(enabled);

        if !window_backend.window_exists(&self.window_label) {
            self.mark_hidden();
            return Ok(());
        }

        if let Err(error) = window_backend
            .hide(&self.window_label)
            .map_err(OverlayError::Window)
        {
            return Err(self.register_failure(config_store, error));
        }

        self.mark_hidden();
        self.clear_failures();
        Ok(())
    }

    fn resolve_target_monitor<W: OverlayWindowBackend>(
        &self,
        window_backend: &W,
    ) -> Result<MonitorBounds, OverlayError> {
        if let Ok(Some(current)) = window_backend.current_monitor(&self.window_label) {
            return Ok(current);
        }
        if let Ok(Some(primary)) = window_backend.primary_monitor(&self.window_label) {
            return Ok(primary);
        }

        window_backend
            .available_monitors(&self.window_label)
            .map_err(OverlayError::Window)?
            .into_iter()
            .next()
            .ok_or(OverlayError::MonitorUnavailable)
    }

    fn register_failure<C: OverlayConfigStore>(
        &mut self,
        config_store: &C,
        error: OverlayError,
    ) -> OverlayError {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);

        if self.consecutive_failures >= self.failure_threshold {
            self.mark_hidden();
            self.auto_disabled = true;
            self.rate_limiter.set_enabled(false);

            if let Err(config_error) = config_store.set_overlay_enabled(false) {
                return OverlayError::Config(format!(
                    "failed to auto-disable overlay after error '{error}': {config_error}"
                ));
            }
        }

        error
    }

    fn clear_failures(&mut self) {
        self.consecutive_failures = 0;
        self.auto_disabled = false;
    }

    fn mark_hidden(&mut self) {
        self.visible = false;
        self.rate_limiter.set_visible(false);
    }
}

pub fn compute_overlay_position(
    monitor: &MonitorBounds,
    window_size: OverlayWindowSize,
    position_config: OverlayPositionConfig,
) -> (i32, i32) {
    let work_x = monitor.work_x;
    let work_y = monitor.work_y;
    let work_width = monitor.work_width as i32;
    let work_height = monitor.work_height as i32;
    let overlay_width = window_size.width as i32;
    let overlay_height = window_size.height as i32;
    let margin_x = position_config.margin_x.max(0);
    let margin_y = position_config.margin_y.max(0);

    let x = match position_config.anchor {
        OverlayAnchor::TopLeft | OverlayAnchor::BottomLeft => work_x + margin_x,
        OverlayAnchor::TopRight | OverlayAnchor::BottomRight => {
            work_x + work_width - overlay_width - margin_x
        }
        OverlayAnchor::BottomCenter => work_x + ((work_width - overlay_width) / 2),
    };

    let y = match position_config.anchor {
        OverlayAnchor::TopLeft | OverlayAnchor::TopRight => work_y + margin_y,
        OverlayAnchor::BottomLeft | OverlayAnchor::BottomRight | OverlayAnchor::BottomCenter => {
            work_y + work_height - overlay_height - margin_y
        }
    };

    (x.max(work_x), y.max(work_y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct MockConfigStore {
        enabled: Arc<Mutex<bool>>,
        fail_set: bool,
    }

    impl MockConfigStore {
        fn new(enabled: bool) -> Self {
            Self {
                enabled: Arc::new(Mutex::new(enabled)),
                fail_set: false,
            }
        }

        fn set_enabled_direct(&self, enabled: bool) {
            *self.enabled.lock().expect("enabled lock poisoned") = enabled;
        }
    }

    impl OverlayConfigStore for MockConfigStore {
        fn is_overlay_enabled(&self) -> Result<bool, String> {
            Ok(*self.enabled.lock().expect("enabled lock poisoned"))
        }

        fn set_overlay_enabled(&self, enabled: bool) -> Result<(), String> {
            if self.fail_set {
                return Err("set failed".to_string());
            }
            *self.enabled.lock().expect("enabled lock poisoned") = enabled;
            Ok(())
        }
    }

    #[derive(Clone)]
    struct MockWindowBackend {
        exists: bool,
        current_monitor: Option<MonitorBounds>,
        primary_monitor: Option<MonitorBounds>,
        monitors: Vec<MonitorBounds>,
        fail_show: bool,
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl MockWindowBackend {
        fn new() -> Self {
            let monitor = MonitorBounds {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                work_x: 0,
                work_y: 0,
                work_width: 1920,
                work_height: 1080,
            };
            Self {
                exists: true,
                current_monitor: Some(monitor),
                primary_monitor: Some(monitor),
                monitors: vec![monitor],
                fail_show: false,
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn call_count(&self, operation: &str) -> usize {
            self.calls
                .lock()
                .expect("calls lock poisoned")
                .iter()
                .filter(|entry| entry.as_str() == operation)
                .count()
        }
    }

    impl OverlayWindowBackend for MockWindowBackend {
        fn window_exists(&self, _label: &str) -> bool {
            self.exists
        }

        fn available_monitors(&self, _label: &str) -> Result<Vec<MonitorBounds>, String> {
            Ok(self.monitors.clone())
        }

        fn current_monitor(&self, _label: &str) -> Result<Option<MonitorBounds>, String> {
            Ok(self.current_monitor)
        }

        fn primary_monitor(&self, _label: &str) -> Result<Option<MonitorBounds>, String> {
            Ok(self.primary_monitor)
        }

        fn set_always_on_top(&self, _label: &str, _always_on_top: bool) -> Result<(), String> {
            self.calls
                .lock()
                .expect("calls lock poisoned")
                .push("set_always_on_top".to_string());
            Ok(())
        }

        fn set_click_through(&self, _label: &str, _click_through: bool) -> Result<(), String> {
            self.calls
                .lock()
                .expect("calls lock poisoned")
                .push("set_click_through".to_string());
            Ok(())
        }

        fn set_position(&self, _label: &str, _x: i32, _y: i32) -> Result<(), String> {
            self.calls
                .lock()
                .expect("calls lock poisoned")
                .push("set_position".to_string());
            Ok(())
        }

        fn show(&self, _label: &str) -> Result<(), String> {
            self.calls
                .lock()
                .expect("calls lock poisoned")
                .push("show".to_string());
            if self.fail_show {
                return Err("show failed".to_string());
            }
            Ok(())
        }

        fn hide(&self, _label: &str) -> Result<(), String> {
            self.calls
                .lock()
                .expect("calls lock poisoned")
                .push("hide".to_string());
            Ok(())
        }
    }

    #[test]
    fn rate_limiter_respects_visibility_and_hz_budgets() {
        let mut limiter = OverlayRateLimiter::default();
        let t0 = Instant::now();

        assert!(!limiter.allow_meter_emit(t0));
        assert!(!limiter.allow_timer_emit(t0, true));

        limiter.set_visible(true);

        assert!(limiter.allow_meter_emit(t0));
        assert!(!limiter.allow_meter_emit(t0 + Duration::from_millis(20)));
        assert!(limiter.allow_meter_emit(t0 + Duration::from_millis(70)));

        assert!(limiter.allow_timer_emit(t0, true));
        assert!(!limiter.allow_timer_emit(t0 + Duration::from_millis(300), true));
        assert!(limiter.allow_timer_emit(t0 + Duration::from_millis(550), true));
        assert!(!limiter.allow_timer_emit(t0 + Duration::from_millis(900), false));

        limiter.set_visible(false);
        assert!(!limiter.allow_meter_emit(t0 + Duration::from_secs(2)));
        assert!(!limiter.allow_timer_emit(t0 + Duration::from_secs(2), true));
    }

    #[test]
    fn compute_overlay_position_bottom_center_uses_work_area() {
        let monitor = MonitorBounds {
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
            work_x: 0,
            work_y: 30,
            work_width: 2560,
            work_height: 1370,
        };

        let (x, y) = compute_overlay_position(
            &monitor,
            OverlayWindowSize {
                width: 300,
                height: 80,
            },
            OverlayPositionConfig::default(),
        );

        assert_eq!(x, 1130);
        assert_eq!(y, 1296);
    }

    #[test]
    fn manager_auto_disables_overlay_after_repeated_failures_and_allows_reenable() {
        let config = MockConfigStore::new(true);
        let mut backend = MockWindowBackend::new();
        backend.fail_show = true;

        let mut manager = OverlayManager::new().with_failure_threshold(2);
        assert!(manager.show(&config, &backend).is_err());
        assert!(manager.show(&config, &backend).is_err());

        assert_eq!(manager.consecutive_failures(), 2);
        assert!(manager.auto_disabled());
        assert!(!config.is_overlay_enabled().expect("config read failed"));
        assert!(!manager.visible());

        config.set_enabled_direct(true);
        backend.fail_show = false;

        assert!(manager.show(&config, &backend).is_ok());
        assert!(manager.visible());
        assert_eq!(manager.consecutive_failures(), 0);
        assert!(!manager.auto_disabled());
    }

    #[test]
    fn manager_skips_window_ops_when_overlay_disabled() {
        let config = MockConfigStore::new(false);
        let backend = MockWindowBackend::new();
        let mut manager = OverlayManager::new();

        assert!(manager.show(&config, &backend).is_ok());
        assert_eq!(backend.call_count("show"), 0);
        assert_eq!(backend.call_count("set_position"), 0);
        assert!(!manager.visible());
    }

    #[test]
    fn manager_handles_recording_state_show_then_hide() {
        let config = MockConfigStore::new(true);
        let backend = MockWindowBackend::new();
        let mut manager = OverlayManager::new();

        assert!(manager
            .handle_recording_state(true, &config, &backend)
            .is_ok());
        assert!(manager.visible());
        assert_eq!(backend.call_count("show"), 1);

        assert!(manager
            .handle_recording_state(false, &config, &backend)
            .is_ok());
        assert!(!manager.visible());
        assert_eq!(backend.call_count("hide"), 1);
    }
}
