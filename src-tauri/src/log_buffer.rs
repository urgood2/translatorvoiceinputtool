//! Ring buffer for capturing recent log entries for diagnostics.
//!
//! Provides bounded log storage that can be included in bug reports.
//! Features:
//! - Bounded by line count (default 500) and byte size (default 256 KiB)
//! - Thread-safe access
//! - Automatic redaction of sensitive data (paths, transcripts)
//! - Integration with the `log` crate via custom subscriber

use std::collections::VecDeque;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Local, Utc};
use log::{Level, Log, Metadata, Record};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;

/// Default maximum number of log entries.
const DEFAULT_MAX_ENTRIES: usize = 500;

/// Default maximum total size in bytes.
const DEFAULT_MAX_BYTES: usize = 256 * 1024; // 256 KiB

/// Global log buffer instance.
static LOG_BUFFER: Lazy<Arc<LogRingBuffer>> =
    Lazy::new(|| Arc::new(LogRingBuffer::new(DEFAULT_MAX_ENTRIES, DEFAULT_MAX_BYTES)));

/// Get the global log buffer.
pub fn global_buffer() -> Arc<LogRingBuffer> {
    Arc::clone(&LOG_BUFFER)
}

/// A single log entry.
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    /// Timestamp when the log was recorded.
    pub timestamp: DateTime<Utc>,
    /// Log level (ERROR, WARN, INFO, DEBUG, TRACE).
    pub level: String,
    /// Logger target (usually module path).
    pub target: String,
    /// Log message (redacted if necessary).
    pub message: String,
    /// Approximate byte size of this entry.
    #[serde(skip)]
    byte_size: usize,
}

impl LogEntry {
    /// Create a new log entry with automatic redaction.
    pub fn new(level: Level, target: &str, message: &str) -> Self {
        let redacted_message = redact_sensitive(message);
        let byte_size = target.len() + redacted_message.len() + 50; // Approximate overhead

        Self {
            timestamp: Utc::now(),
            level: level.to_string(),
            target: target.to_string(),
            message: redacted_message,
            byte_size,
        }
    }

    /// Format as a single line for diagnostics output.
    pub fn format_line(&self) -> String {
        let local_time: DateTime<Local> = self.timestamp.into();
        format!(
            "{} [{}] {}: {}",
            local_time.format("%Y-%m-%d %H:%M:%S%.3f"),
            self.level,
            self.target,
            self.message
        )
    }
}

/// Patterns for redacting sensitive data.
static REDACTION_PATTERNS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    vec![
        // User home directories
        (Regex::new(r"/Users/[^/\s]+").unwrap(), "/Users/[REDACTED]"),
        (Regex::new(r"/home/[^/\s]+").unwrap(), "/home/[REDACTED]"),
        (
            Regex::new(r"C:\\Users\\[^\\\s]+").unwrap(),
            "C:\\Users\\[REDACTED]",
        ),
        // Authorization bearer tokens
        (
            Regex::new(r#"(?i)(authorization\s*[:=]\s*bearer\s+)[A-Za-z0-9\-._~+/]+=*"#).unwrap(),
            "$1[REDACTED]",
        ),
        // Env-like credentials (HF_TOKEN=..., SERVICE_SECRET: ..., etc.)
        (
            Regex::new(
                r#"(?i)\b([A-Z0-9_]*(TOKEN|SECRET|PASSWORD|API_KEY|KEY)[A-Z0-9_]*)\b\s*[:=]\s*['"]?[^'"\s]+['"]?"#,
            )
            .unwrap(),
            "$1=[REDACTED]",
        ),
        // API keys and tokens (common patterns)
        (
            Regex::new(
                r#"(?i)\b(api[_-]?key|token|secret|password|credential)\b\s*[:=]\s*['"]?[^'"\s]+['"]?"#,
            )
            .unwrap(),
            "$1=[REDACTED]",
        ),
        // Email addresses
        (
            Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap(),
            "[EMAIL_REDACTED]",
        ),
        // Transcript text (between quotes after "text:" or "transcript:")
        (
            Regex::new(r#"(text|transcript)[=:]\s*["']([^"']{20,})["']"#).unwrap(),
            "$1=\"[TRANSCRIPT_REDACTED]\"",
        ),
        // Long quoted strings that might be transcripts (>50 chars)
        (
            Regex::new(r#""([^"]{50,})""#).unwrap(),
            "\"[LONG_STRING_REDACTED]\"",
        ),
    ]
});

/// Redact sensitive information from a log message.
fn redact_sensitive(message: &str) -> String {
    let mut result = message.to_string();

    for (pattern, replacement) in REDACTION_PATTERNS.iter() {
        result = pattern.replace_all(&result, *replacement).into_owned();
    }

    result
}

/// Thread-safe ring buffer for log entries.
pub struct LogRingBuffer {
    entries: RwLock<VecDeque<LogEntry>>,
    max_entries: usize,
    max_bytes: usize,
    current_bytes: RwLock<usize>,
}

impl LogRingBuffer {
    /// Create a new log buffer with specified limits.
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: RwLock::new(VecDeque::with_capacity(max_entries.min(1000))),
            max_entries,
            max_bytes,
            current_bytes: RwLock::new(0),
        }
    }

    /// Add a log entry, evicting old entries if limits exceeded.
    pub fn push(&self, entry: LogEntry) {
        let entry_size = entry.byte_size;

        let mut entries = self.entries.write().unwrap();
        let mut current_bytes = self.current_bytes.write().unwrap();

        // Evict entries if we exceed limits
        while entries.len() >= self.max_entries
            || (*current_bytes + entry_size > self.max_bytes && !entries.is_empty())
        {
            if let Some(removed) = entries.pop_front() {
                *current_bytes = current_bytes.saturating_sub(removed.byte_size);
            }
        }

        *current_bytes += entry_size;
        entries.push_back(entry);
    }

    /// Get all entries as a vector.
    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries.read().unwrap().iter().cloned().collect()
    }

    /// Get entry count.
    pub fn len(&self) -> usize {
        self.entries.read().unwrap().len()
    }

    /// Check if buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.read().unwrap().is_empty()
    }

    /// Get current byte size.
    pub fn byte_size(&self) -> usize {
        *self.current_bytes.read().unwrap()
    }

    /// Clear all entries.
    pub fn clear(&self) {
        let mut entries = self.entries.write().unwrap();
        let mut current_bytes = self.current_bytes.write().unwrap();
        entries.clear();
        *current_bytes = 0;
    }

    /// Format all entries as a multiline string for diagnostics.
    pub fn format_all(&self) -> String {
        let entries = self.entries.read().unwrap();
        let mut lines: Vec<String> = Vec::with_capacity(entries.len());

        for entry in entries.iter() {
            lines.push(entry.format_line());
        }

        lines.join("\n")
    }

    /// Get stats about the buffer.
    pub fn stats(&self) -> BufferStats {
        let entries = self.entries.read().unwrap();
        let current_bytes = *self.current_bytes.read().unwrap();

        BufferStats {
            entry_count: entries.len(),
            max_entries: self.max_entries,
            byte_size: current_bytes,
            max_bytes: self.max_bytes,
        }
    }
}

/// Statistics about the log buffer.
#[derive(Debug, Clone, Serialize)]
pub struct BufferStats {
    pub entry_count: usize,
    pub max_entries: usize,
    pub byte_size: usize,
    pub max_bytes: usize,
}

/// Logger implementation that captures to the ring buffer.
pub struct BufferLogger {
    buffer: Arc<LogRingBuffer>,
    min_level: Level,
}

impl BufferLogger {
    /// Create a new buffer logger.
    pub fn new(buffer: Arc<LogRingBuffer>, min_level: Level) -> Self {
        Self { buffer, min_level }
    }

    /// Create using the global buffer.
    pub fn global(min_level: Level) -> Self {
        Self::new(global_buffer(), min_level)
    }
}

impl Log for BufferLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.min_level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let entry = LogEntry::new(record.level(), record.target(), &record.args().to_string());
            self.buffer.push(entry);
        }
    }

    fn flush(&self) {
        // No-op for in-memory buffer
    }
}

/// Logger that tees records to a primary logger and the diagnostics buffer.
struct CombinedLogger {
    primary: Box<dyn Log + Send + Sync>,
    buffer: BufferLogger,
}

impl CombinedLogger {
    fn new(primary: Box<dyn Log + Send + Sync>, buffer: BufferLogger) -> Self {
        Self { primary, buffer }
    }
}

impl Log for CombinedLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.primary.enabled(metadata) || self.buffer.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        if self.primary.enabled(record.metadata()) {
            self.primary.log(record);
        }
        if self.buffer.enabled(record.metadata()) {
            self.buffer.log(record);
        }
    }

    fn flush(&self) {
        self.primary.flush();
        self.buffer.flush();
    }
}

/// Initialize the buffer logger alongside the existing logger.
/// Call this early in application startup.
pub fn init_buffer_logger(min_level: Level) {
    let primary_logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .build();
    let combined = CombinedLogger::new(Box::new(primary_logger), BufferLogger::global(min_level));

    if log::set_boxed_logger(Box::new(combined)).is_ok() {
        // Keep this permissive so env_logger and buffer logger can apply their own filters.
        log::set_max_level(log::LevelFilter::Trace);
    }
}

/// Convenience function to log to the global buffer directly.
pub fn log_to_buffer(level: Level, target: &str, message: &str) {
    let entry = LogEntry::new(level, target, message);
    global_buffer().push(entry);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockLogger {
        records: Arc<Mutex<Vec<String>>>,
    }

    impl Log for MockLogger {
        fn enabled(&self, _metadata: &Metadata) -> bool {
            true
        }

        fn log(&self, record: &Record) {
            self.records
                .lock()
                .unwrap()
                .push(format!("{}", record.args()));
        }

        fn flush(&self) {}
    }

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry::new(Level::Info, "test::module", "Hello world");

        assert_eq!(entry.level, "INFO");
        assert_eq!(entry.target, "test::module");
        assert_eq!(entry.message, "Hello world");
        assert!(entry.byte_size > 0);
    }

    #[test]
    fn test_path_redaction() {
        let message = "Loading config from /Users/john/config.json";
        let entry = LogEntry::new(Level::Info, "test", message);

        assert!(entry.message.contains("[REDACTED]"));
        assert!(!entry.message.contains("john"));
    }

    #[test]
    fn test_linux_path_redaction() {
        let message = "Cache at /home/alice/.cache/openvoicy";
        let entry = LogEntry::new(Level::Info, "test", message);

        assert!(entry.message.contains("[REDACTED]"));
        assert!(!entry.message.contains("alice"));
    }

    #[test]
    fn test_windows_path_redaction() {
        let message = "Config at C:\\Users\\Bob\\AppData\\Local";
        let entry = LogEntry::new(Level::Info, "test", message);

        assert!(entry.message.contains("[REDACTED]"));
        assert!(!entry.message.contains("Bob"));
    }

    #[test]
    fn test_email_redaction() {
        let message = "User email: user@example.com logged in";
        let entry = LogEntry::new(Level::Info, "test", message);

        assert!(entry.message.contains("[EMAIL_REDACTED]"));
        assert!(!entry.message.contains("user@example.com"));
    }

    #[test]
    fn test_api_key_redaction() {
        let message = "Using api_key=sk_live_abc123def456";
        let entry = LogEntry::new(Level::Info, "test", message);

        assert!(entry.message.contains("[REDACTED]"));
        assert!(!entry.message.contains("abc123"));
    }

    #[test]
    fn test_hf_token_redaction() {
        let message = "HF_TOKEN=hf_very_secret_token";
        let entry = LogEntry::new(Level::Info, "test", message);

        assert!(entry.message.contains("HF_TOKEN=[REDACTED]"));
        assert!(!entry.message.contains("hf_very_secret_token"));
    }

    #[test]
    fn test_authorization_bearer_redaction() {
        let message = "Authorization: Bearer hf_super_secret_bearer_token";
        let entry = LogEntry::new(Level::Info, "test", message);

        assert!(entry.message.contains("Authorization: Bearer [REDACTED]"));
        assert!(!entry.message.contains("hf_super_secret_bearer_token"));
    }

    #[test]
    fn test_transcript_redaction() {
        let message = r#"Transcription text="The quick brown fox jumps over the lazy dog and more text here""#;
        let entry = LogEntry::new(Level::Info, "test", message);

        assert!(entry.message.contains("[TRANSCRIPT_REDACTED]"));
        assert!(!entry.message.contains("quick brown fox"));
    }

    #[test]
    fn test_buffer_push_and_retrieve() {
        let buffer = LogRingBuffer::new(100, 10000);

        buffer.push(LogEntry::new(Level::Info, "test", "Message 1"));
        buffer.push(LogEntry::new(Level::Warn, "test", "Message 2"));

        assert_eq!(buffer.len(), 2);

        let entries = buffer.entries();
        assert_eq!(entries[0].message, "Message 1");
        assert_eq!(entries[1].message, "Message 2");
    }

    #[test]
    fn test_buffer_eviction_by_count() {
        let buffer = LogRingBuffer::new(3, 1_000_000);

        for i in 0..5 {
            buffer.push(LogEntry::new(
                Level::Info,
                "test",
                &format!("Message {}", i),
            ));
        }

        assert_eq!(buffer.len(), 3);

        let entries = buffer.entries();
        assert_eq!(entries[0].message, "Message 2");
        assert_eq!(entries[1].message, "Message 3");
        assert_eq!(entries[2].message, "Message 4");
    }

    #[test]
    fn test_buffer_eviction_by_size() {
        // Very small byte limit
        let buffer = LogRingBuffer::new(1000, 200);

        // Each entry is ~100 bytes with overhead
        for i in 0..10 {
            buffer.push(LogEntry::new(
                Level::Info,
                "test",
                &format!("Message number {}", i),
            ));
        }

        // Should have evicted some entries
        assert!(buffer.len() < 10);
        assert!(buffer.byte_size() <= 200 + 100); // Allow some slack for last entry
    }

    #[test]
    fn test_buffer_clear() {
        let buffer = LogRingBuffer::new(100, 10000);

        buffer.push(LogEntry::new(Level::Info, "test", "Message"));
        assert_eq!(buffer.len(), 1);

        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.byte_size(), 0);
    }

    #[test]
    fn test_format_all() {
        let buffer = LogRingBuffer::new(100, 10000);

        buffer.push(LogEntry::new(Level::Info, "mod1", "First"));
        buffer.push(LogEntry::new(Level::Error, "mod2", "Second"));

        let formatted = buffer.format_all();

        assert!(formatted.contains("[INFO]"));
        assert!(formatted.contains("[ERROR]"));
        assert!(formatted.contains("mod1"));
        assert!(formatted.contains("mod2"));
        assert!(formatted.contains("First"));
        assert!(formatted.contains("Second"));
    }

    #[test]
    fn test_stats() {
        let buffer = LogRingBuffer::new(500, 256 * 1024);

        buffer.push(LogEntry::new(Level::Info, "test", "Message"));

        let stats = buffer.stats();
        assert_eq!(stats.entry_count, 1);
        assert_eq!(stats.max_entries, 500);
        assert!(stats.byte_size > 0);
        assert_eq!(stats.max_bytes, 256 * 1024);
    }

    #[test]
    fn test_buffer_thread_safety() {
        use std::thread;

        let buffer = Arc::new(LogRingBuffer::new(1000, 100_000));
        let mut handles = vec![];

        for i in 0..10 {
            let buf = Arc::clone(&buffer);
            handles.push(thread::spawn(move || {
                for j in 0..100 {
                    buf.push(LogEntry::new(
                        Level::Info,
                        "thread_test",
                        &format!("Thread {} Message {}", i, j),
                    ));
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Should have captured entries from all threads
        assert!(buffer.len() > 0);
        assert!(buffer.len() <= 1000);
    }

    #[test]
    fn test_global_buffer() {
        let buffer = global_buffer();

        // Push to global buffer
        log_to_buffer(Level::Info, "global_test", "Global message");

        assert!(buffer.len() > 0);
    }

    #[test]
    fn test_combined_logger_forwards_to_primary_and_buffer() {
        let primary_records = Arc::new(Mutex::new(Vec::new()));
        let primary = MockLogger {
            records: Arc::clone(&primary_records),
        };
        let buffer = Arc::new(LogRingBuffer::new(10, 4096));
        let combined =
            CombinedLogger::new(Box::new(primary), BufferLogger::new(Arc::clone(&buffer), Level::Info));

        let record = Record::builder()
            .args(format_args!("combined hello"))
            .level(Level::Info)
            .target("combined::test")
            .build();
        combined.log(&record);

        assert_eq!(buffer.len(), 1);
        assert!(
            primary_records
                .lock()
                .unwrap()
                .iter()
                .any(|message| message == "combined hello")
        );
    }

    #[test]
    fn test_short_strings_not_redacted() {
        // Short strings should not be redacted (might be legitimate short messages)
        let message = r#"Status: "ready""#;
        let entry = LogEntry::new(Level::Info, "test", message);

        // "ready" is short, should not be redacted
        assert!(entry.message.contains("ready"));
    }

    #[test]
    fn test_format_line() {
        let entry = LogEntry::new(Level::Warn, "my::module", "Test warning");
        let line = entry.format_line();

        assert!(line.contains("[WARN]"));
        assert!(line.contains("my::module"));
        assert!(line.contains("Test warning"));
        // Should have timestamp
        assert!(line.contains("-"));
        assert!(line.contains(":"));
    }
}
