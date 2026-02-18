//! Sidecar process lifecycle management.
//!
//! This module handles spawning, monitoring, and restarting the Python
//! sidecar process that performs ASR transcription.
//!
//! In release builds, the sidecar is a bundled PyInstaller binary.
//! In debug builds, it runs via Python interpreter for faster iteration.

#![allow(dead_code)] // Methods will be used in future RPC client implementation

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::errors::AppError;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

/// Maximum number of restart attempts before giving up.
const MAX_RESTART_ATTEMPTS: u32 = 5;

/// Backoff delays in milliseconds: 250 → 500 → 1000 → 2000 → 10000
const BACKOFF_DELAYS_MS: [u64; 5] = [250, 500, 1000, 2000, 10000];

/// Event name for sidecar status changes
const EVENT_SIDECAR_STATUS: &str = "sidecar:status";

/// Sidecar lifecycle state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SidecarState {
    /// Initial state before first spawn
    NotStarted,
    /// Spawn in progress
    Starting,
    /// Process alive and IPC working
    Running,
    /// Crashed, waiting for backoff before restart
    Restarting,
    /// Max retries exceeded, manual intervention needed
    Failed,
    /// Graceful shutdown in progress
    ShuttingDown,
}

/// Status event payload
#[derive(Debug, Clone, Serialize)]
pub struct SidecarStatus {
    pub state: SidecarState,
    pub restart_count: u32,
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AppError>,
}

/// JSON-RPC response for ping
#[derive(Debug, Deserialize)]
struct PingResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: serde_json::Value,
    result: Option<PingResult>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct PingResult {
    protocol: String,
    version: String,
}

/// Internal sidecar state
struct SidecarInner {
    state: SidecarState,
    restart_count: u32,
    child: Option<Child>,
    last_error: Option<String>,
}

/// Sidecar spawn mode
#[derive(Debug, Clone)]
enum SpawnMode {
    /// Use bundled binary (release mode)
    Bundled,
    /// Use Python interpreter (development mode)
    Python { path: String, module: String },
}

/// Sidecar manager for process lifecycle management.
pub struct SidecarManager {
    inner: Arc<Mutex<SidecarInner>>,
    shutdown_flag: Arc<AtomicBool>,
    app_handle: Option<AppHandle>,
    spawn_mode: SpawnMode,
}

impl SidecarManager {
    /// Create a new sidecar manager.
    ///
    /// In release builds, defaults to using the bundled binary.
    /// In debug builds, defaults to Python interpreter mode.
    pub fn new() -> Self {
        // Default spawn mode based on build type
        #[cfg(debug_assertions)]
        let spawn_mode = SpawnMode::Python {
            path: "python3".to_string(),
            module: "openvoicy_sidecar".to_string(),
        };

        #[cfg(not(debug_assertions))]
        let spawn_mode = SpawnMode::Bundled;

        Self {
            inner: Arc::new(Mutex::new(SidecarInner {
                state: SidecarState::NotStarted,
                restart_count: 0,
                child: None,
                last_error: None,
            })),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            app_handle: None,
            spawn_mode,
        }
    }

    /// Set the Tauri app handle for emitting events.
    pub fn set_app_handle(&mut self, handle: AppHandle) {
        self.app_handle = Some(handle);
    }

    /// Set Python mode for development.
    #[allow(dead_code)]
    pub fn set_python_mode(&mut self, path: String, module: String) {
        self.spawn_mode = SpawnMode::Python { path, module };
    }

    /// Set bundled mode (uses Tauri sidecar).
    #[allow(dead_code)]
    pub fn set_bundled_mode(&mut self) {
        self.spawn_mode = SpawnMode::Bundled;
    }

    /// Check if using bundled binary mode.
    #[allow(dead_code)]
    pub fn is_bundled_mode(&self) -> bool {
        matches!(self.spawn_mode, SpawnMode::Bundled)
    }

    /// Get the current sidecar state.
    pub fn get_state(&self) -> SidecarState {
        self.inner.lock().unwrap().state
    }

    /// Get the current status.
    pub fn get_status(&self) -> SidecarStatus {
        let inner = self.inner.lock().unwrap();
        let error = inner
            .last_error
            .as_deref()
            .and_then(|msg| Self::status_error(inner.state, inner.restart_count, msg));
        SidecarStatus {
            state: inner.state,
            restart_count: inner.restart_count,
            message: inner.last_error.clone(),
            error,
        }
    }

    fn status_error(state: SidecarState, restart_count: u32, message: &str) -> Option<AppError> {
        match state {
            SidecarState::Restarting => Some(AppError::new(
                "E_SIDECAR_RESTARTING",
                "Background service restarted after an error",
                Some(json!({
                    "restart_count": restart_count,
                    "message": message
                })),
                true,
            )),
            SidecarState::Failed => Some(AppError::new(
                "E_SIDECAR_FAILED",
                "Background service failed after multiple restart attempts",
                Some(json!({
                    "restart_count": restart_count,
                    "message": message
                })),
                false,
            )),
            _ => None,
        }
    }

    /// Emit a status event to the frontend.
    fn emit_status(&self, status: SidecarStatus) {
        if let Some(ref handle) = self.app_handle {
            let _ = handle.emit(EVENT_SIDECAR_STATUS, status);
        }
    }

    /// Start the sidecar process.
    pub fn start(&self) -> Result<(), String> {
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.state == SidecarState::Running {
                return Ok(()); // Already running
            }
            inner.state = SidecarState::Starting;
            inner.restart_count = 0;
        }

        self.emit_status(SidecarStatus {
            state: SidecarState::Starting,
            restart_count: 0,
            message: Some("Starting sidecar...".to_string()),
            error: None,
        });

        self.spawn_process()
    }

    /// Spawn the sidecar process.
    fn spawn_process(&self) -> Result<(), String> {
        log::info!("Spawning sidecar process (mode: {:?})", self.spawn_mode);

        let child = match &self.spawn_mode {
            SpawnMode::Python { path, module } => {
                // Development mode: run via Python interpreter
                log::info!("Using Python mode: {} -m {}", path, module);
                Command::new(path)
                    .arg("-m")
                    .arg(module)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("Failed to spawn Python sidecar: {}", e))?
            }
            SpawnMode::Bundled => {
                // Release mode: use bundled binary
                // Get the sidecar path from Tauri's resource directory
                let sidecar_path = self.get_bundled_sidecar_path()?;
                log::info!("Using bundled sidecar: {:?}", sidecar_path);

                // Handle macOS quarantine attribute removal
                #[cfg(target_os = "macos")]
                {
                    if let Err(e) = Self::remove_macos_quarantine(&sidecar_path) {
                        log::warn!("Failed to remove quarantine attribute: {}", e);
                    }
                }

                // Ensure executable permissions on Unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(metadata) = std::fs::metadata(&sidecar_path) {
                        let mut perms = metadata.permissions();
                        perms.set_mode(perms.mode() | 0o111);
                        let _ = std::fs::set_permissions(&sidecar_path, perms);
                    }
                }

                Command::new(&sidecar_path)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("Failed to spawn bundled sidecar: {}", e))?
            }
        };

        let pid = child.id();
        log::info!("Sidecar spawned with PID {}", pid);

        {
            let mut inner = self.inner.lock().unwrap();
            inner.child = Some(child);
            inner.state = SidecarState::Running;
            inner.last_error = None;
        }

        self.emit_status(SidecarStatus {
            state: SidecarState::Running,
            restart_count: self.inner.lock().unwrap().restart_count,
            message: Some(format!("Sidecar running (PID {})", pid)),
            error: None,
        });

        // Start monitoring thread
        self.start_monitor_thread();

        Ok(())
    }

    /// Get the path to the bundled sidecar binary.
    fn get_bundled_sidecar_path(&self) -> Result<std::path::PathBuf, String> {
        // In release mode, the sidecar is in the app's resource directory
        // Tauri places externalBin binaries alongside the main executable
        let app_handle = self
            .app_handle
            .as_ref()
            .ok_or_else(|| "App handle not set".to_string())?;

        // Get the path to the sidecar binary
        // Tauri 2.x: binaries are in the same directory as the main executable
        let exe_path =
            std::env::current_exe().map_err(|e| format!("Failed to get executable path: {}", e))?;

        let exe_dir = exe_path
            .parent()
            .ok_or_else(|| "Failed to get executable directory".to_string())?;

        // Try resource path first (for bundled apps)
        if let Ok(resource_dir) = app_handle.path().resource_dir() {
            let sidecar_name = Self::get_sidecar_binary_name();
            let resource_path = resource_dir.join(&sidecar_name);
            if resource_path.exists() {
                log::info!("Found sidecar in resource dir: {:?}", resource_path);
                return Ok(resource_path);
            }
        }

        // Fallback: same directory as executable
        let sidecar_name = Self::get_sidecar_binary_name();
        let sidecar_path = exe_dir.join(&sidecar_name);

        if sidecar_path.exists() {
            log::info!("Found sidecar in exe dir: {:?}", sidecar_path);
            Ok(sidecar_path)
        } else {
            // Development fallback: check dist directory
            let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("sidecar")
                .join("dist")
                .join(&sidecar_name);

            if dev_path.exists() {
                log::info!("Found sidecar in dev dist: {:?}", dev_path);
                Ok(dev_path)
            } else {
                Err(format!(
                    "Sidecar binary not found. Checked: {:?}, {:?}",
                    sidecar_path, dev_path
                ))
            }
        }
    }

    /// Get the sidecar binary name for the current platform.
    fn get_sidecar_binary_name() -> String {
        #[cfg(target_os = "windows")]
        {
            "openvoicy-sidecar.exe".to_string()
        }

        #[cfg(not(target_os = "windows"))]
        {
            "openvoicy-sidecar".to_string()
        }
    }

    /// Remove macOS quarantine attribute from the sidecar binary.
    #[cfg(target_os = "macos")]
    fn remove_macos_quarantine(path: &std::path::Path) -> Result<(), String> {
        use std::process::Command as StdCommand;

        log::info!("Removing quarantine attribute from {:?}", path);

        let output = StdCommand::new("xattr")
            .args(["-d", "com.apple.quarantine"])
            .arg(path)
            .output()
            .map_err(|e| format!("Failed to run xattr: {}", e))?;

        if !output.status.success() {
            // Ignore errors - file might not have quarantine attribute
            log::debug!(
                "xattr returned non-zero (this is ok if no quarantine): {:?}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    /// Start a thread to monitor the sidecar process.
    fn start_monitor_thread(&self) {
        let inner = Arc::clone(&self.inner);
        let shutdown_flag = Arc::clone(&self.shutdown_flag);
        let manager = self.clone_for_thread();

        thread::spawn(move || {
            loop {
                // Check shutdown flag
                if shutdown_flag.load(Ordering::SeqCst) {
                    log::info!("Monitor thread: shutdown requested");
                    break;
                }

                // Check if process is still running
                let should_restart = {
                    let mut inner_guard = inner.lock().unwrap();
                    if let Some(ref mut child) = inner_guard.child {
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                // Process exited
                                log::warn!("Sidecar exited with status: {:?}", status);
                                inner_guard.last_error =
                                    Some(format!("Process exited: {:?}", status));
                                inner_guard.child = None;
                                inner_guard.state != SidecarState::ShuttingDown
                            }
                            Ok(None) => {
                                // Still running
                                false
                            }
                            Err(e) => {
                                log::error!("Error checking sidecar status: {}", e);
                                false
                            }
                        }
                    } else {
                        false
                    }
                };

                if should_restart {
                    manager.handle_crash();
                    break; // New monitor will be started by spawn_process
                }

                thread::sleep(Duration::from_millis(100));
            }
        });
    }

    /// Handle a sidecar crash.
    fn handle_crash(&self) {
        let (restart_count, should_restart) = {
            let mut inner = self.inner.lock().unwrap();

            if inner.restart_count >= MAX_RESTART_ATTEMPTS {
                inner.state = SidecarState::Failed;
                self.emit_status(SidecarStatus {
                    state: SidecarState::Failed,
                    restart_count: inner.restart_count,
                    message: Some("Sidecar failed after multiple restart attempts".to_string()),
                    error: Some(AppError::new(
                        "E_SIDECAR_FAILED",
                        "Background service failed after multiple restart attempts",
                        Some(json!({
                            "restart_count": inner.restart_count
                        })),
                        false,
                    )),
                });
                return;
            }

            inner.restart_count += 1;
            inner.state = SidecarState::Restarting;
            (inner.restart_count, true)
        };

        if !should_restart {
            return;
        }

        let delay_ms = BACKOFF_DELAYS_MS
            .get((restart_count - 1) as usize)
            .copied()
            .unwrap_or(10000);

        log::info!(
            "Sidecar crashed, restarting in {}ms (attempt {}/{})",
            delay_ms,
            restart_count,
            MAX_RESTART_ATTEMPTS
        );

        self.emit_status(SidecarStatus {
            state: SidecarState::Restarting,
            restart_count,
            message: Some(format!(
                "Restarting in {}ms (attempt {}/{})",
                delay_ms, restart_count, MAX_RESTART_ATTEMPTS
            )),
            error: Some(AppError::new(
                "E_SIDECAR_RESTARTING",
                "Background service restarted after an error",
                Some(json!({
                    "restart_count": restart_count,
                    "backoff_ms": delay_ms
                })),
                true,
            )),
        });

        // Wait for backoff delay
        thread::sleep(Duration::from_millis(delay_ms));

        // Attempt restart
        if let Err(e) = self.spawn_process() {
            log::error!("Failed to restart sidecar: {}", e);
            self.handle_crash(); // Recursive call for next attempt
        }
    }

    /// Stop the sidecar process.
    pub fn stop(&self) -> Result<(), String> {
        log::info!("Stopping sidecar");

        self.shutdown_flag.store(true, Ordering::SeqCst);

        {
            let mut inner = self.inner.lock().unwrap();
            inner.state = SidecarState::ShuttingDown;

            if let Some(ref mut child) = inner.child {
                // Try graceful shutdown via stdin close
                if let Some(ref mut stdin) = child.stdin.take() {
                    // Send shutdown command
                    let shutdown_cmd =
                        r#"{"jsonrpc":"2.0","id":"shutdown","method":"system.shutdown"}"#;
                    let _ = writeln!(stdin, "{}", shutdown_cmd);
                    let _ = stdin.flush();
                }

                // Wait briefly for graceful exit
                thread::sleep(Duration::from_millis(500));

                // Force kill if still running
                match child.try_wait() {
                    Ok(None) => {
                        log::warn!("Sidecar did not exit gracefully, killing");
                        let _ = child.kill();
                    }
                    _ => {}
                }

                inner.child = None;
            }

            inner.state = SidecarState::NotStarted;
            inner.restart_count = 0;
        }

        self.emit_status(SidecarStatus {
            state: SidecarState::NotStarted,
            restart_count: 0,
            message: Some("Sidecar stopped".to_string()),
            error: None,
        });

        Ok(())
    }

    /// Manually retry starting the sidecar after failure.
    pub fn retry(&self) -> Result<(), String> {
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.state != SidecarState::Failed {
                return Err("Can only retry when in Failed state".to_string());
            }
            inner.restart_count = 0;
        }

        self.shutdown_flag.store(false, Ordering::SeqCst);
        self.start()
    }

    /// Write a line to the sidecar's stdin.
    pub fn write_line(&self, line: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();

        if inner.state != SidecarState::Running {
            return Err(format!("Sidecar not running (state: {:?})", inner.state));
        }

        if let Some(ref mut child) = inner.child {
            if let Some(ref mut stdin) = child.stdin.as_mut() {
                writeln!(stdin, "{}", line).map_err(|e| format!("Write error: {}", e))?;
                stdin.flush().map_err(|e| format!("Flush error: {}", e))?;
                Ok(())
            } else {
                Err("Stdin not available".to_string())
            }
        } else {
            Err("No child process".to_string())
        }
    }

    /// Read a line from the sidecar's stdout.
    /// Note: This blocks until a line is available.
    pub fn read_line(&self) -> Result<String, String> {
        // We need to extract stdout in a non-blocking way
        // For a real implementation, this would use async I/O
        // For now, this is a simplified synchronous version

        let child_stdout = {
            let mut inner = self.inner.lock().unwrap();
            if let Some(ref mut child) = inner.child {
                child.stdout.take()
            } else {
                None
            }
        };

        if let Some(stdout) = child_stdout {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .map_err(|e| format!("Read error: {}", e))?;

            // Put stdout back (this is a simplification - real impl would not do this)
            // In practice, you'd use async channels
            Ok(line.trim().to_string())
        } else {
            Err("Stdout not available".to_string())
        }
    }

    /// Clone self for use in thread (without cloning app_handle).
    fn clone_for_thread(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            shutdown_flag: Arc::clone(&self.shutdown_flag),
            app_handle: self.app_handle.clone(),
            spawn_mode: self.spawn_mode.clone(),
        }
    }

    /// Perform a self-check by pinging the sidecar.
    /// Returns the sidecar version if successful.
    pub fn self_check(&self) -> Result<String, String> {
        let ping_request = r#"{"jsonrpc":"2.0","id":1,"method":"system.ping","params":{}}"#;

        // Write the ping request
        self.write_line(ping_request)?;

        // Read the response (with timeout handling done by caller)
        let response = self.read_line()?;

        // Parse the response
        let parsed: PingResponse = serde_json::from_str(&response)
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if let Some(error) = parsed.error {
            return Err(format!("Sidecar returned error: {:?}", error));
        }

        if let Some(result) = parsed.result {
            if result.protocol != "v1" {
                return Err(format!(
                    "Protocol mismatch: expected v1, got {}",
                    result.protocol
                ));
            }
            Ok(result.version)
        } else {
            Err("No result in ping response".to_string())
        }
    }
}

impl Default for SidecarManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let manager = SidecarManager::new();
        assert_eq!(manager.get_state(), SidecarState::NotStarted);
    }

    #[test]
    fn test_backoff_delays() {
        assert_eq!(BACKOFF_DELAYS_MS[0], 250);
        assert_eq!(BACKOFF_DELAYS_MS[4], 10000);
    }

    #[test]
    fn test_max_restart_attempts_constant() {
        assert_eq!(MAX_RESTART_ATTEMPTS, 5);
    }

    #[test]
    fn test_backoff_delay_progression() {
        // Verify delays increase exponentially-ish
        assert!(BACKOFF_DELAYS_MS[0] < BACKOFF_DELAYS_MS[1]);
        assert!(BACKOFF_DELAYS_MS[1] < BACKOFF_DELAYS_MS[2]);
        assert!(BACKOFF_DELAYS_MS[2] < BACKOFF_DELAYS_MS[3]);
        assert!(BACKOFF_DELAYS_MS[3] < BACKOFF_DELAYS_MS[4]);
    }

    #[test]
    fn test_backoff_delay_bounds() {
        // First delay should be fast
        assert!(BACKOFF_DELAYS_MS[0] <= 500);
        // Last delay should be substantial
        assert!(BACKOFF_DELAYS_MS[4] >= 5000);
    }

    #[test]
    fn test_initial_status() {
        let manager = SidecarManager::new();
        let status = manager.get_status();
        assert_eq!(status.state, SidecarState::NotStarted);
        assert_eq!(status.restart_count, 0);
        assert!(status.message.is_none());
    }

    #[test]
    fn test_sidecar_state_serialization() {
        let states = [
            SidecarState::NotStarted,
            SidecarState::Starting,
            SidecarState::Running,
            SidecarState::Restarting,
            SidecarState::Failed,
            SidecarState::ShuttingDown,
        ];

        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            assert!(!json.is_empty());
            // Verify snake_case serialization
            assert!(!json.contains("Starting") || json.contains("starting"));
        }
    }

    #[test]
    fn test_sidecar_status_serialization() {
        let status = SidecarStatus {
            state: SidecarState::Running,
            restart_count: 2,
            message: Some("All systems go".to_string()),
            error: None,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"state\":\"running\""));
        assert!(json.contains("\"restart_count\":2"));
        assert!(json.contains("All systems go"));
    }

    #[test]
    fn test_default_implementation() {
        let manager = SidecarManager::default();
        assert_eq!(manager.get_state(), SidecarState::NotStarted);
    }

    #[test]
    fn test_backoff_index_for_restart_count() {
        // Test the backoff delay lookup logic
        for i in 0..=5 {
            let delay = BACKOFF_DELAYS_MS.get(i as usize).copied().unwrap_or(10000);
            assert!(delay >= 250);
            assert!(delay <= 10000);
        }
    }

    #[test]
    fn test_sidecar_binary_name() {
        let name = SidecarManager::get_sidecar_binary_name();
        #[cfg(target_os = "windows")]
        assert!(name.ends_with(".exe"));
        #[cfg(not(target_os = "windows"))]
        assert!(!name.ends_with(".exe"));
        assert!(name.contains("openvoicy-sidecar"));
    }

    #[test]
    fn test_spawn_mode_debug_assertions() {
        // In test mode (debug), should use Python mode by default
        let manager = SidecarManager::new();
        #[cfg(debug_assertions)]
        assert!(!manager.is_bundled_mode());
    }

    #[test]
    fn test_set_python_mode() {
        let mut manager = SidecarManager::new();
        manager.set_python_mode("/usr/bin/python3".to_string(), "custom_module".to_string());
        assert!(!manager.is_bundled_mode());
    }

    #[test]
    fn test_set_bundled_mode() {
        let mut manager = SidecarManager::new();
        manager.set_bundled_mode();
        assert!(manager.is_bundled_mode());
    }

    #[test]
    fn test_retry_in_wrong_state() {
        let manager = SidecarManager::new();
        // Should fail when not in Failed state
        let result = manager.retry();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Can only retry when in Failed state"));
    }

    #[test]
    fn test_write_line_not_running() {
        let manager = SidecarManager::new();
        let result = manager.write_line("test");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not running"));
    }

    #[test]
    fn test_all_states_serializable() {
        // Ensure all states can be serialized for frontend communication
        let states = [
            (SidecarState::NotStarted, "not_started"),
            (SidecarState::Starting, "starting"),
            (SidecarState::Running, "running"),
            (SidecarState::Restarting, "restarting"),
            (SidecarState::Failed, "failed"),
            (SidecarState::ShuttingDown, "shutting_down"),
        ];

        for (state, expected) in states {
            let json = serde_json::to_string(&state).unwrap();
            assert!(
                json.contains(expected),
                "State {:?} should serialize to contain '{}', got {}",
                state,
                expected,
                json
            );
        }
    }

    #[test]
    fn test_ping_result_parsing() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"protocol":"v1","version":"0.1.0"}}"#;
        let resp: PingResponse = serde_json::from_str(json).unwrap();
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result.protocol, "v1");
        assert_eq!(result.version, "0.1.0");
    }

    #[test]
    fn test_ping_error_response_parsing() {
        let json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#;
        let resp: PingResponse = serde_json::from_str(json).unwrap();
        assert!(resp.error.is_some());
        assert!(resp.result.is_none());
    }
}
