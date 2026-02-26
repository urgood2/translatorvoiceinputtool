//! Sidecar lifecycle supervision and restart policy.
//!
//! `sidecar.rs` owns process spawn/IO primitives.
//! `watchdog.rs` owns health monitoring.
//! `supervisor.rs` owns lifecycle policy (state transitions, restart backoff,
//! circuit breaker, and sidecar status emission).

#![allow(dead_code)] // Module is added ahead of full runtime wiring.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::sidecar::SidecarManager;

const EVENT_SIDECAR_STATUS: &str = "sidecar:status";
const DEFAULT_CAPTURED_LOG_MAX_LINES: usize = 1000;
const STATUS_LOG_EXCERPT_LINES: usize = 8;
const STATUS_LOG_LINE_MAX_CHARS: usize = 180;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SidecarState {
    Starting,
    Ready,
    Failed,
    Restarting,
    Stopped,
}

impl SidecarState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Restarting => "restarting",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarLogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub struct SidecarLogRecord {
    pub stream: SidecarLogStream,
    pub line: String,
    pub captured_at: Instant,
}

#[derive(Debug, Clone)]
pub struct SidecarSupervisorConfig {
    /// Max rapid restarts before circuit breaker opens.
    pub max_restart_count: u32,
    /// Initial delay before restart (milliseconds).
    pub backoff_base_ms: u64,
    /// Exponential multiplier for each subsequent retry.
    pub backoff_factor: f64,
    /// Maximum restart delay cap (milliseconds).
    pub backoff_max_ms: u64,
    /// Rolling window for counting rapid failures (milliseconds).
    pub circuit_breaker_window_ms: u64,
    /// Master switch for auto-restart policy.
    pub auto_restart_enabled: bool,
    /// Maximum sidecar log lines retained in supervisor memory.
    pub captured_log_max_lines: usize,
}

impl Default for SidecarSupervisorConfig {
    fn default() -> Self {
        Self {
            max_restart_count: 5,
            backoff_base_ms: 1000,
            backoff_factor: 2.0,
            backoff_max_ms: 30_000,
            circuit_breaker_window_ms: 60_000,
            auto_restart_enabled: true,
            captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
        }
    }
}

impl SidecarSupervisorConfig {
    fn circuit_breaker_window(&self) -> Duration {
        Duration::from_millis(self.circuit_breaker_window_ms)
    }
}

#[derive(Debug, Clone, Default)]
struct CircuitBreakerState {
    is_open: bool,
    rapid_failure_count: u32,
    last_failure_at: Option<Instant>,
    opened_at: Option<Instant>,
}

pub trait SidecarController: Send + Sync {
    fn start(&self) -> Result<(), String>;
    fn stop(&self) -> Result<(), String>;
    fn self_check(&self) -> Result<String, String>;
    fn drain_captured_logs(&self) -> Vec<String> {
        Vec::new()
    }
}

impl SidecarController for SidecarManager {
    fn start(&self) -> Result<(), String> {
        Self::start(self)
    }

    fn stop(&self) -> Result<(), String> {
        Self::stop(self)
    }

    fn self_check(&self) -> Result<String, String> {
        Self::self_check(self)
    }

    fn drain_captured_logs(&self) -> Vec<String> {
        Self::drain_captured_logs(self)
    }
}

/// Minimum time the sidecar must run in Ready state before backoff resets.
const SUSTAINED_READY_THRESHOLD: Duration = Duration::from_secs(30);

pub struct SidecarSupervisor<C = SidecarManager>
where
    C: SidecarController,
{
    controller: C,
    pub config: SidecarSupervisorConfig,
    state: SidecarState,
    restart_count: u32,
    last_restart_at: Option<Instant>,
    /// When the sidecar last entered Ready state (used for sustained-health backoff reset).
    ready_since: Option<Instant>,
    circuit_breaker: CircuitBreakerState,
    app_handle: Option<AppHandle>,
    captured_logs: VecDeque<SidecarLogRecord>,
}

impl<C> SidecarSupervisor<C>
where
    C: SidecarController,
{
    pub fn new(controller: C, config: SidecarSupervisorConfig) -> Self {
        Self {
            controller,
            config,
            state: SidecarState::Stopped,
            restart_count: 0,
            last_restart_at: None,
            ready_since: None,
            circuit_breaker: CircuitBreakerState::default(),
            app_handle: None,
            captured_logs: VecDeque::new(),
        }
    }

    pub fn with_app_handle(mut self, app_handle: AppHandle) -> Self {
        self.app_handle = Some(app_handle);
        self
    }

    pub fn set_app_handle(&mut self, app_handle: AppHandle) {
        self.app_handle = Some(app_handle);
    }

    pub fn state(&self) -> SidecarState {
        self.state
    }

    pub fn restart_count(&self) -> u32 {
        self.restart_count
    }

    pub fn controller(&self) -> &C {
        &self.controller
    }

    pub fn controller_mut(&mut self) -> &mut C {
        &mut self.controller
    }

    pub fn circuit_breaker_open(&self) -> bool {
        self.circuit_breaker.is_open
    }

    pub fn record_log_line(&mut self, stream: SidecarLogStream, line: impl Into<String>) {
        self.captured_logs.push_back(SidecarLogRecord {
            stream,
            line: line.into(),
            captured_at: Instant::now(),
        });
        let max_lines = self.config.captured_log_max_lines.max(1);
        while self.captured_logs.len() > max_lines {
            self.captured_logs.pop_front();
        }
    }

    pub fn drain_captured_logs(&mut self) -> Vec<SidecarLogRecord> {
        self.capture_controller_logs();
        self.captured_logs.drain(..).collect()
    }

    pub fn recent_captured_log_lines(&mut self, count: usize) -> Vec<String> {
        self.capture_controller_logs();
        if count == 0 {
            return Vec::new();
        }
        let len = self.captured_logs.len();
        let skip = len.saturating_sub(count);
        self.captured_logs
            .iter()
            .skip(skip)
            .map(|entry| entry.line.clone())
            .collect()
    }

    pub async fn start(&mut self) -> Result<(), String> {
        self.capture_controller_logs();
        self.state = SidecarState::Starting;
        self.emit_status(Some("starting sidecar"));

        self.controller.start().map_err(|err| {
            self.state = SidecarState::Failed;
            self.emit_status(Some(&format!("failed to spawn sidecar: {err}")));
            err
        })?;

        self.controller.self_check().map_err(|err| {
            let _ = self.controller.stop();
            self.state = SidecarState::Failed;
            self.emit_status(Some(&format!("sidecar ping failed: {err}")));
            err
        })?;

        // Record when sidecar became ready; backoff resets only after sustained healthy operation.
        self.ready_since = Some(Instant::now());
        self.state = SidecarState::Ready;
        self.emit_status(None);
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), String> {
        self.capture_controller_logs();
        self.controller.stop()?;
        self.state = SidecarState::Stopped;
        self.emit_status(Some("sidecar stopped"));
        Ok(())
    }

    pub async fn restart(&mut self) -> Result<(), String> {
        self.reset_circuit_breaker();
        self.restart_count = 0;
        self.ready_since = None;
        self.state = SidecarState::Restarting;
        self.emit_status(Some("manual sidecar restart requested"));

        let _ = self.controller.stop();
        self.start().await
    }

    pub async fn handle_crash(&mut self) -> Result<(), String> {
        let now = Instant::now();
        self.capture_controller_logs();

        // Reset backoff only if the sidecar ran healthily for a sustained period.
        if let Some(ready_at) = self.ready_since {
            if now.duration_since(ready_at) >= SUSTAINED_READY_THRESHOLD {
                self.restart_count = 0;
                self.last_restart_at = None;
            }
        }
        self.ready_since = None;

        self.register_failure(now);
        let recent_excerpt = self.recent_log_excerpt(STATUS_LOG_EXCERPT_LINES);

        if !self.config.auto_restart_enabled {
            self.state = SidecarState::Failed;
            let message = match recent_excerpt {
                Some(excerpt) => format!(
                    "automatic restart disabled by configuration | recent sidecar logs: {}",
                    excerpt
                ),
                None => "automatic restart disabled by configuration".to_string(),
            };
            self.emit_status(Some(&message));
            return Ok(());
        }

        if self.circuit_breaker.is_open {
            self.state = SidecarState::Failed;
            let message = match recent_excerpt {
                Some(excerpt) => format!(
                    "sidecar failed after rapid restart attempts; circuit breaker tripped (manual restart required) | recent sidecar logs: {}",
                    excerpt
                ),
                None => "sidecar failed after rapid restart attempts; circuit breaker tripped (manual restart required)".to_string(),
            };
            self.emit_status(Some(&message));
            return Ok(());
        }

        self.restart_count = self.restart_count.saturating_add(1);
        self.last_restart_at = Some(now);
        let delay = self.backoff_delay_for_attempt(self.restart_count);

        self.state = SidecarState::Restarting;
        let message = match recent_excerpt {
            Some(excerpt) => format!(
                "sidecar crash detected; restarting in {}ms (attempt {}) | recent sidecar logs: {}",
                delay.as_millis(),
                self.restart_count,
                excerpt
            ),
            None => format!(
                "sidecar crash detected; restarting in {}ms (attempt {})",
                delay.as_millis(),
                self.restart_count
            ),
        };
        self.emit_status(Some(&message));

        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }

        // Stop any lingering process before starting a new one (e.g., hung but not crashed).
        let _ = self.controller.stop();
        self.start().await
    }

    fn reset_circuit_breaker(&mut self) {
        self.circuit_breaker = CircuitBreakerState::default();
    }

    fn register_failure(&mut self, now: Instant) {
        let within_window = self
            .circuit_breaker
            .last_failure_at
            .map(|previous| now.duration_since(previous) <= self.config.circuit_breaker_window())
            .unwrap_or(false);

        self.circuit_breaker.rapid_failure_count = if within_window {
            self.circuit_breaker.rapid_failure_count.saturating_add(1)
        } else {
            1
        };
        self.circuit_breaker.last_failure_at = Some(now);

        let max_restarts = self.config.max_restart_count.max(1);
        if self.circuit_breaker.rapid_failure_count >= max_restarts {
            self.circuit_breaker.is_open = true;
            self.circuit_breaker.opened_at = Some(now);
        }
    }

    fn backoff_delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt <= 1 {
            return Duration::ZERO;
        }
        if self.config.backoff_base_ms == 0 {
            return Duration::ZERO;
        }

        let factor = if self.config.backoff_factor.is_finite() && self.config.backoff_factor > 0.0 {
            self.config.backoff_factor
        } else {
            1.0
        };
        let exponent = attempt.saturating_sub(2) as i32;
        let scaled = (self.config.backoff_base_ms as f64) * factor.powi(exponent);
        let scaled_ms = scaled.max(0.0).round() as u128;
        let max_ms = u128::from(self.config.backoff_max_ms.max(self.config.backoff_base_ms));
        let delay_ms = scaled_ms.min(max_ms);

        let delay_u64 = if delay_ms > u128::from(u64::MAX) {
            u64::MAX
        } else {
            delay_ms as u64
        };
        Duration::from_millis(delay_u64)
    }

    fn capture_controller_logs(&mut self) {
        for line in self.controller.drain_captured_logs() {
            self.record_log_line(SidecarLogStream::Stderr, line);
        }
    }

    fn recent_log_excerpt(&self, count: usize) -> Option<String> {
        if self.captured_logs.is_empty() || count == 0 {
            return None;
        }

        let len = self.captured_logs.len();
        let skip = len.saturating_sub(count);
        let joined = self
            .captured_logs
            .iter()
            .skip(skip)
            .map(|entry| Self::shorten_log_line(&entry.line))
            .collect::<Vec<_>>()
            .join(" || ");

        if joined.is_empty() {
            None
        } else {
            Some(joined)
        }
    }

    fn shorten_log_line(line: &str) -> String {
        let trimmed = line.trim();
        if trimmed.chars().count() <= STATUS_LOG_LINE_MAX_CHARS {
            return trimmed.to_string();
        }

        let shortened = trimmed
            .chars()
            .take(STATUS_LOG_LINE_MAX_CHARS.saturating_sub(3))
            .collect::<String>();
        format!("{}...", shortened)
    }

    fn emit_status(&self, message: Option<&str>) {
        let payload = self.status_payload(message);
        if let Some(app_handle) = &self.app_handle {
            let _ = app_handle.emit(EVENT_SIDECAR_STATUS, payload);
        }
    }

    fn status_payload(&self, message: Option<&str>) -> Value {
        let mut payload = json!({
            "state": self.state.as_str(),
            "restart_count": self.restart_count,
        });

        if let Some(message) = message.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }) {
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("message".to_string(), json!(message));
            }
        }

        crate::event_seq::payload_with_next_seq(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeControllerState {
        start_calls: u32,
        stop_calls: u32,
        ping_calls: u32,
        fail_start: bool,
        fail_ping: bool,
        captured_logs: Vec<String>,
    }

    #[derive(Clone, Default)]
    struct FakeController {
        inner: Arc<Mutex<FakeControllerState>>,
    }

    impl FakeController {
        fn state(&self) -> FakeControllerState {
            let locked = self.inner.lock().expect("controller state lock poisoned");
            FakeControllerState {
                start_calls: locked.start_calls,
                stop_calls: locked.stop_calls,
                ping_calls: locked.ping_calls,
                fail_start: locked.fail_start,
                fail_ping: locked.fail_ping,
                captured_logs: locked.captured_logs.clone(),
            }
        }

        fn set_fail_start(&self, fail_start: bool) {
            self.inner
                .lock()
                .expect("controller state lock poisoned")
                .fail_start = fail_start;
        }

        fn set_fail_ping(&self, fail_ping: bool) {
            self.inner
                .lock()
                .expect("controller state lock poisoned")
                .fail_ping = fail_ping;
        }

        fn push_captured_log(&self, line: impl Into<String>) {
            self.inner
                .lock()
                .expect("controller state lock poisoned")
                .captured_logs
                .push(line.into());
        }
    }

    impl SidecarController for FakeController {
        fn start(&self) -> Result<(), String> {
            let mut state = self.inner.lock().expect("controller state lock poisoned");
            state.start_calls = state.start_calls.saturating_add(1);
            if state.fail_start {
                return Err("spawn failed".to_string());
            }
            Ok(())
        }

        fn stop(&self) -> Result<(), String> {
            let mut state = self.inner.lock().expect("controller state lock poisoned");
            state.stop_calls = state.stop_calls.saturating_add(1);
            Ok(())
        }

        fn self_check(&self) -> Result<String, String> {
            let mut state = self.inner.lock().expect("controller state lock poisoned");
            state.ping_calls = state.ping_calls.saturating_add(1);
            if state.fail_ping {
                return Err("ping timeout".to_string());
            }
            Ok("0.1.0".to_string())
        }

        fn drain_captured_logs(&self) -> Vec<String> {
            let mut state = self.inner.lock().expect("controller state lock poisoned");
            std::mem::take(&mut state.captured_logs)
        }
    }

    #[test]
    fn backoff_policy_is_immediate_then_exponential() {
        let controller = FakeController::default();
        let supervisor = SidecarSupervisor::new(
            controller,
            SidecarSupervisorConfig {
                max_restart_count: 10,
                backoff_base_ms: 100,
                backoff_factor: 2.0,
                backoff_max_ms: 500,
                circuit_breaker_window_ms: 30_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        assert_eq!(supervisor.backoff_delay_for_attempt(1), Duration::ZERO);
        assert_eq!(
            supervisor.backoff_delay_for_attempt(2),
            Duration::from_millis(100)
        );
        assert_eq!(
            supervisor.backoff_delay_for_attempt(3),
            Duration::from_millis(200)
        );
        assert_eq!(
            supervisor.backoff_delay_for_attempt(4),
            Duration::from_millis(400)
        );
        assert_eq!(
            supervisor.backoff_delay_for_attempt(5),
            Duration::from_millis(500)
        );
    }

    #[tokio::test]
    async fn restart_attempt_progression_is_immediate_then_delayed() {
        println!("[SUPERVISOR_TEST] verifying restart timing progression");
        let controller = FakeController::default();
        controller.set_fail_start(true);
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 10,
                backoff_base_ms: 40,
                backoff_factor: 2.0,
                backoff_max_ms: 200,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        let first_started = Instant::now();
        let first_err = supervisor
            .handle_crash()
            .await
            .expect_err("first crash should bubble spawn failure");
        let first_elapsed = first_started.elapsed();
        println!(
            "[SUPERVISOR_TEST] first crash elapsed={:?} restart_count={}",
            first_elapsed,
            supervisor.restart_count()
        );
        assert!(first_err.contains("spawn failed"));
        assert_eq!(supervisor.restart_count(), 1);
        assert_eq!(supervisor.state(), SidecarState::Failed);

        let second_started = Instant::now();
        let second_err = supervisor
            .handle_crash()
            .await
            .expect_err("second crash should include backoff before failing");
        let second_elapsed = second_started.elapsed();
        println!(
            "[SUPERVISOR_TEST] second crash elapsed={:?} restart_count={}",
            second_elapsed,
            supervisor.restart_count()
        );
        assert!(second_err.contains("spawn failed"));
        assert_eq!(supervisor.restart_count(), 2);
        assert!(
            second_elapsed >= Duration::from_millis(30),
            "expected second restart delay to include configured backoff"
        );
        assert!(
            second_elapsed > first_elapsed,
            "expected second restart attempt to take longer than first"
        );
    }

    #[tokio::test]
    async fn restart_count_increments_on_repeated_failures() {
        println!("[SUPERVISOR_TEST] verifying restart_count increments");
        let controller = FakeController::default();
        controller.set_fail_start(true);
        let mut supervisor = SidecarSupervisor::new(
            controller,
            SidecarSupervisorConfig {
                backoff_base_ms: 0,
                ..SidecarSupervisorConfig::default()
            },
        );

        let _ = supervisor.handle_crash().await;
        assert_eq!(supervisor.restart_count(), 1);
        let _ = supervisor.handle_crash().await;
        assert_eq!(supervisor.restart_count(), 2);
        let _ = supervisor.handle_crash().await;
        assert_eq!(supervisor.restart_count(), 3);
    }

    #[test]
    fn backoff_delay_never_exceeds_max_cap() {
        println!("[SUPERVISOR_TEST] verifying backoff max cap");
        let controller = FakeController::default();
        let supervisor = SidecarSupervisor::new(
            controller,
            SidecarSupervisorConfig {
                max_restart_count: 10,
                backoff_base_ms: 100,
                backoff_factor: 2.0,
                backoff_max_ms: 250,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        for attempt in 2..=8 {
            assert!(
                supervisor.backoff_delay_for_attempt(attempt) <= Duration::from_millis(250),
                "attempt {} exceeded backoff cap",
                attempt
            );
        }
        assert_eq!(
            supervisor.backoff_delay_for_attempt(8),
            Duration::from_millis(250)
        );
    }

    #[tokio::test]
    async fn handle_crash_trips_circuit_breaker_after_rapid_failures() {
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 2,
                backoff_base_ms: 0,
                backoff_factor: 2.0,
                backoff_max_ms: 0,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        supervisor
            .handle_crash()
            .await
            .expect("first crash should auto-restart");
        assert_eq!(supervisor.state(), SidecarState::Ready);
        assert!(!supervisor.circuit_breaker_open());

        supervisor
            .handle_crash()
            .await
            .expect("second rapid crash should trip breaker");
        assert_eq!(supervisor.state(), SidecarState::Failed);
        assert!(supervisor.circuit_breaker_open());

        let state = controller.state();
        assert_eq!(state.start_calls, 1, "only first crash should auto-restart");
    }

    #[tokio::test]
    async fn circuit_breaker_trips_at_exact_configured_threshold() {
        println!("[CIRCUIT_BREAKER_TEST] verifying exact threshold trip behavior");
        let controller = FakeController::default();
        controller.set_fail_start(true);
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 3,
                backoff_base_ms: 0,
                backoff_factor: 2.0,
                backoff_max_ms: 0,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        let _ = supervisor
            .handle_crash()
            .await
            .expect_err("attempt 1 should fail spawn while breaker remains closed");
        assert!(!supervisor.circuit_breaker_open());

        let _ = supervisor
            .handle_crash()
            .await
            .expect_err("attempt 2 should fail spawn while breaker remains closed");
        assert!(!supervisor.circuit_breaker_open());

        supervisor
            .handle_crash()
            .await
            .expect("attempt 3 should trip breaker and stop auto-restart");
        assert!(supervisor.circuit_breaker_open());
        assert_eq!(supervisor.state(), SidecarState::Failed);

        let state = controller.state();
        assert_eq!(
            state.start_calls, 2,
            "breaker should block the threshold attempt from restarting"
        );
    }

    #[test]
    fn circuit_breaker_failure_window_only_counts_rapid_failures() {
        println!("[SUPERVISOR_TEST] verifying circuit breaker failure window");
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(
            controller,
            SidecarSupervisorConfig {
                max_restart_count: 2,
                backoff_base_ms: 0,
                backoff_factor: 2.0,
                backoff_max_ms: 0,
                circuit_breaker_window_ms: 20,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        let t0 = Instant::now();
        supervisor.register_failure(t0);
        assert_eq!(supervisor.circuit_breaker.rapid_failure_count, 1);
        assert!(!supervisor.circuit_breaker.is_open);

        // Outside configured window -> counter resets instead of tripping breaker.
        supervisor.register_failure(t0 + Duration::from_millis(25));
        assert_eq!(supervisor.circuit_breaker.rapid_failure_count, 1);
        assert!(!supervisor.circuit_breaker.is_open);
    }

    #[tokio::test]
    async fn max_restart_count_zero_trips_immediately() {
        println!("[CIRCUIT_BREAKER_TEST] verifying max_restart_count=0 immediate trip");
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 0,
                backoff_base_ms: 0,
                backoff_factor: 2.0,
                backoff_max_ms: 0,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        supervisor
            .handle_crash()
            .await
            .expect("circuit breaker should open and short-circuit restart");
        assert!(supervisor.circuit_breaker_open());
        assert_eq!(supervisor.state(), SidecarState::Failed);
        assert_eq!(
            controller.state().start_calls,
            0,
            "no restart attempt should occur when max_restart_count is zero"
        );
    }

    #[tokio::test]
    async fn tripped_breaker_disables_auto_restart_until_manual_reset() {
        println!("[CIRCUIT_BREAKER_TEST] verifying tripped state blocks auto-restart");
        let controller = FakeController::default();
        controller.set_fail_start(true);
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 1,
                backoff_base_ms: 0,
                backoff_factor: 2.0,
                backoff_max_ms: 0,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        supervisor
            .handle_crash()
            .await
            .expect("first crash should trip breaker without restart");
        assert!(supervisor.circuit_breaker_open());
        assert_eq!(controller.state().start_calls, 0);

        supervisor
            .handle_crash()
            .await
            .expect("subsequent crash should remain blocked by breaker");
        assert!(supervisor.circuit_breaker_open());
        assert_eq!(
            controller.state().start_calls,
            0,
            "auto-restart must remain disabled while breaker is open"
        );
    }

    #[tokio::test]
    async fn manual_restart_resets_circuit_breaker() {
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 1,
                backoff_base_ms: 0,
                backoff_factor: 2.0,
                backoff_max_ms: 0,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        supervisor
            .handle_crash()
            .await
            .expect("breaker opens on first rapid failure");
        assert!(supervisor.circuit_breaker_open());
        assert_eq!(supervisor.state(), SidecarState::Failed);

        supervisor
            .restart()
            .await
            .expect("manual restart should always be allowed");
        assert_eq!(supervisor.state(), SidecarState::Ready);
        assert!(!supervisor.circuit_breaker_open());
        assert_eq!(supervisor.restart_count(), 0);
    }

    #[tokio::test]
    async fn manual_reset_reenables_auto_restart_path() {
        println!("[CIRCUIT_BREAKER_TEST] verifying manual reset re-enables auto-restart");
        let controller = FakeController::default();
        controller.set_fail_start(true);
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 2,
                backoff_base_ms: 0,
                backoff_factor: 2.0,
                backoff_max_ms: 0,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        let _ = supervisor
            .handle_crash()
            .await
            .expect_err("first crash should attempt restart and fail spawn");
        supervisor
            .handle_crash()
            .await
            .expect("second rapid crash should trip breaker");
        assert!(supervisor.circuit_breaker_open());

        controller.set_fail_start(false);
        supervisor
            .restart()
            .await
            .expect("manual restart should reset breaker and reach ready");
        assert!(!supervisor.circuit_breaker_open());
        assert_eq!(supervisor.state(), SidecarState::Ready);

        controller.set_fail_start(true);
        let _ = supervisor
            .handle_crash()
            .await
            .expect_err("auto-restart path should be active again after manual reset");
        assert_eq!(
            controller.state().start_calls,
            3,
            "one failed start before trip + one manual restart + one post-reset auto-restart attempt"
        );
    }

    #[tokio::test]
    async fn start_marks_failed_when_ping_fails() {
        let controller = FakeController::default();
        controller.set_fail_ping(true);

        let mut supervisor =
            SidecarSupervisor::new(controller.clone(), SidecarSupervisorConfig::default());
        let err = supervisor
            .start()
            .await
            .expect_err("ping failure should fail start");
        assert!(err.contains("ping timeout"));
        assert_eq!(supervisor.state(), SidecarState::Failed);

        let state = controller.state();
        assert_eq!(state.start_calls, 1);
        assert_eq!(state.stop_calls, 1);
        assert_eq!(state.ping_calls, 1);
    }

    #[test]
    fn captures_stdout_and_stderr_lines() {
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(controller, SidecarSupervisorConfig::default());

        supervisor.record_log_line(SidecarLogStream::Stdout, "hello");
        supervisor.record_log_line(SidecarLogStream::Stderr, "warn");

        let logs = supervisor.drain_captured_logs();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].stream, SidecarLogStream::Stdout);
        assert_eq!(logs[0].line, "hello");
        assert_eq!(logs[1].stream, SidecarLogStream::Stderr);
        assert_eq!(logs[1].line, "warn");
    }

    #[test]
    fn default_config_matches_supervisor_policy_contract() {
        let config = SidecarSupervisorConfig::default();
        assert_eq!(config.max_restart_count, 5);
        assert_eq!(config.backoff_base_ms, 1000);
        assert_eq!(config.backoff_factor, 2.0);
        assert_eq!(config.backoff_max_ms, 30_000);
        assert_eq!(config.circuit_breaker_window_ms, 60_000);
        assert!(config.auto_restart_enabled);
        assert_eq!(
            config.captured_log_max_lines,
            DEFAULT_CAPTURED_LOG_MAX_LINES
        );
    }

    #[test]
    fn supervisor_ingests_controller_captured_logs() {
        let controller = FakeController::default();
        controller.push_captured_log("line-one");
        controller.push_captured_log("{\"json\":true}");

        let mut supervisor = SidecarSupervisor::new(controller, SidecarSupervisorConfig::default());
        let logs = supervisor.recent_captured_log_lines(10);

        assert_eq!(
            logs,
            vec!["line-one".to_string(), "{\"json\":true}".to_string()]
        );
    }

    #[test]
    fn captured_log_ring_buffer_respects_configured_max() {
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(
            controller,
            SidecarSupervisorConfig {
                captured_log_max_lines: 2,
                ..SidecarSupervisorConfig::default()
            },
        );

        supervisor.record_log_line(SidecarLogStream::Stderr, "one");
        supervisor.record_log_line(SidecarLogStream::Stderr, "two");
        supervisor.record_log_line(SidecarLogStream::Stderr, "three");

        let logs = supervisor.recent_captured_log_lines(10);
        assert_eq!(logs, vec!["two".to_string(), "three".to_string()]);
    }

    #[tokio::test]
    async fn auto_restart_toggle_disables_restart_attempts() {
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                auto_restart_enabled: false,
                backoff_base_ms: 0,
                ..SidecarSupervisorConfig::default()
            },
        );

        supervisor
            .handle_crash()
            .await
            .expect("crash handling should still complete");

        assert_eq!(supervisor.state(), SidecarState::Failed);
        assert_eq!(supervisor.restart_count(), 0);
        assert!(!supervisor.circuit_breaker_open());
        assert_eq!(controller.state().start_calls, 0);
    }

    #[tokio::test]
    async fn successful_restart_resets_backoff_counter() {
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 10,
                backoff_base_ms: 100,
                backoff_factor: 2.0,
                backoff_max_ms: 30_000,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        supervisor
            .handle_crash()
            .await
            .expect("restart should recover after successful self-check");

        assert_eq!(supervisor.state(), SidecarState::Ready);
        // Backoff counter is NOT reset immediately; it only resets after sustained healthy operation.
        assert_eq!(supervisor.restart_count(), 1);
        assert!(!supervisor.circuit_breaker_open());

        let state = controller.state();
        assert_eq!(state.start_calls, 1);
        assert_eq!(state.ping_calls, 1);
    }

    #[tokio::test]
    async fn successful_recovery_after_failures_resets_restart_progression() {
        println!("[SUPERVISOR_TEST] verifying reset after successful recovery");
        let controller = FakeController::default();
        controller.set_fail_start(true);
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 10,
                backoff_base_ms: 0,
                backoff_factor: 2.0,
                backoff_max_ms: 0,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        let _ = supervisor
            .handle_crash()
            .await
            .expect_err("first restart should fail while spawn is forced down");
        assert_eq!(supervisor.restart_count(), 1);
        assert_eq!(supervisor.state(), SidecarState::Failed);

        controller.set_fail_start(false);
        supervisor
            .handle_crash()
            .await
            .expect("restart should recover once spawn failures stop");

        assert_eq!(supervisor.state(), SidecarState::Ready);
        // restart_count is NOT reset immediately on successful start; only after sustained healthy operation.
        assert_eq!(supervisor.restart_count(), 2);
        assert!(!supervisor.circuit_breaker_open());
    }

    #[tokio::test]
    async fn backoff_resets_after_sustained_healthy_operation() {
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 10,
                backoff_base_ms: 100,
                backoff_factor: 2.0,
                backoff_max_ms: 5000,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        // First crash: restart succeeds
        supervisor
            .handle_crash()
            .await
            .expect("restart should succeed");
        assert_eq!(supervisor.restart_count(), 1);

        // Simulate the sidecar having been ready for longer than the sustained threshold
        supervisor.ready_since = Some(Instant::now() - SUSTAINED_READY_THRESHOLD - Duration::from_secs(1));

        // Second crash: should reset backoff because sidecar was healthy for > threshold
        supervisor
            .handle_crash()
            .await
            .expect("restart should succeed after backoff reset");

        // restart_count was reset to 0 before incrementing to 1 for this crash
        assert_eq!(supervisor.restart_count(), 1);
    }

    #[tokio::test]
    async fn backoff_does_not_reset_after_quick_crash() {
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 10,
                backoff_base_ms: 0,
                backoff_factor: 2.0,
                backoff_max_ms: 0,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        // First crash: restart succeeds, restart_count = 1
        supervisor
            .handle_crash()
            .await
            .expect("restart should succeed");
        assert_eq!(supervisor.restart_count(), 1);

        // ready_since was just set by start(), so < threshold
        // Second crash: backoff should NOT reset because not sustained-healthy
        supervisor
            .handle_crash()
            .await
            .expect("restart should succeed");
        assert_eq!(supervisor.restart_count(), 2, "backoff should not reset after quick crash");
    }

    #[tokio::test]
    async fn handle_crash_stops_lingering_process_before_starting_new_one() {
        // Regression: 3kwp — a hung (still alive) sidecar was never killed before
        // starting its replacement, leading to duplicate processes.
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 10,
                backoff_base_ms: 0,
                backoff_factor: 2.0,
                backoff_max_ms: 0,
                circuit_breaker_window_ms: 60_000,
                auto_restart_enabled: true,
                captured_log_max_lines: DEFAULT_CAPTURED_LOG_MAX_LINES,
            },
        );

        supervisor
            .handle_crash()
            .await
            .expect("crash recovery should succeed");

        let state = controller.state();
        // handle_crash must call stop() to kill a potentially hung process
        // before calling start() to spawn a new one.
        assert!(
            state.stop_calls >= 1,
            "handle_crash must stop lingering process before starting replacement (stop_calls={})",
            state.stop_calls
        );
        assert_eq!(state.start_calls, 1);
        assert_eq!(state.ping_calls, 1);
        assert_eq!(supervisor.state(), SidecarState::Ready);
    }

    #[test]
    fn status_payload_is_structured_and_includes_required_fields() {
        let controller = FakeController::default();
        let supervisor = SidecarSupervisor::new(controller, SidecarSupervisorConfig::default());

        let payload = supervisor.status_payload(Some("starting sidecar"));
        assert!(payload.get("seq").and_then(Value::as_u64).is_some());
        assert_eq!(
            payload.get("state").and_then(Value::as_str),
            Some("stopped")
        );
        assert_eq!(
            payload.get("restart_count").and_then(Value::as_u64),
            Some(0)
        );
        assert_eq!(
            payload.get("message").and_then(Value::as_str),
            Some("starting sidecar")
        );
    }

    #[test]
    fn status_payload_omits_message_when_not_provided() {
        let controller = FakeController::default();
        let supervisor = SidecarSupervisor::new(controller, SidecarSupervisorConfig::default());

        let payload = supervisor.status_payload(None);
        assert!(payload.get("seq").and_then(Value::as_u64).is_some());
        assert_eq!(
            payload.get("state").and_then(Value::as_str),
            Some("stopped")
        );
        assert_eq!(
            payload.get("restart_count").and_then(Value::as_u64),
            Some(0)
        );
        assert!(payload.get("message").is_none());
    }
}
