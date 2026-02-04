//! Sidecar process lifecycle management.
//!
//! This module handles spawning, monitoring, and restarting the Python
//! sidecar process that performs ASR transcription.

#![allow(dead_code)] // Methods will be used in future RPC client implementation

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

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
}

/// Internal sidecar state
struct SidecarInner {
    state: SidecarState,
    restart_count: u32,
    child: Option<Child>,
    last_error: Option<String>,
}

/// Sidecar manager for process lifecycle management.
pub struct SidecarManager {
    inner: Arc<Mutex<SidecarInner>>,
    shutdown_flag: Arc<AtomicBool>,
    app_handle: Option<AppHandle>,
    python_path: String,
    sidecar_module: String,
}

impl SidecarManager {
    /// Create a new sidecar manager.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SidecarInner {
                state: SidecarState::NotStarted,
                restart_count: 0,
                child: None,
                last_error: None,
            })),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            app_handle: None,
            python_path: "python3".to_string(),
            sidecar_module: "openvoicy_sidecar".to_string(),
        }
    }

    /// Set the Tauri app handle for emitting events.
    pub fn set_app_handle(&mut self, handle: AppHandle) {
        self.app_handle = Some(handle);
    }

    /// Set the Python executable path.
    pub fn set_python_path(&mut self, path: String) {
        self.python_path = path;
    }

    /// Set the sidecar module path.
    pub fn set_sidecar_module(&mut self, module: String) {
        self.sidecar_module = module;
    }

    /// Get the current sidecar state.
    pub fn get_state(&self) -> SidecarState {
        self.inner.lock().unwrap().state
    }

    /// Get the current status.
    pub fn get_status(&self) -> SidecarStatus {
        let inner = self.inner.lock().unwrap();
        SidecarStatus {
            state: inner.state,
            restart_count: inner.restart_count,
            message: inner.last_error.clone(),
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
        });

        self.spawn_process()
    }

    /// Spawn the sidecar process.
    fn spawn_process(&self) -> Result<(), String> {
        log::info!("Spawning sidecar process");

        let child = Command::new(&self.python_path)
            .arg("-m")
            .arg(&self.sidecar_module)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn sidecar: {}", e))?;

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
        });

        // Start monitoring thread
        self.start_monitor_thread();

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
            python_path: self.python_path.clone(),
            sidecar_module: self.sidecar_module.clone(),
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
}
