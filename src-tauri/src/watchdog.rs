//! Watchdog for sidecar health monitoring and OS resume handling.
//!
//! This module provides:
//! - Periodic health checks to detect hung (non-responsive) sidecar
//! - OS suspend/resume event handling
//! - Revalidation of sidecar, devices, and model after system resume

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;

/// Default interval between health checks.
const DEFAULT_CHECK_INTERVAL: Duration = Duration::from_secs(10);

/// Timeout for ping response before considering sidecar unresponsive.
const PING_TIMEOUT: Duration = Duration::from_secs(5);

/// Duration of unresponsiveness before declaring sidecar hung.
const HANG_THRESHOLD: Duration = Duration::from_secs(30);

/// Minimum loop gap treated as likely suspend/resume.
const RESUME_GAP_MIN_THRESHOLD: Duration = Duration::from_secs(20);

/// Watchdog health check result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Sidecar is responding normally.
    Healthy,
    /// Sidecar missed a ping but within threshold.
    Unhealthy,
    /// Sidecar has been unresponsive beyond threshold.
    Hung,
    /// Sidecar is not running (no process).
    NotRunning,
}

/// System power event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerEvent {
    /// System is about to suspend/sleep.
    Suspending,
    /// System has resumed from suspend/sleep.
    Resumed,
}

/// Watchdog event emitted to listeners.
#[derive(Debug, Clone)]
pub enum WatchdogEvent {
    /// Health check completed.
    HealthCheck { status: HealthStatus },
    /// Sidecar appears hung and supervisor recovery should be attempted.
    SidecarRecoveryRequested { reason: String },
    /// Legacy hang signal kept for backward compatibility.
    SidecarHung,
    /// System resumed from suspend.
    SystemResumed,
    /// Revalidation needed after resume.
    RevalidationNeeded,
}

/// Callback trait for ping operations.
pub trait PingCallback: Send + Sync {
    /// Attempt to ping the sidecar.
    /// Returns Ok(()) if successful, Err with reason if failed.
    fn ping(&self) -> impl std::future::Future<Output = Result<(), String>> + Send;
}

/// Watchdog state tracking.
struct WatchdogState {
    /// Last successful activity timestamp.
    last_activity: Instant,
    /// Last health status.
    last_status: HealthStatus,
    /// Whether the system is currently suspended.
    is_suspended: bool,
    /// Whether a resume revalidation is pending.
    revalidation_pending: bool,
}

impl Default for WatchdogState {
    fn default() -> Self {
        Self {
            last_activity: Instant::now(),
            last_status: HealthStatus::NotRunning,
            is_suspended: false,
            revalidation_pending: false,
        }
    }
}

/// Watchdog configuration.
#[derive(Debug, Clone)]
pub struct WatchdogConfig {
    /// Interval between health checks.
    pub check_interval: Duration,
    /// Timeout for ping operations.
    pub ping_timeout: Duration,
    /// Duration before declaring hung.
    pub hang_threshold: Duration,
    /// Whether to auto-restart on hang detection.
    pub auto_restart_on_hang: bool,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            check_interval: DEFAULT_CHECK_INTERVAL,
            ping_timeout: PING_TIMEOUT,
            hang_threshold: HANG_THRESHOLD,
            auto_restart_on_hang: true,
        }
    }
}

/// Watchdog for monitoring sidecar health.
pub struct Watchdog {
    /// Internal state.
    state: Arc<RwLock<WatchdogState>>,
    /// Configuration.
    config: WatchdogConfig,
    /// Event broadcaster.
    event_tx: broadcast::Sender<WatchdogEvent>,
    /// Shutdown flag.
    shutdown_flag: Arc<AtomicBool>,
}

impl Watchdog {
    /// Create a new watchdog with default configuration.
    pub fn new() -> Self {
        Self::with_config(WatchdogConfig::default())
    }

    /// Create a new watchdog with custom configuration.
    pub fn with_config(config: WatchdogConfig) -> Self {
        let (event_tx, _) = broadcast::channel(32);

        Self {
            state: Arc::new(RwLock::new(WatchdogState::default())),
            config,
            event_tx,
            shutdown_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Subscribe to watchdog events.
    pub fn subscribe(&self) -> broadcast::Receiver<WatchdogEvent> {
        self.event_tx.subscribe()
    }

    /// Mark activity - call this when sidecar responds to any RPC.
    pub async fn mark_activity(&self) {
        let mut state = self.state.write().await;
        state.last_activity = Instant::now();
        state.last_status = HealthStatus::Healthy;
    }

    /// Get current health status without performing a check.
    pub async fn get_status(&self) -> HealthStatus {
        self.state.read().await.last_status
    }

    /// Get time since last activity.
    pub async fn time_since_activity(&self) -> Duration {
        self.state.read().await.last_activity.elapsed()
    }

    /// Perform a health check.
    ///
    /// This is typically called by the watchdog loop, but can also be
    /// called manually to force a check.
    pub async fn check_health<P: PingCallback>(&self, pinger: &P) -> HealthStatus {
        let mut state = self.state.write().await;

        // If system is suspended, don't check
        if state.is_suspended {
            return state.last_status;
        }

        // Try to ping with timeout
        let ping_result = tokio::time::timeout(self.config.ping_timeout, pinger.ping()).await;

        let status = match ping_result {
            Ok(Ok(())) => {
                // Ping successful
                state.last_activity = Instant::now();
                HealthStatus::Healthy
            }
            Ok(Err(e)) => {
                // Ping failed (RPC error)
                log::warn!("Watchdog ping failed: {}", e);
                if state.last_activity.elapsed() > self.config.hang_threshold {
                    HealthStatus::Hung
                } else {
                    HealthStatus::Unhealthy
                }
            }
            Err(_) => {
                // Ping timed out
                log::warn!("Watchdog ping timed out");
                if state.last_activity.elapsed() > self.config.hang_threshold {
                    HealthStatus::Hung
                } else {
                    HealthStatus::Unhealthy
                }
            }
        };

        state.last_status = status;
        status
    }

    /// Handle a power event from the OS.
    pub async fn on_power_event(&self, event: PowerEvent) {
        let mut state = self.state.write().await;

        match event {
            PowerEvent::Suspending => {
                log::info!("Watchdog: System suspending");
                state.is_suspended = true;
            }
            PowerEvent::Resumed => {
                log::info!("Watchdog: System resumed");
                state.is_suspended = false;
                state.revalidation_pending = true;

                // Emit resume event
                let _ = self.event_tx.send(WatchdogEvent::SystemResumed);
                let _ = self.event_tx.send(WatchdogEvent::RevalidationNeeded);
            }
        }
    }

    /// Check if revalidation is needed (after resume).
    pub async fn is_revalidation_pending(&self) -> bool {
        self.state.read().await.revalidation_pending
    }

    /// Clear the revalidation pending flag.
    pub async fn clear_revalidation_pending(&self) {
        self.state.write().await.revalidation_pending = false;
    }

    /// Mark sidecar as not running.
    pub async fn mark_not_running(&self) {
        let mut state = self.state.write().await;
        state.last_status = HealthStatus::NotRunning;
    }

    /// Start the watchdog monitoring loop.
    ///
    /// This spawns a background task that periodically checks health.
    /// The loop runs until shutdown() is called.
    pub fn start_loop<P: PingCallback + 'static>(&self, pinger: Arc<P>) {
        let state = Arc::clone(&self.state);
        let config = self.config.clone();
        let event_tx = self.event_tx.clone();
        let shutdown_flag = Arc::clone(&self.shutdown_flag);

        tokio::spawn(async move {
            let mut check_interval = interval(config.check_interval);
            let mut last_tick = Instant::now();
            log::info!(
                "Watchdog loop started (interval: {:?})",
                config.check_interval
            );

            loop {
                check_interval.tick().await;
                let now = Instant::now();
                let loop_gap = now.saturating_duration_since(last_tick);
                last_tick = now;

                // Check shutdown flag
                if shutdown_flag.load(Ordering::SeqCst) {
                    log::info!("Watchdog loop shutting down");
                    break;
                }

                // Fallback resume detection when platform listeners are unavailable.
                if is_probable_resume_gap(loop_gap, config.check_interval) {
                    let should_emit = {
                        let mut state_guard = state.write().await;
                        state_guard.is_suspended = false;
                        if state_guard.revalidation_pending {
                            false
                        } else {
                            state_guard.revalidation_pending = true;
                            true
                        }
                    };

                    if should_emit {
                        log::info!(
                            "Watchdog inferred suspend/resume from loop gap {:?}; requesting revalidation",
                            loop_gap
                        );
                        let _ = event_tx.send(WatchdogEvent::SystemResumed);
                        let _ = event_tx.send(WatchdogEvent::RevalidationNeeded);
                    }
                }

                // Skip check if suspended
                {
                    let state_guard = state.read().await;
                    if state_guard.is_suspended {
                        continue;
                    }
                }

                // Perform health check
                let ping_result = tokio::time::timeout(config.ping_timeout, pinger.ping()).await;

                let status = {
                    let mut state_guard = state.write().await;

                    match ping_result {
                        Ok(Ok(())) => {
                            state_guard.last_activity = Instant::now();
                            state_guard.last_status = HealthStatus::Healthy;
                            HealthStatus::Healthy
                        }
                        Ok(Err(e)) => {
                            log::warn!("Watchdog: ping error: {}", e);
                            let elapsed = state_guard.last_activity.elapsed();
                            if elapsed > config.hang_threshold {
                                state_guard.last_status = HealthStatus::Hung;
                                HealthStatus::Hung
                            } else {
                                state_guard.last_status = HealthStatus::Unhealthy;
                                HealthStatus::Unhealthy
                            }
                        }
                        Err(_) => {
                            log::warn!("Watchdog: ping timeout");
                            let elapsed = state_guard.last_activity.elapsed();
                            if elapsed > config.hang_threshold {
                                state_guard.last_status = HealthStatus::Hung;
                                HealthStatus::Hung
                            } else {
                                state_guard.last_status = HealthStatus::Unhealthy;
                                HealthStatus::Unhealthy
                            }
                        }
                    }
                };

                // Emit health check event
                let _ = event_tx.send(WatchdogEvent::HealthCheck { status });

                // If hung and auto-restart is enabled, request supervisor-mediated recovery.
                if status == HealthStatus::Hung && config.auto_restart_on_hang {
                    log::error!("Watchdog: Sidecar appears hung, requesting supervisor recovery");
                    let _ = event_tx.send(WatchdogEvent::SidecarRecoveryRequested {
                        reason: "sidecar_hung".to_string(),
                    });
                    // Keep emitting the legacy event during migration.
                    let _ = event_tx.send(WatchdogEvent::SidecarHung);
                }
            }
        });
    }

    /// Signal the watchdog loop to shutdown.
    pub fn shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
    }

    /// Check if shutdown has been requested.
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_flag.load(Ordering::SeqCst)
    }
}

fn is_probable_resume_gap(loop_gap: Duration, check_interval: Duration) -> bool {
    let dynamic_threshold = check_interval.saturating_mul(3);
    let threshold = dynamic_threshold.max(RESUME_GAP_MIN_THRESHOLD);
    loop_gap > threshold
}

impl Default for Watchdog {
    fn default() -> Self {
        Self::new()
    }
}

// Platform-specific resume detection

/// Platform-specific power event listener.
#[cfg(target_os = "windows")]
pub mod platform {
    use super::PowerEvent;

    /// Start listening for Windows power events.
    /// Returns a receiver that emits PowerEvent when suspend/resume occurs.
    pub fn start_power_listener() -> Option<tokio::sync::mpsc::Receiver<PowerEvent>> {
        // Windows implementation would use WM_POWERBROADCAST
        // For now, return None to indicate not implemented
        log::info!("Windows power event listener not yet implemented");
        None
    }
}

#[cfg(target_os = "macos")]
pub mod platform {
    use super::PowerEvent;

    /// Start listening for macOS power events.
    /// Returns a receiver that emits PowerEvent when suspend/resume occurs.
    pub fn start_power_listener() -> Option<tokio::sync::mpsc::Receiver<PowerEvent>> {
        // macOS implementation would use NSWorkspace notifications
        // For now, return None to indicate not implemented
        log::info!("macOS power event listener not yet implemented");
        None
    }
}

#[cfg(target_os = "linux")]
pub mod platform {
    use super::PowerEvent;

    /// Start listening for Linux power events.
    /// Returns a receiver that emits PowerEvent when suspend/resume occurs.
    pub fn start_power_listener() -> Option<tokio::sync::mpsc::Receiver<PowerEvent>> {
        // Linux implementation would use systemd-logind D-Bus signals
        // For now, return None to indicate not implemented
        log::info!("Linux power event listener not yet implemented");
        None
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub mod platform {
    use super::PowerEvent;

    pub fn start_power_listener() -> Option<tokio::sync::mpsc::Receiver<PowerEvent>> {
        log::info!("Power event listener not supported on this platform");
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    /// Mock pinger for testing.
    struct MockPinger {
        should_succeed: Arc<AtomicBool>,
        call_count: Arc<AtomicU32>,
    }

    impl MockPinger {
        fn new(should_succeed: bool) -> Self {
            Self {
                should_succeed: Arc::new(AtomicBool::new(should_succeed)),
                call_count: Arc::new(AtomicU32::new(0)),
            }
        }

        fn set_success(&self, succeed: bool) {
            self.should_succeed.store(succeed, Ordering::SeqCst);
        }

        fn get_call_count(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    impl PingCallback for MockPinger {
        async fn ping(&self) -> Result<(), String> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.should_succeed.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err("Mock ping failed".to_string())
            }
        }
    }

    #[test]
    fn test_watchdog_config_default() {
        let config = WatchdogConfig::default();
        assert_eq!(config.check_interval, DEFAULT_CHECK_INTERVAL);
        assert_eq!(config.ping_timeout, PING_TIMEOUT);
        assert_eq!(config.hang_threshold, HANG_THRESHOLD);
        assert!(config.auto_restart_on_hang);
    }

    #[test]
    fn test_health_status_serialization() {
        let statuses = [
            (HealthStatus::Healthy, "healthy"),
            (HealthStatus::Unhealthy, "unhealthy"),
            (HealthStatus::Hung, "hung"),
            (HealthStatus::NotRunning, "not_running"),
        ];

        for (status, expected) in statuses {
            let json = serde_json::to_string(&status).unwrap();
            assert!(
                json.contains(expected),
                "Status {:?} should serialize to contain '{}', got {}",
                status,
                expected,
                json
            );
        }
    }

    #[tokio::test]
    async fn test_watchdog_initial_state() {
        let watchdog = Watchdog::new();
        // Initial status should be NotRunning (default)
        let status = watchdog.get_status().await;
        assert_eq!(status, HealthStatus::NotRunning);
    }

    #[tokio::test]
    async fn test_mark_activity() {
        let watchdog = Watchdog::new();

        // Mark activity
        watchdog.mark_activity().await;

        // Status should be healthy
        let status = watchdog.get_status().await;
        assert_eq!(status, HealthStatus::Healthy);

        // Time since activity should be very small
        let elapsed = watchdog.time_since_activity().await;
        assert!(elapsed < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_check_health_success() {
        let watchdog = Watchdog::new();
        let pinger = MockPinger::new(true);

        let status = watchdog.check_health(&pinger).await;
        assert_eq!(status, HealthStatus::Healthy);
        assert_eq!(pinger.get_call_count(), 1);
    }

    #[tokio::test]
    async fn test_check_health_failure_within_threshold() {
        let config = WatchdogConfig {
            hang_threshold: Duration::from_secs(60), // Long threshold
            ..Default::default()
        };
        let watchdog = Watchdog::with_config(config);
        let pinger = MockPinger::new(false);

        // Mark some recent activity
        watchdog.mark_activity().await;

        // Check should return unhealthy (not hung yet)
        let status = watchdog.check_health(&pinger).await;
        assert_eq!(status, HealthStatus::Unhealthy);
    }

    #[tokio::test]
    async fn test_check_health_failure_beyond_threshold() {
        let config = WatchdogConfig {
            hang_threshold: Duration::from_millis(1), // Very short threshold
            ..Default::default()
        };
        let watchdog = Watchdog::with_config(config);
        let pinger = MockPinger::new(false);

        // Wait for threshold to pass
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Check should return hung
        let status = watchdog.check_health(&pinger).await;
        assert_eq!(status, HealthStatus::Hung);
    }

    #[tokio::test]
    async fn test_power_events() {
        let watchdog = Watchdog::new();

        // Handle suspend
        watchdog.on_power_event(PowerEvent::Suspending).await;

        // Revalidation should not be pending after suspend
        assert!(!watchdog.is_revalidation_pending().await);

        // Handle resume
        watchdog.on_power_event(PowerEvent::Resumed).await;

        // Revalidation should be pending after resume
        assert!(watchdog.is_revalidation_pending().await);

        // Clear it
        watchdog.clear_revalidation_pending().await;
        assert!(!watchdog.is_revalidation_pending().await);
    }

    #[tokio::test]
    async fn test_suspend_skips_health_check() {
        let watchdog = Watchdog::new();
        let pinger = MockPinger::new(true);

        // Simulate suspend
        watchdog.on_power_event(PowerEvent::Suspending).await;

        // Health check should return previous status without calling ping
        let status = watchdog.check_health(&pinger).await;
        assert_eq!(status, HealthStatus::NotRunning); // Default status
        assert_eq!(pinger.get_call_count(), 0); // Ping was not called
    }

    #[tokio::test]
    async fn test_mark_not_running() {
        let watchdog = Watchdog::new();

        // First mark as healthy
        watchdog.mark_activity().await;
        assert_eq!(watchdog.get_status().await, HealthStatus::Healthy);

        // Then mark as not running
        watchdog.mark_not_running().await;
        assert_eq!(watchdog.get_status().await, HealthStatus::NotRunning);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let watchdog = Watchdog::new();
        let mut receiver = watchdog.subscribe();

        // Trigger a power event
        watchdog.on_power_event(PowerEvent::Resumed).await;

        // Should receive events
        let event = tokio::time::timeout(Duration::from_millis(100), receiver.recv())
            .await
            .expect("timeout")
            .expect("recv error");

        match event {
            WatchdogEvent::SystemResumed => {}
            _ => panic!("Expected SystemResumed event"),
        }
    }

    #[test]
    fn test_shutdown_flag() {
        let watchdog = Watchdog::new();

        assert!(!watchdog.is_shutdown_requested());

        watchdog.shutdown();

        assert!(watchdog.is_shutdown_requested());
    }

    #[tokio::test]
    async fn test_watchdog_loop_with_healthy_pinger() {
        let config = WatchdogConfig {
            check_interval: Duration::from_millis(50),
            ..Default::default()
        };
        let watchdog = Watchdog::with_config(config);
        let pinger = Arc::new(MockPinger::new(true));
        let mut receiver = watchdog.subscribe();

        // Start the loop
        watchdog.start_loop(Arc::clone(&pinger));

        // Wait for a few checks
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Should have received health check events
        let mut healthy_count = 0;
        while let Ok(event) = tokio::time::timeout(Duration::from_millis(10), receiver.recv()).await
        {
            if let Ok(WatchdogEvent::HealthCheck { status }) = event {
                if status == HealthStatus::Healthy {
                    healthy_count += 1;
                }
            }
        }

        assert!(healthy_count >= 2, "Expected at least 2 healthy checks");

        // Shutdown
        watchdog.shutdown();
    }

    #[tokio::test]
    async fn test_watchdog_loop_detects_hung_and_requests_supervisor_recovery() {
        let config = WatchdogConfig {
            check_interval: Duration::from_millis(20),
            hang_threshold: Duration::from_millis(50),
            auto_restart_on_hang: true,
            ..Default::default()
        };
        let watchdog = Watchdog::with_config(config);
        let pinger = Arc::new(MockPinger::new(false)); // Always fails
        let mut receiver = watchdog.subscribe();

        // Start the loop
        watchdog.start_loop(Arc::clone(&pinger));

        // Wait for hung detection + supervisor recovery request.
        let mut recovery_event_received = false;
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(500) {
            if let Ok(Ok(event)) =
                tokio::time::timeout(Duration::from_millis(50), receiver.recv()).await
            {
                match event {
                    WatchdogEvent::SidecarRecoveryRequested { reason } => {
                        assert_eq!(reason, "sidecar_hung");
                        recovery_event_received = true;
                        break;
                    }
                    WatchdogEvent::SidecarHung => {
                        // Legacy event should continue to be emitted, but do not rely on ordering.
                    }
                    _ => {}
                }
            }
        }

        assert!(
            recovery_event_received,
            "Expected SidecarRecoveryRequested event"
        );

        // Shutdown
        watchdog.shutdown();
    }

    #[tokio::test]
    async fn test_watchdog_loop_skips_recovery_request_when_auto_restart_disabled() {
        let config = WatchdogConfig {
            check_interval: Duration::from_millis(20),
            hang_threshold: Duration::from_millis(50),
            auto_restart_on_hang: false,
            ..Default::default()
        };
        let watchdog = Watchdog::with_config(config);
        let pinger = Arc::new(MockPinger::new(false)); // Always fails
        let mut receiver = watchdog.subscribe();

        watchdog.start_loop(Arc::clone(&pinger));

        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(300) {
            if let Ok(Ok(event)) =
                tokio::time::timeout(Duration::from_millis(50), receiver.recv()).await
            {
                if matches!(
                    event,
                    WatchdogEvent::SidecarRecoveryRequested { .. } | WatchdogEvent::SidecarHung
                ) {
                    panic!("Did not expect recovery request when auto_restart_on_hang is false");
                }
            }
        }

        watchdog.shutdown();
    }

    #[test]
    fn test_power_event_enum() {
        // Just verify the enum variants exist and can be compared
        assert_eq!(PowerEvent::Suspending, PowerEvent::Suspending);
        assert_eq!(PowerEvent::Resumed, PowerEvent::Resumed);
        assert_ne!(PowerEvent::Suspending, PowerEvent::Resumed);
    }

    #[test]
    fn test_probable_resume_gap_uses_dynamic_and_min_thresholds() {
        // check_interval=2s -> threshold=max(6s,20s)=20s
        assert!(!is_probable_resume_gap(
            Duration::from_secs(19),
            Duration::from_secs(2)
        ));
        assert!(is_probable_resume_gap(
            Duration::from_secs(21),
            Duration::from_secs(2)
        ));

        // check_interval=10s -> threshold=max(30s,20s)=30s
        assert!(!is_probable_resume_gap(
            Duration::from_secs(30),
            Duration::from_secs(10)
        ));
        assert!(is_probable_resume_gap(
            Duration::from_secs(31),
            Duration::from_secs(10)
        ));
    }

    #[tokio::test]
    async fn test_recovery_after_ping_success() {
        let config = WatchdogConfig {
            hang_threshold: Duration::from_millis(1),
            ..Default::default()
        };
        let watchdog = Watchdog::with_config(config);
        let pinger = MockPinger::new(false);

        // Wait to exceed threshold
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Should be hung
        let status = watchdog.check_health(&pinger).await;
        assert_eq!(status, HealthStatus::Hung);

        // Now ping succeeds
        pinger.set_success(true);

        // Should recover to healthy
        let status = watchdog.check_health(&pinger).await;
        assert_eq!(status, HealthStatus::Healthy);
    }
}
