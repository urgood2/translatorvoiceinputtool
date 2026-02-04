//! JSON-RPC 2.0 client for sidecar communication.
//!
//! This module provides an async RPC client that handles:
//! - Request/response correlation
//! - Per-method timeouts
//! - Notification broadcasting
//! - Line buffering and oversized line detection

#![allow(dead_code)] // Client will be used when integrated with SidecarManager

pub mod types;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio::time::timeout;

pub use types::*;

/// Maximum line length (1 MiB). Lines exceeding this cause a fatal error.
const MAX_LINE_LENGTH: usize = 1024 * 1024;

/// RPC client errors.
#[derive(Debug, Error)]
pub enum RpcError {
    #[error("Timeout waiting for response to {method}")]
    Timeout { method: String },

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Remote error: {kind} - {message}")]
    Remote {
        code: i32,
        message: String,
        kind: String,
    },

    #[error("Disconnected from sidecar")]
    Disconnected,

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Channel error: {0}")]
    Channel(String),
}

/// Notification event from the sidecar.
#[derive(Debug, Clone)]
pub struct NotificationEvent {
    pub method: String,
    pub params: Value,
}

/// Internal command for the writer task.
enum WriterCommand {
    Send(String),
    Shutdown,
}

/// Pending request waiting for a response.
struct PendingRequest {
    sender: oneshot::Sender<Result<Response, RpcError>>,
}

/// RPC client for communicating with the sidecar.
pub struct RpcClient {
    /// Counter for generating request IDs.
    next_id: AtomicU64,

    /// Channel for sending messages to the writer task.
    writer_tx: mpsc::Sender<WriterCommand>,

    /// Pending requests waiting for responses.
    pending: Arc<Mutex<HashMap<u64, PendingRequest>>>,

    /// Broadcast channel for notifications.
    notification_tx: broadcast::Sender<NotificationEvent>,

    /// Flag indicating if the client is connected.
    connected: Arc<std::sync::atomic::AtomicBool>,
}

impl RpcClient {
    /// Create a new RPC client connected to the given stdin/stdout.
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        let (writer_tx, writer_rx) = mpsc::channel::<WriterCommand>(32);
        let (notification_tx, _) = broadcast::channel::<NotificationEvent>(64);

        let pending: Arc<Mutex<HashMap<u64, PendingRequest>>> = Arc::new(Mutex::new(HashMap::new()));
        let connected = Arc::new(std::sync::atomic::AtomicBool::new(true));

        // Start writer task
        let stdin = Arc::new(std::sync::Mutex::new(stdin));
        let stdin_clone = Arc::clone(&stdin);
        let connected_clone = Arc::clone(&connected);
        std::thread::spawn(move || {
            Self::writer_loop(stdin_clone, writer_rx, connected_clone);
        });

        // Start reader task
        let pending_clone = Arc::clone(&pending);
        let notification_tx_clone = notification_tx.clone();
        let connected_clone = Arc::clone(&connected);
        std::thread::spawn(move || {
            Self::reader_loop(stdout, pending_clone, notification_tx_clone, connected_clone);
        });

        Self {
            next_id: AtomicU64::new(1),
            writer_tx,
            pending,
            notification_tx,
            connected,
        }
    }

    /// Check if the client is connected.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Subscribe to notifications.
    pub fn subscribe(&self) -> broadcast::Receiver<NotificationEvent> {
        self.notification_tx.subscribe()
    }

    /// Call an RPC method and wait for the response.
    pub async fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<T, RpcError> {
        if !self.is_connected() {
            return Err(RpcError::Disconnected);
        }

        // Generate request ID
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        // Create request
        let request = Request::new(id, method, params);
        let request_json = serde_json::to_string(&request)?;

        // Create response channel
        let (tx, rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, PendingRequest { sender: tx });
        }

        // Send request
        self.writer_tx
            .send(WriterCommand::Send(request_json))
            .await
            .map_err(|_| RpcError::Disconnected)?;

        // Wait for response with timeout
        let method_timeout = TimeoutConfig::get(method);
        let response = match timeout(method_timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                // Channel was closed
                self.cleanup_pending(id).await;
                return Err(RpcError::Disconnected);
            }
            Err(_) => {
                // Timeout
                self.cleanup_pending(id).await;
                return Err(RpcError::Timeout {
                    method: method.to_string(),
                });
            }
        }?;

        // Check for error
        if let Some(err) = response.error {
            return Err(RpcError::Remote {
                code: err.code,
                message: err.message,
                kind: err.data.map(|d| d.kind).unwrap_or_default(),
            });
        }

        // Parse result
        let result = response
            .result
            .ok_or_else(|| RpcError::Protocol("Missing result in response".to_string()))?;

        serde_json::from_value(result).map_err(RpcError::from)
    }

    /// Clean up a pending request.
    async fn cleanup_pending(&self, id: u64) {
        let mut pending = self.pending.lock().await;
        pending.remove(&id);
    }

    /// Writer loop - sends messages to stdin.
    fn writer_loop(
        stdin: Arc<std::sync::Mutex<ChildStdin>>,
        mut rx: mpsc::Receiver<WriterCommand>,
        connected: Arc<std::sync::atomic::AtomicBool>,
    ) {
        while let Some(cmd) = rx.blocking_recv() {
            match cmd {
                WriterCommand::Send(line) => {
                    let mut stdin = stdin.lock().unwrap();
                    if writeln!(stdin, "{}", line).is_err() {
                        log::error!("Failed to write to sidecar stdin");
                        connected.store(false, Ordering::SeqCst);
                        break;
                    }
                    if stdin.flush().is_err() {
                        log::error!("Failed to flush sidecar stdin");
                        connected.store(false, Ordering::SeqCst);
                        break;
                    }
                }
                WriterCommand::Shutdown => {
                    log::info!("Writer loop shutting down");
                    break;
                }
            }
        }
    }

    /// Reader loop - reads responses from stdout.
    fn reader_loop(
        stdout: ChildStdout,
        pending: Arc<Mutex<HashMap<u64, PendingRequest>>>,
        notification_tx: broadcast::Sender<NotificationEvent>,
        connected: Arc<std::sync::atomic::AtomicBool>,
    ) {
        let reader = BufReader::new(stdout);

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    log::error!("Error reading from sidecar: {}", e);
                    connected.store(false, Ordering::SeqCst);
                    break;
                }
            };

            // Check line length
            if line.len() > MAX_LINE_LENGTH {
                log::error!(
                    "Line exceeds maximum length ({} > {}), fatal",
                    line.len(),
                    MAX_LINE_LENGTH
                );
                connected.store(false, Ordering::SeqCst);
                break;
            }

            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Parse message
            let message: IncomingMessage = match serde_json::from_str(&line) {
                Ok(m) => m,
                Err(e) => {
                    log::warn!("Failed to parse message from sidecar: {}", e);
                    continue;
                }
            };

            match message {
                IncomingMessage::Response(response) => {
                    if let Some(RequestId::Number(id)) = response.id {
                        // Correlate with pending request
                        let mut pending_guard =
                            tokio::runtime::Handle::current().block_on(pending.lock());
                        if let Some(request) = pending_guard.remove(&id) {
                            let _ = request.sender.send(Ok(response));
                        } else {
                            log::warn!("Received response for unknown request id: {}", id);
                        }
                    }
                }
                IncomingMessage::Notification(notif) => {
                    // Broadcast notification
                    let event = NotificationEvent {
                        method: notif.method,
                        params: notif.params,
                    };
                    let _ = notification_tx.send(event);
                }
            }
        }

        log::info!("Reader loop ended");
        connected.store(false, Ordering::SeqCst);

        // Notify all pending requests that we're disconnected
        let mut pending_guard = tokio::runtime::Handle::current().block_on(pending.lock());
        for (_, request) in pending_guard.drain() {
            let _ = request.sender.send(Err(RpcError::Disconnected));
        }
    }

    /// Shutdown the client.
    pub async fn shutdown(&self) {
        let _ = self.writer_tx.send(WriterCommand::Shutdown).await;
        self.connected.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_error_display() {
        let err = RpcError::Timeout {
            method: "test".to_string(),
        };
        assert!(err.to_string().contains("Timeout"));

        let err = RpcError::Remote {
            code: -32601,
            message: "Method not found".to_string(),
            kind: "E_METHOD_NOT_FOUND".to_string(),
        };
        assert!(err.to_string().contains("E_METHOD_NOT_FOUND"));
    }
}
