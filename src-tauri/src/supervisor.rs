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
    pub max_restart_count: u32,
    pub rapid_failure_window: Duration,
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
}

impl Default for SidecarSupervisorConfig {
    fn default() -> Self {
        Self {
            max_restart_count: 5,
            rapid_failure_window: Duration::from_secs(60),
            backoff_initial: Duration::from_millis(250),
            backoff_max: Duration::from_secs(10),
        }
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
}

pub struct SidecarSupervisor<C = SidecarManager>
where
    C: SidecarController,
{
    controller: C,
    pub config: SidecarSupervisorConfig,
    state: SidecarState,
    restart_count: u32,
    last_restart_at: Option<Instant>,
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

    pub fn circuit_breaker_open(&self) -> bool {
        self.circuit_breaker.is_open
    }

    pub fn record_log_line(&mut self, stream: SidecarLogStream, line: impl Into<String>) {
        self.captured_logs.push_back(SidecarLogRecord {
            stream,
            line: line.into(),
            captured_at: Instant::now(),
        });
    }

    pub fn drain_captured_logs(&mut self) -> Vec<SidecarLogRecord> {
        self.captured_logs.drain(..).collect()
    }

    pub async fn start(&mut self) -> Result<(), String> {
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

        self.state = SidecarState::Ready;
        self.emit_status(None);
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), String> {
        self.controller.stop()?;
        self.state = SidecarState::Stopped;
        self.emit_status(Some("sidecar stopped"));
        Ok(())
    }

    pub async fn restart(&mut self) -> Result<(), String> {
        self.reset_circuit_breaker();
        self.restart_count = 0;
        self.state = SidecarState::Restarting;
        self.emit_status(Some("manual sidecar restart requested"));

        let _ = self.controller.stop();
        self.start().await
    }

    pub async fn handle_crash(&mut self) -> Result<(), String> {
        self.state = SidecarState::Failed;
        self.emit_status(Some("sidecar crash detected"));

        let now = Instant::now();
        self.register_failure(now);

        if !self.should_auto_restart() {
            self.state = SidecarState::Stopped;
            self.emit_status(Some("automatic restart disabled by circuit breaker"));
            return Ok(());
        }

        self.restart_count = self.restart_count.saturating_add(1);
        self.last_restart_at = Some(now);
        let delay = self.backoff_delay_for_attempt(self.restart_count);

        self.state = SidecarState::Restarting;
        self.emit_status(Some(&format!(
            "restarting sidecar in {}ms (attempt {})",
            delay.as_millis(),
            self.restart_count
        )));

        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }

        self.start().await
    }

    fn should_auto_restart(&self) -> bool {
        !self.circuit_breaker.is_open
    }

    fn reset_circuit_breaker(&mut self) {
        self.circuit_breaker = CircuitBreakerState::default();
    }

    fn register_failure(&mut self, now: Instant) {
        let within_window = self
            .circuit_breaker
            .last_failure_at
            .map(|previous| now.duration_since(previous) <= self.config.rapid_failure_window)
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
        if self.config.backoff_initial.is_zero() {
            return Duration::ZERO;
        }

        let exponent = attempt.saturating_sub(2).min(62);
        let multiplier = 1u128 << exponent;
        let base_ms = self.config.backoff_initial.as_millis();
        let max_ms = self.config.backoff_max.as_millis();
        let delay_ms = base_ms.saturating_mul(multiplier).min(max_ms);

        let delay_u64 = if delay_ms > u128::from(u64::MAX) {
            u64::MAX
        } else {
            delay_ms as u64
        };
        Duration::from_millis(delay_u64)
    }

    fn emit_status(&self, message: Option<&str>) {
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

        let payload = crate::event_seq::payload_with_next_seq(payload);
        if let Some(app_handle) = &self.app_handle {
            let _ = app_handle.emit(EVENT_SIDECAR_STATUS, payload);
        }
    }

    #[allow(dead_code)]
    fn status_payload_for_testing(&self, message: Option<&str>) -> Value {
        let mut payload = json!({
            "state": self.state.as_str(),
            "restart_count": self.restart_count,
        });
        if let Some(message) = message {
            payload
                .as_object_mut()
                .expect("payload object")
                .insert("message".to_string(), json!(message));
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
            }
        }

        fn set_fail_ping(&self, fail_ping: bool) {
            self.inner
                .lock()
                .expect("controller state lock poisoned")
                .fail_ping = fail_ping;
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
    }

    #[test]
    fn backoff_policy_is_immediate_then_exponential() {
        let controller = FakeController::default();
        let supervisor = SidecarSupervisor::new(
            controller,
            SidecarSupervisorConfig {
                max_restart_count: 10,
                rapid_failure_window: Duration::from_secs(30),
                backoff_initial: Duration::from_millis(100),
                backoff_max: Duration::from_millis(500),
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
    async fn handle_crash_trips_circuit_breaker_after_rapid_failures() {
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 2,
                rapid_failure_window: Duration::from_secs(60),
                backoff_initial: Duration::ZERO,
                backoff_max: Duration::ZERO,
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
        assert_eq!(supervisor.state(), SidecarState::Stopped);
        assert!(supervisor.circuit_breaker_open());

        let state = controller.state();
        assert_eq!(state.start_calls, 1, "only first crash should auto-restart");
    }

    #[tokio::test]
    async fn manual_restart_resets_circuit_breaker() {
        let controller = FakeController::default();
        let mut supervisor = SidecarSupervisor::new(
            controller.clone(),
            SidecarSupervisorConfig {
                max_restart_count: 1,
                rapid_failure_window: Duration::from_secs(60),
                backoff_initial: Duration::ZERO,
                backoff_max: Duration::ZERO,
            },
        );

        supervisor
            .handle_crash()
            .await
            .expect("breaker opens on first rapid failure");
        assert!(supervisor.circuit_breaker_open());
        assert_eq!(supervisor.state(), SidecarState::Stopped);

        supervisor
            .restart()
            .await
            .expect("manual restart should always be allowed");
        assert_eq!(supervisor.state(), SidecarState::Ready);
        assert!(!supervisor.circuit_breaker_open());
        assert_eq!(supervisor.restart_count(), 0);
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
}
