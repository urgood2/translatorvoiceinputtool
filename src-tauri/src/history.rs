//! Transcript history ring buffer.
//!
//! Stores recent transcriptions in memory for quick access via tray menu
//! or UI. Privacy by default: no disk persistence, cleared on app quit.

#![allow(dead_code)] // Module under construction

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;
use uuid::Uuid;

/// Default maximum history size.
const DEFAULT_MAX_SIZE: usize = 100;

/// Result of text injection for a transcript entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum HistoryInjectionResult {
    /// Text was successfully injected via paste.
    Injected,
    /// Text copied to clipboard only (paste not performed).
    ClipboardOnly { reason: String },
    /// Injection failed with an error.
    Error { message: String },
}

impl HistoryInjectionResult {
    /// Create from injection module result.
    pub fn from_injection_result(result: &crate::injection::InjectionResult) -> Self {
        match result {
            crate::injection::InjectionResult::Injected { .. } => HistoryInjectionResult::Injected,
            crate::injection::InjectionResult::ClipboardOnly { reason, .. } => {
                HistoryInjectionResult::ClipboardOnly {
                    reason: reason.clone(),
                }
            }
            crate::injection::InjectionResult::Failed { error, .. } => {
                HistoryInjectionResult::Error {
                    message: error.clone(),
                }
            }
        }
    }
}

/// Timing breakdown for the stop -> injection pipeline.
#[derive(Debug, Clone, Serialize)]
pub struct TranscriptTimings {
    /// Time from stop request until recording.stop RPC returns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipc_ms: Option<u64>,
    /// Time from stop RPC return until transcription_complete is received.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcribe_ms: Option<u64>,
    /// Time spent in host-side post-processing before injection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postprocess_ms: Option<u64>,
    /// Time spent injecting text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inject_ms: Option<u64>,
    /// End-to-end stop -> injection total.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_ms: Option<u64>,
}

/// A single transcript entry in the history.
#[derive(Debug, Clone, Serialize)]
pub struct TranscriptEntry {
    /// Unique identifier for this entry.
    pub id: Uuid,
    /// The transcribed text.
    pub text: String,
    /// When the transcription was created.
    pub timestamp: DateTime<Utc>,
    /// Duration of the audio recording in milliseconds.
    pub audio_duration_ms: u32,
    /// Time taken to transcribe in milliseconds.
    pub transcription_duration_ms: u32,
    /// Result of injection attempt.
    pub injection_result: HistoryInjectionResult,
    /// Optional stop -> injection timing breakdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timings: Option<TranscriptTimings>,
}

impl TranscriptEntry {
    /// Create a new transcript entry.
    pub fn new(
        text: String,
        audio_duration_ms: u32,
        transcription_duration_ms: u32,
        injection_result: HistoryInjectionResult,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            text,
            timestamp: Utc::now(),
            audio_duration_ms,
            transcription_duration_ms,
            injection_result,
            timings: None,
        }
    }

    /// Attach pipeline timings.
    pub fn with_timings(mut self, timings: TranscriptTimings) -> Self {
        self.timings = Some(timings);
        self
    }
}

/// Thread-safe transcript history ring buffer.
pub struct TranscriptHistory {
    entries: RwLock<VecDeque<TranscriptEntry>>,
    max_size: AtomicUsize,
}

impl Default for TranscriptHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl TranscriptHistory {
    /// Create a new history with default max size (100).
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_SIZE)
    }

    /// Create a new history with a specific max size.
    pub fn with_capacity(max_size: usize) -> Self {
        let max_size = max_size.max(1);
        Self {
            entries: RwLock::new(VecDeque::with_capacity(max_size)),
            max_size: AtomicUsize::new(max_size),
        }
    }

    /// Add a transcript entry to the history.
    ///
    /// If the history is full, the oldest entry is removed.
    pub fn push(&self, entry: TranscriptEntry) {
        let mut entries = self.entries.write().unwrap();
        let max_size = self.max_size.load(Ordering::Relaxed);
        if entries.len() >= max_size {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    /// Resize the maximum retained entries.
    ///
    /// When shrinking, oldest entries are dropped first.
    pub fn resize(&self, new_max_size: usize) {
        let new_max_size = new_max_size.max(1);
        let previous_max = self.max_size.swap(new_max_size, Ordering::Relaxed);
        if previous_max == new_max_size {
            return;
        }

        let mut entries = self.entries.write().unwrap();
        let before = entries.len();
        while entries.len() > new_max_size {
            entries.pop_front();
        }
        let removed = before.saturating_sub(entries.len());
        log::info!(
            "Resized transcript history max entries: {} -> {} (removed {} old entries)",
            previous_max,
            new_max_size,
            removed
        );
    }

    /// Current configured max entries.
    pub fn max_size(&self) -> usize {
        self.max_size.load(Ordering::Relaxed)
    }

    /// Get the most recent transcript entry.
    pub fn last(&self) -> Option<TranscriptEntry> {
        let entries = self.entries.read().unwrap();
        entries.back().cloned()
    }

    /// Get a transcript entry by ID.
    pub fn get(&self, id: Uuid) -> Option<TranscriptEntry> {
        let entries = self.entries.read().unwrap();
        entries.iter().find(|e| e.id == id).cloned()
    }

    /// Get all entries, newest first.
    pub fn all(&self) -> Vec<TranscriptEntry> {
        let entries = self.entries.read().unwrap();
        entries.iter().rev().cloned().collect()
    }

    /// Get the number of entries in the history.
    pub fn len(&self) -> usize {
        let entries = self.entries.read().unwrap();
        entries.len()
    }

    /// Check if the history is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all entries from the history.
    pub fn clear(&self) {
        let mut entries = self.entries.write().unwrap();
        entries.clear();
    }

    /// Get the text of the most recent transcript.
    pub fn last_text(&self) -> Option<String> {
        self.last().map(|e| e.text)
    }

    /// Copy a transcript's text to the clipboard by ID.
    ///
    /// Returns the text that was copied, or None if not found.
    pub fn copy_by_id(&self, id: Uuid) -> Option<String> {
        let entry = self.get(id)?;
        let text = entry.text.clone();

        // Copy to clipboard
        if crate::injection::set_clipboard_public(&text).is_ok() {
            Some(text)
        } else {
            None
        }
    }

    /// Copy the most recent transcript's text to the clipboard.
    ///
    /// Returns the text that was copied, or None if history is empty.
    pub fn copy_last(&self) -> Option<String> {
        let text = self.last_text()?;

        // Copy to clipboard
        if crate::injection::set_clipboard_public(&text).is_ok() {
            Some(text)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_history_is_empty() {
        let history = TranscriptHistory::new();
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
        assert!(history.last().is_none());
    }

    #[test]
    fn test_push_and_retrieve() {
        let history = TranscriptHistory::new();

        let entry = TranscriptEntry::new(
            "Hello, world!".to_string(),
            1000,
            200,
            HistoryInjectionResult::Injected,
        );
        let entry_id = entry.id;

        history.push(entry);

        assert_eq!(history.len(), 1);
        assert!(!history.is_empty());

        let retrieved = history.last().unwrap();
        assert_eq!(retrieved.text, "Hello, world!");
        assert_eq!(retrieved.id, entry_id);
    }

    #[test]
    fn test_ring_buffer_eviction() {
        let history = TranscriptHistory::with_capacity(3);

        // Add 5 entries to a buffer of size 3
        for i in 0..5 {
            let entry = TranscriptEntry::new(
                format!("Entry {}", i),
                1000,
                200,
                HistoryInjectionResult::Injected,
            );
            history.push(entry);
        }

        // Should only have 3 entries
        assert_eq!(history.len(), 3);

        // Should have entries 2, 3, 4 (0 and 1 were evicted)
        let all = history.all();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].text, "Entry 4"); // Newest first
        assert_eq!(all[1].text, "Entry 3");
        assert_eq!(all[2].text, "Entry 2");
    }

    #[test]
    fn test_default_capacity_matches_config_default() {
        let history = TranscriptHistory::new();
        assert_eq!(history.max_size(), 100);
    }

    #[test]
    fn test_resize_drops_oldest_entries() {
        let history = TranscriptHistory::with_capacity(5);

        for i in 0..5 {
            let entry = TranscriptEntry::new(
                format!("Entry {}", i),
                1000,
                200,
                HistoryInjectionResult::Injected,
            );
            history.push(entry);
        }

        history.resize(3);
        assert_eq!(history.max_size(), 3);
        assert_eq!(history.len(), 3);

        let all = history.all();
        assert_eq!(all[0].text, "Entry 4");
        assert_eq!(all[1].text, "Entry 3");
        assert_eq!(all[2].text, "Entry 2");
    }

    #[test]
    fn test_get_by_id() {
        let history = TranscriptHistory::new();

        let entry1 = TranscriptEntry::new(
            "First".to_string(),
            1000,
            200,
            HistoryInjectionResult::Injected,
        );
        let entry2 = TranscriptEntry::new(
            "Second".to_string(),
            1500,
            250,
            HistoryInjectionResult::ClipboardOnly {
                reason: "Focus changed".to_string(),
            },
        );

        let id1 = entry1.id;
        let id2 = entry2.id;

        history.push(entry1);
        history.push(entry2);

        let retrieved1 = history.get(id1).unwrap();
        assert_eq!(retrieved1.text, "First");

        let retrieved2 = history.get(id2).unwrap();
        assert_eq!(retrieved2.text, "Second");

        // Non-existent ID
        assert!(history.get(Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_all_returns_newest_first() {
        let history = TranscriptHistory::new();

        for i in 0..5 {
            let entry = TranscriptEntry::new(
                format!("Entry {}", i),
                1000,
                200,
                HistoryInjectionResult::Injected,
            );
            history.push(entry);
        }

        let all = history.all();
        assert_eq!(all[0].text, "Entry 4");
        assert_eq!(all[4].text, "Entry 0");
    }

    #[test]
    fn test_clear() {
        let history = TranscriptHistory::new();

        for i in 0..3 {
            let entry = TranscriptEntry::new(
                format!("Entry {}", i),
                1000,
                200,
                HistoryInjectionResult::Injected,
            );
            history.push(entry);
        }

        assert_eq!(history.len(), 3);
        history.clear();
        assert_eq!(history.len(), 0);
        assert!(history.is_empty());
    }

    #[test]
    fn test_last_text() {
        let history = TranscriptHistory::new();

        assert!(history.last_text().is_none());

        let entry = TranscriptEntry::new(
            "Hello".to_string(),
            1000,
            200,
            HistoryInjectionResult::Injected,
        );
        history.push(entry);

        assert_eq!(history.last_text(), Some("Hello".to_string()));
    }

    #[test]
    fn test_injection_result_serialization() {
        let injected = HistoryInjectionResult::Injected;
        let json = serde_json::to_string(&injected).unwrap();
        assert!(json.contains("\"status\":\"injected\""));

        let clipboard_only = HistoryInjectionResult::ClipboardOnly {
            reason: "Focus changed".to_string(),
        };
        let json = serde_json::to_string(&clipboard_only).unwrap();
        assert!(json.contains("\"status\":\"clipboard_only\""));
        assert!(json.contains("Focus changed"));

        let error = HistoryInjectionResult::Error {
            message: "Clipboard error".to_string(),
        };
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"status\":\"error\""));
        assert!(json.contains("Clipboard error"));
    }

    #[test]
    fn test_entry_serialization() {
        let entry = TranscriptEntry::new(
            "Test text".to_string(),
            2000,
            350,
            HistoryInjectionResult::Injected,
        );

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"text\":\"Test text\""));
        assert!(json.contains("\"audio_duration_ms\":2000"));
        assert!(json.contains("\"transcription_duration_ms\":350"));
    }

    #[test]
    fn test_entry_timings_serialization() {
        let entry = TranscriptEntry::new(
            "Test text".to_string(),
            2000,
            350,
            HistoryInjectionResult::Injected,
        )
        .with_timings(TranscriptTimings {
            ipc_ms: Some(15),
            transcribe_ms: Some(780),
            postprocess_ms: Some(5),
            inject_ms: Some(50),
            total_ms: Some(850),
        });

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"timings\""));
        assert!(json.contains("\"ipc_ms\":15"));
        assert!(json.contains("\"total_ms\":850"));
    }

    #[test]
    fn test_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let history = Arc::new(TranscriptHistory::with_capacity(100));

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let history = Arc::clone(&history);
                thread::spawn(move || {
                    for j in 0..10 {
                        let entry = TranscriptEntry::new(
                            format!("Thread {} Entry {}", i, j),
                            1000,
                            200,
                            HistoryInjectionResult::Injected,
                        );
                        history.push(entry);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // All 100 entries should have been added
        assert_eq!(history.len(), 100);
    }
}
