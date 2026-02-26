//! Transcript history ring buffer.
//!
//! Stores recent transcriptions in memory for quick access via tray menu
//! or UI. Privacy by default: no disk persistence, cleared on app quit.

#![allow(dead_code)] // Module under construction

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;
use thiserror::Error;
use uuid::Uuid;

use crate::history_persistence::HistoryPersistence;

/// Default maximum history size.
const DEFAULT_MAX_SIZE: usize = 100;
const CSV_UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryExportFormat {
    Markdown,
    Csv,
}

impl HistoryExportFormat {
    fn parse(input: &str) -> Result<Self, HistoryExportError> {
        match input.trim().to_ascii_lowercase().as_str() {
            "md" | "markdown" => Ok(Self::Markdown),
            "csv" => Ok(Self::Csv),
            value => Err(HistoryExportError::InvalidFormat {
                value: value.to_string(),
            }),
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Csv => "csv",
        }
    }
}

#[derive(Debug, Error)]
pub enum HistoryExportError {
    #[error("unsupported export format: {value}")]
    InvalidFormat { value: String },
    #[error("failed to resolve export directory")]
    MissingExportDirectory,
    #[error("failed to export history: {0}")]
    Io(#[from] std::io::Error),
}

/// Result of text injection for a transcript entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    /// Unique identifier for this entry.
    pub id: Uuid,
    /// The transcribed text.
    pub text: String,
    /// Raw transcript text before optional sidecar post-processing.
    #[serde(default)]
    pub raw_text: String,
    /// Final transcript text after sidecar post-processing.
    #[serde(default)]
    pub final_text: String,
    /// When the transcription was created.
    pub timestamp: DateTime<Utc>,
    /// Duration of the audio recording in milliseconds.
    pub audio_duration_ms: u32,
    /// Time taken to transcribe in milliseconds.
    pub transcription_duration_ms: u32,
    /// Recording session ID correlated to the sidecar session, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    /// Optional detected language code (for example, "en").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Optional confidence score in [0.0, 1.0].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
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
        let raw_text = text.clone();
        let final_text = text.clone();
        Self {
            id: Uuid::new_v4(),
            text,
            raw_text,
            final_text,
            timestamp: Utc::now(),
            audio_duration_ms,
            transcription_duration_ms,
            session_id: None,
            language: None,
            confidence: None,
            injection_result,
            timings: None,
        }
    }

    /// Attach recording session ID.
    pub fn with_session_id(mut self, session_id: Option<Uuid>) -> Self {
        self.session_id = session_id;
        self
    }

    /// Attach optional language and confidence metadata.
    pub fn with_asr_metadata(mut self, language: Option<String>, confidence: Option<f32>) -> Self {
        self.language = language;
        self.confidence = confidence;
        self
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
    persistence: Option<Box<dyn HistoryPersistence>>,
}

impl Default for TranscriptHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl TranscriptHistory {
    /// Create a new history with default max size (100).
    pub fn new() -> Self {
        Self::with_capacity_and_persistence(DEFAULT_MAX_SIZE, None)
    }

    /// Create a new history with a specific max size.
    pub fn with_capacity(max_size: usize) -> Self {
        Self::with_capacity_and_persistence(max_size, None)
    }

    /// Create a new history with a specific max size and persistence backend.
    pub fn with_capacity_and_persistence(
        max_size: usize,
        persistence: Option<Box<dyn HistoryPersistence>>,
    ) -> Self {
        let max_size = max_size.max(1);
        let mut loaded_entries = persistence
            .as_ref()
            .map(|storage| match storage.load() {
                Ok(entries) => entries,
                Err(error) => {
                    log::warn!("Failed to load transcript history from persistence: {error}");
                    Vec::new()
                }
            })
            .unwrap_or_default();

        if loaded_entries.len() > max_size {
            loaded_entries = loaded_entries.split_off(loaded_entries.len() - max_size);
        }

        let mut entries = VecDeque::with_capacity(max_size);
        for entry in loaded_entries {
            entries.push_back(entry);
        }

        Self {
            entries: RwLock::new(entries),
            max_size: AtomicUsize::new(max_size),
            persistence,
        }
    }

    fn persist_snapshot(&self, snapshot: &[TranscriptEntry]) {
        if let Some(persistence) = self.persistence.as_ref() {
            if let Err(error) = persistence.save(snapshot) {
                log::warn!("Failed to persist transcript history snapshot: {error}");
            }
        }
    }

    fn purge_persistence(&self) {
        if let Some(persistence) = self.persistence.as_ref() {
            if let Err(error) = persistence.purge() {
                log::warn!("Failed to purge transcript history persistence: {error}");
            }
        }
    }

    /// Add a transcript entry to the history.
    ///
    /// If the history is full, the oldest entry is removed.
    pub fn push(&self, entry: TranscriptEntry) {
        let snapshot = {
            let mut entries = self.entries.write().unwrap();
            let max_size = self.max_size.load(Ordering::Relaxed);
            if entries.len() >= max_size {
                entries.pop_front();
            }
            entries.push_back(entry);
            entries.iter().cloned().collect::<Vec<_>>()
        };
        self.persist_snapshot(&snapshot);
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

        let (removed, snapshot) = {
            let mut entries = self.entries.write().unwrap();
            let before = entries.len();
            while entries.len() > new_max_size {
                entries.pop_front();
            }
            let removed = before.saturating_sub(entries.len());
            let snapshot = entries.iter().cloned().collect::<Vec<_>>();
            (removed, snapshot)
        };
        log::info!(
            "Resized transcript history max entries: {} -> {} (removed {} old entries)",
            previous_max,
            new_max_size,
            removed
        );
        self.persist_snapshot(&snapshot);
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
        {
            let mut entries = self.entries.write().unwrap();
            entries.clear();
        }
        self.purge_persistence();
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

    /// Export transcript history to a user-accessible location.
    pub fn export(&self, format: &str) -> Result<PathBuf, HistoryExportError> {
        let export_format = HistoryExportFormat::parse(format)?;
        let export_dir = resolve_export_directory()?;
        self.export_to_dir(export_format, &export_dir)
    }

    fn export_to_dir(
        &self,
        export_format: HistoryExportFormat,
        export_dir: &Path,
    ) -> Result<PathBuf, HistoryExportError> {
        fs::create_dir_all(export_dir)?;

        let filename = format!(
            "openvoicy-history-{}.{}",
            Utc::now().format("%Y%m%d-%H%M%S-%3f"),
            export_format.extension()
        );
        let output_path = export_dir.join(filename);

        let entries = self.all();
        match export_format {
            HistoryExportFormat::Markdown => {
                fs::write(&output_path, render_markdown_export(&entries))?;
            }
            HistoryExportFormat::Csv => {
                let mut bytes = Vec::new();
                bytes.extend_from_slice(CSV_UTF8_BOM);
                bytes.extend_from_slice(render_csv_export(&entries).as_bytes());
                fs::write(&output_path, bytes)?;
            }
        }

        Ok(output_path)
    }
}

fn resolve_export_directory() -> Result<PathBuf, HistoryExportError> {
    dirs::download_dir()
        .or_else(dirs::document_dir)
        .or_else(dirs::home_dir)
        .ok_or(HistoryExportError::MissingExportDirectory)
}

fn render_markdown_export(entries: &[TranscriptEntry]) -> String {
    let mut out = String::from("# OpenVoicy Transcript History Export\n\n");
    out.push_str(&format!("Generated (UTC): {}\n\n", Utc::now().to_rfc3339()));

    if entries.is_empty() {
        out.push_str("_No transcript entries available._\n");
        return out;
    }

    for (index, entry) in entries.iter().rev().enumerate() {
        out.push_str(&format!("## Entry {}\n", index + 1));
        out.push_str(&format!("- ID: `{}`\n", entry.id));
        out.push_str(&format!(
            "- Timestamp (UTC): `{}`\n",
            entry.timestamp.to_rfc3339()
        ));
        out.push_str(&format!(
            "- Session ID: `{}`\n",
            entry
                .session_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        ));
        out.push_str(&format!(
            "- Audio Duration (ms): `{}`\n",
            entry.audio_duration_ms
        ));
        out.push_str(&format!(
            "- Transcription Duration (ms): `{}`\n",
            entry.transcription_duration_ms
        ));
        out.push_str(&format!(
            "- Language: `{}`\n",
            entry.language.as_deref().unwrap_or("n/a")
        ));
        out.push_str(&format!(
            "- Confidence: `{}`\n",
            entry
                .confidence
                .map(|value| format!("{value:.3}"))
                .unwrap_or_else(|| "n/a".to_string())
        ));
        out.push_str(&format!(
            "- Injection Result: `{}`\n\n",
            injection_status_label(&entry.injection_result)
        ));

        out.push_str("### Raw Text\n");
        out.push_str("```\n");
        out.push_str(if entry.raw_text.is_empty() {
            entry.text.as_str()
        } else {
            entry.raw_text.as_str()
        });
        out.push_str("\n```\n\n");

        out.push_str("### Final Text\n");
        out.push_str("```\n");
        out.push_str(if entry.final_text.is_empty() {
            entry.text.as_str()
        } else {
            entry.final_text.as_str()
        });
        out.push_str("\n```\n\n");
    }

    out
}

fn render_csv_export(entries: &[TranscriptEntry]) -> String {
    let mut csv = String::from(
        "id,timestamp,session_id,text,raw_text,final_text,audio_duration_ms,transcription_duration_ms,language,confidence,injection_status,injection_reason,injection_error,ipc_ms,transcribe_ms,postprocess_ms,inject_ms,total_ms\n",
    );

    for entry in entries.iter().rev() {
        let (injection_reason, injection_error) = match &entry.injection_result {
            HistoryInjectionResult::ClipboardOnly { reason } => (reason.as_str(), ""),
            HistoryInjectionResult::Error { message } => ("", message.as_str()),
            HistoryInjectionResult::Injected => ("", ""),
        };

        let timings = entry.timings.as_ref();
        let fields = [
            entry.id.to_string(),
            entry.timestamp.to_rfc3339(),
            entry
                .session_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
            entry.text.clone(),
            entry.raw_text.clone(),
            entry.final_text.clone(),
            entry.audio_duration_ms.to_string(),
            entry.transcription_duration_ms.to_string(),
            entry.language.clone().unwrap_or_default(),
            entry
                .confidence
                .map(|value| format!("{value:.6}"))
                .unwrap_or_default(),
            injection_status_label(&entry.injection_result).to_string(),
            injection_reason.to_string(),
            injection_error.to_string(),
            timings
                .and_then(|value| value.ipc_ms)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            timings
                .and_then(|value| value.transcribe_ms)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            timings
                .and_then(|value| value.postprocess_ms)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            timings
                .and_then(|value| value.inject_ms)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            timings
                .and_then(|value| value.total_ms)
                .map(|v| v.to_string())
                .unwrap_or_default(),
        ];

        let escaped = fields
            .iter()
            .map(|value| csv_escape(value))
            .collect::<Vec<_>>()
            .join(",");
        csv.push_str(&escaped);
        csv.push('\n');
    }

    csv
}

fn injection_status_label(result: &HistoryInjectionResult) -> &'static str {
    match result {
        HistoryInjectionResult::Injected => "injected",
        HistoryInjectionResult::ClipboardOnly { .. } => "clipboard_only",
        HistoryInjectionResult::Error { .. } => "error",
    }
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history_persistence::{HistoryPersistence, PersistenceError};
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    struct MockPersistenceCore {
        loaded_entries: Vec<TranscriptEntry>,
        saved_snapshots: Mutex<Vec<Vec<TranscriptEntry>>>,
        purge_calls: AtomicUsize,
    }

    impl MockPersistenceCore {
        fn with_loaded_entries(loaded_entries: Vec<TranscriptEntry>) -> Self {
            Self {
                loaded_entries,
                saved_snapshots: Mutex::new(Vec::new()),
                purge_calls: AtomicUsize::new(0),
            }
        }
    }

    struct SharedMockPersistence(Arc<MockPersistenceCore>);

    impl HistoryPersistence for SharedMockPersistence {
        fn save(&self, entries: &[TranscriptEntry]) -> Result<(), PersistenceError> {
            let mut snapshots = self.0.saved_snapshots.lock().unwrap();
            snapshots.push(entries.to_vec());
            Ok(())
        }

        fn load(&self) -> Result<Vec<TranscriptEntry>, PersistenceError> {
            Ok(self.0.loaded_entries.clone())
        }

        fn purge(&self) -> Result<(), PersistenceError> {
            self.0.purge_calls.fetch_add(1, AtomicOrdering::Relaxed);
            Ok(())
        }

        fn entry_count(&self) -> Result<usize, PersistenceError> {
            Ok(self.0.loaded_entries.len())
        }
    }

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
        assert_eq!(retrieved.raw_text, "Hello, world!");
        assert_eq!(retrieved.final_text, "Hello, world!");
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
        assert!(json.contains("\"raw_text\":\"Test text\""));
        assert!(json.contains("\"final_text\":\"Test text\""));
        assert!(json.contains("\"audio_duration_ms\":2000"));
        assert!(json.contains("\"transcription_duration_ms\":350"));
    }

    #[test]
    fn test_entry_deserialization_defaults_session_id_to_none() {
        let value = serde_json::json!({
            "id": Uuid::new_v4(),
            "text": "legacy entry",
            "timestamp": Utc::now().to_rfc3339(),
            "audio_duration_ms": 1000,
            "transcription_duration_ms": 250,
            "injection_result": { "status": "injected" }
        });

        let entry: TranscriptEntry =
            serde_json::from_value(value).expect("legacy entry should deserialize");
        assert!(entry.session_id.is_none());
        assert!(entry.language.is_none());
        assert!(entry.confidence.is_none());
        assert!(entry.raw_text.is_empty());
        assert!(entry.final_text.is_empty());
    }

    #[test]
    fn test_entry_with_session_id_serialization() {
        let session_id = Uuid::new_v4();
        let entry = TranscriptEntry::new(
            "Test text".to_string(),
            2000,
            350,
            HistoryInjectionResult::Injected,
        )
        .with_session_id(Some(session_id));

        let value = serde_json::to_value(&entry).unwrap();
        let expected = session_id.to_string();
        assert_eq!(
            value.get("session_id").and_then(serde_json::Value::as_str),
            Some(expected.as_str())
        );
    }

    #[test]
    fn test_entry_with_asr_metadata_serialization() {
        let entry = TranscriptEntry::new(
            "Test text".to_string(),
            2000,
            350,
            HistoryInjectionResult::Injected,
        )
        .with_asr_metadata(Some("en".to_string()), Some(0.93));

        let value = serde_json::to_value(&entry).unwrap();
        assert_eq!(
            value.get("language").and_then(serde_json::Value::as_str),
            Some("en")
        );
        let confidence = value
            .get("confidence")
            .and_then(serde_json::Value::as_f64)
            .expect("confidence should serialize");
        assert!((confidence - 0.93_f64).abs() < 1e-6_f64);
    }

    #[test]
    fn test_entry_serialization_omits_optional_metadata_when_absent() {
        let entry = TranscriptEntry::new(
            "Test text".to_string(),
            2000,
            350,
            HistoryInjectionResult::Injected,
        );

        let value = serde_json::to_value(&entry).unwrap();
        assert!(value.get("session_id").is_none());
        assert!(value.get("language").is_none());
        assert!(value.get("confidence").is_none());
        assert!(value.get("timings").is_none());
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
    fn test_export_markdown_contains_session_and_text_sections() {
        let history = TranscriptHistory::new();
        let entry = TranscriptEntry::new(
            "raw text".to_string(),
            2300,
            480,
            HistoryInjectionResult::Injected,
        )
        .with_session_id(Some(Uuid::new_v4()))
        .with_asr_metadata(Some("en".to_string()), Some(0.98));
        history.push(entry);

        let dir = tempdir().expect("temp dir should be available");
        let output = history
            .export_to_dir(HistoryExportFormat::Markdown, dir.path())
            .expect("markdown export should succeed");

        let content = fs::read_to_string(output).expect("markdown file should be readable");
        assert!(content.contains("# OpenVoicy Transcript History Export"));
        assert!(content.contains("Session ID"));
        assert!(content.contains("### Raw Text"));
        assert!(content.contains("### Final Text"));
    }

    #[test]
    fn test_export_csv_empty_history_writes_bom_and_headers() {
        let history = TranscriptHistory::new();
        let dir = tempdir().expect("temp dir should be available");

        let output = history
            .export_to_dir(HistoryExportFormat::Csv, dir.path())
            .expect("csv export should succeed");
        let bytes = fs::read(output).expect("csv file should be readable");

        assert!(bytes.starts_with(CSV_UTF8_BOM));
        let content = String::from_utf8(bytes[CSV_UTF8_BOM.len()..].to_vec())
            .expect("csv content should be utf-8");
        assert!(content.starts_with("id,timestamp,session_id,text,raw_text,final_text"));
        assert_eq!(content.lines().count(), 1);
    }

    #[test]
    fn test_export_csv_escapes_special_characters() {
        let history = TranscriptHistory::new();
        let mut entry = TranscriptEntry::new(
            "hello, \"world\"\nline2".to_string(),
            1000,
            200,
            HistoryInjectionResult::ClipboardOnly {
                reason: "focus,changed".to_string(),
            },
        );
        entry.raw_text = "raw,value".to_string();
        entry.final_text = "final\"value".to_string();
        history.push(entry);

        let dir = tempdir().expect("temp dir should be available");
        let output = history
            .export_to_dir(HistoryExportFormat::Csv, dir.path())
            .expect("csv export should succeed");
        let bytes = fs::read(output).expect("csv file should be readable");
        let content = String::from_utf8(bytes[CSV_UTF8_BOM.len()..].to_vec())
            .expect("csv content should be utf-8");

        assert!(content.contains("\"hello, \"\"world\"\"\nline2\""));
        assert!(content.contains("\"raw,value\""));
        assert!(content.contains("\"final\"\"value\""));
        assert!(content.contains("\"focus,changed\""));
    }

    #[test]
    fn test_export_rejects_invalid_format() {
        let history = TranscriptHistory::new();
        let error = history
            .export("json")
            .expect_err("invalid format should fail");
        assert!(matches!(error, HistoryExportError::InvalidFormat { .. }));
    }

    #[test]
    fn test_thread_safety() {
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

    #[test]
    fn test_history_loads_entries_from_persistence_on_startup() {
        let loaded = vec![
            TranscriptEntry::new(
                "oldest".to_string(),
                1000,
                100,
                HistoryInjectionResult::Injected,
            ),
            TranscriptEntry::new(
                "middle".to_string(),
                1000,
                100,
                HistoryInjectionResult::Injected,
            ),
            TranscriptEntry::new(
                "newest".to_string(),
                1000,
                100,
                HistoryInjectionResult::Injected,
            ),
        ];

        let persistence = Arc::new(MockPersistenceCore::with_loaded_entries(loaded));
        let history = TranscriptHistory::with_capacity_and_persistence(
            2,
            Some(Box::new(SharedMockPersistence(Arc::clone(&persistence)))),
        );

        let entries = history.all();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "newest");
        assert_eq!(entries[1].text, "middle");
    }

    #[test]
    fn test_history_push_persists_snapshot() {
        let persistence = Arc::new(MockPersistenceCore::with_loaded_entries(Vec::new()));
        let history = TranscriptHistory::with_capacity_and_persistence(
            3,
            Some(Box::new(SharedMockPersistence(Arc::clone(&persistence)))),
        );

        history.push(TranscriptEntry::new(
            "first".to_string(),
            1000,
            100,
            HistoryInjectionResult::Injected,
        ));
        history.push(TranscriptEntry::new(
            "second".to_string(),
            1000,
            100,
            HistoryInjectionResult::Injected,
        ));

        let snapshots = persistence.saved_snapshots.lock().unwrap();
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].len(), 1);
        assert_eq!(snapshots[1].len(), 2);
        assert_eq!(snapshots[1][0].text, "first");
        assert_eq!(snapshots[1][1].text, "second");
    }

    #[test]
    fn test_history_clear_purges_persistence() {
        let persistence = Arc::new(MockPersistenceCore::with_loaded_entries(Vec::new()));
        let history = TranscriptHistory::with_capacity_and_persistence(
            3,
            Some(Box::new(SharedMockPersistence(Arc::clone(&persistence)))),
        );
        history.push(TranscriptEntry::new(
            "entry".to_string(),
            1000,
            100,
            HistoryInjectionResult::Injected,
        ));

        history.clear();
        assert_eq!(history.len(), 0);
        assert_eq!(persistence.purge_calls.load(AtomicOrdering::Relaxed), 1);
    }
}
