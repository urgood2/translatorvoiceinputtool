//! Transcript history persistence backends.
//!
//! Keeps disk I/O concerns separate from the in-memory history ring buffer.

#![allow(dead_code)] // Module under construction

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use keyring::Error as KeyringError;
use rand::RngCore;
use thiserror::Error;
use uuid::Uuid;

use crate::history::TranscriptEntry;

const ENCRYPTION_KEY_BYTES: usize = 32;
const ENCRYPTION_NONCE_BYTES: usize = 12;
const ENCRYPTION_MAGIC: &[u8] = b"OVH1";
const HISTORY_KEYRING_SERVICE: &str = "openvoicy.history";
const HISTORY_KEYRING_ACCOUNT: &str = "history-encryption-key";

/// Storage error type for transcript history persistence.
#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("history persistence I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("history persistence serialization error: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("history persistence parse error on line {line}: {source}")]
    Deserialize {
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("history keychain error: {0}")]
    Keychain(String),
    #[error("history encryption error: {0}")]
    Crypto(String),
}

/// Persistence abstraction for transcript history snapshots.
pub trait HistoryPersistence: Send + Sync {
    fn save(&self, entries: &[TranscriptEntry]) -> Result<(), PersistenceError>;
    fn load(&self) -> Result<Vec<TranscriptEntry>, PersistenceError>;
    fn purge(&self) -> Result<(), PersistenceError>;
    fn entry_count(&self) -> Result<usize, PersistenceError>;
}

/// Effective persistence policy after applying config gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryPersistencePolicy {
    MemoryOnly,
    DiskPlaintext,
    DiskEncrypted,
}

/// Resolve effective persistence policy from config flags and keychain availability.
pub fn resolve_history_persistence_policy(
    persistence_mode: &str,
    encrypt_at_rest: bool,
    keychain_available: bool,
) -> HistoryPersistencePolicy {
    if persistence_mode != "disk" {
        return HistoryPersistencePolicy::MemoryOnly;
    }

    if !encrypt_at_rest {
        return HistoryPersistencePolicy::DiskPlaintext;
    }

    if keychain_available {
        HistoryPersistencePolicy::DiskEncrypted
    } else {
        HistoryPersistencePolicy::MemoryOnly
    }
}

/// Keychain-backed encryption provider.
#[derive(Debug, Clone)]
pub struct EncryptionProvider {
    key: [u8; ENCRYPTION_KEY_BYTES],
}

impl EncryptionProvider {
    /// Load or create the persistent history encryption key from the OS keychain.
    pub fn from_keychain() -> Result<Self, PersistenceError> {
        let entry = keyring::Entry::new(HISTORY_KEYRING_SERVICE, HISTORY_KEYRING_ACCOUNT).map_err(
            |error| {
                PersistenceError::Keychain(format!(
                    "failed to initialize keychain entry for history encryption: {error}"
                ))
            },
        )?;

        match entry.get_password() {
            Ok(encoded_key) => {
                let raw_key = BASE64_STANDARD.decode(encoded_key).map_err(|error| {
                    PersistenceError::Keychain(format!(
                        "history encryption key in keychain is not valid base64: {error}"
                    ))
                })?;
                Self::from_key_bytes(&raw_key)
            }
            Err(KeyringError::NoEntry) => {
                let mut key = [0u8; ENCRYPTION_KEY_BYTES];
                rand::rngs::OsRng.fill_bytes(&mut key);

                let encoded_key = BASE64_STANDARD.encode(key);
                entry.set_password(&encoded_key).map_err(|error| {
                    PersistenceError::Keychain(format!(
                        "failed to store history encryption key in keychain: {error}"
                    ))
                })?;

                Ok(Self { key })
            }
            Err(error) => Err(PersistenceError::Keychain(format!(
                "failed to read history encryption key from keychain: {error}"
            ))),
        }
    }

    fn from_key_bytes(raw_key: &[u8]) -> Result<Self, PersistenceError> {
        if raw_key.len() != ENCRYPTION_KEY_BYTES {
            return Err(PersistenceError::Keychain(format!(
                "history encryption key has invalid length: expected {ENCRYPTION_KEY_BYTES}, got {}",
                raw_key.len()
            )));
        }

        let mut key = [0u8; ENCRYPTION_KEY_BYTES];
        key.copy_from_slice(raw_key);
        Ok(Self { key })
    }

    #[cfg(test)]
    fn from_raw_key(key: [u8; ENCRYPTION_KEY_BYTES]) -> Self {
        Self { key }
    }

    fn cipher(&self) -> Result<Aes256Gcm, PersistenceError> {
        Aes256Gcm::new_from_slice(&self.key).map_err(|error| {
            PersistenceError::Crypto(format!("failed to initialize AES-256-GCM cipher: {error}"))
        })
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, PersistenceError> {
        let mut nonce_bytes = [0u8; ENCRYPTION_NONCE_BYTES];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);

        let ciphertext = self
            .cipher()?
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
            .map_err(|_| {
                PersistenceError::Crypto(
                    "failed to encrypt history payload with AES-256-GCM".to_string(),
                )
            })?;

        let mut payload =
            Vec::with_capacity(ENCRYPTION_MAGIC.len() + ENCRYPTION_NONCE_BYTES + ciphertext.len());
        payload.extend_from_slice(ENCRYPTION_MAGIC);
        payload.extend_from_slice(&nonce_bytes);
        payload.extend_from_slice(&ciphertext);
        Ok(payload)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, PersistenceError> {
        let min_size = ENCRYPTION_MAGIC.len() + ENCRYPTION_NONCE_BYTES;
        if ciphertext.len() < min_size {
            return Err(PersistenceError::Crypto(
                "history encryption payload is truncated or corrupt; purge history to recover"
                    .to_string(),
            ));
        }

        let (magic, rest) = ciphertext.split_at(ENCRYPTION_MAGIC.len());
        if magic != ENCRYPTION_MAGIC {
            return Err(PersistenceError::Crypto(
                "history encryption payload header is invalid; purge history to recover"
                    .to_string(),
            ));
        }

        let (nonce_bytes, encrypted_bytes) = rest.split_at(ENCRYPTION_NONCE_BYTES);
        self.cipher()?
            .decrypt(Nonce::from_slice(nonce_bytes), encrypted_bytes)
            .map_err(|_| {
                PersistenceError::Crypto(
                    "failed to decrypt history payload; keychain key unavailable or payload corrupted. Purge and reinstall to recover"
                        .to_string(),
                )
            })
    }
}

/// JSONL persistence backend.
pub struct JsonlPersistence {
    path: PathBuf,
    encryption: Option<EncryptionProvider>,
    max_entries: usize,
    save_state: Mutex<JsonlSaveState>,
}

#[derive(Debug, Default)]
struct JsonlSaveState {
    last_saved_id: Option<Uuid>,
    last_snapshot_len: usize,
}

impl JsonlPersistence {
    pub fn new(path: PathBuf, encryption: Option<EncryptionProvider>, max_entries: usize) -> Self {
        Self {
            path,
            encryption,
            max_entries: max_entries.max(1),
            save_state: Mutex::new(JsonlSaveState::default()),
        }
    }

    fn temporary_path(path: &Path) -> PathBuf {
        let mut os_path = path.as_os_str().to_os_string();
        os_path.push(".tmp");
        PathBuf::from(os_path)
    }

    fn normalize_entries(&self, mut entries: Vec<TranscriptEntry>) -> Vec<TranscriptEntry> {
        let keep = self.max_entries;
        if entries.len() > keep {
            entries = entries.split_off(entries.len() - keep);
        }
        entries
    }

    fn maybe_encrypt(&self, plaintext: Vec<u8>) -> Result<Vec<u8>, PersistenceError> {
        if let Some(provider) = &self.encryption {
            provider.encrypt(&plaintext)
        } else {
            Ok(plaintext)
        }
    }

    fn maybe_decrypt(&self, ciphertext: Vec<u8>) -> Result<Vec<u8>, PersistenceError> {
        if let Some(provider) = &self.encryption {
            provider.decrypt(&ciphertext)
        } else {
            Ok(ciphertext)
        }
    }

    fn update_save_state(&self, entries: &[TranscriptEntry]) {
        let mut state = self.save_state.lock().unwrap();
        state.last_saved_id = entries.last().map(|entry| entry.id);
        state.last_snapshot_len = entries.len();
    }

    fn reset_save_state(&self) {
        let mut state = self.save_state.lock().unwrap();
        state.last_saved_id = None;
        state.last_snapshot_len = 0;
    }

    fn rewrite_snapshot(&self, entries: &[TranscriptEntry]) -> Result<(), PersistenceError> {
        let start = entries.len().saturating_sub(self.max_entries);
        let mut payload = String::new();
        for entry in entries.iter().skip(start) {
            payload.push_str(&serde_json::to_string(entry)?);
            payload.push('\n');
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp = Self::temporary_path(&self.path);
        let encrypted = self.maybe_encrypt(payload.into_bytes())?;
        fs::write(&tmp, encrypted)?;
        if self.path.exists() {
            match fs::remove_file(&self.path) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(PersistenceError::Io(error)),
            }
        }
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    fn append_entries(&self, entries: &[TranscriptEntry]) -> Result<(), PersistenceError> {
        if entries.is_empty() {
            return Ok(());
        }

        // Encrypted mode always rewrites the full snapshot because
        // independent encrypted appends are not stream-decodable as JSONL.
        if self.encryption.is_some() {
            return Err(PersistenceError::Crypto(
                "append mode is unavailable for encrypted history payloads".to_string(),
            ));
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut payload = String::new();
        for entry in entries {
            payload.push_str(&serde_json::to_string(entry)?);
            payload.push('\n');
        }

        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(payload.as_bytes())?;
        file.flush()?;
        Ok(())
    }
}

impl HistoryPersistence for JsonlPersistence {
    fn save(&self, entries: &[TranscriptEntry]) -> Result<(), PersistenceError> {
        let start = entries.len().saturating_sub(self.max_entries);
        let snapshot = &entries[start..];

        if self.encryption.is_some() {
            self.rewrite_snapshot(snapshot)?;
            self.update_save_state(snapshot);
            return Ok(());
        }

        if snapshot.is_empty() {
            self.rewrite_snapshot(snapshot)?;
            self.update_save_state(snapshot);
            return Ok(());
        }

        let state = self.save_state.lock().unwrap();
        if snapshot.len() < state.last_snapshot_len {
            drop(state);
            self.rewrite_snapshot(snapshot)?;
            self.update_save_state(snapshot);
            return Ok(());
        }

        if let Some(last_saved_id) = state.last_saved_id {
            if let Some(position) = snapshot.iter().position(|entry| entry.id == last_saved_id) {
                let new_entries = &snapshot[position + 1..];
                drop(state);
                self.append_entries(new_entries)?;
                self.update_save_state(snapshot);
                return Ok(());
            }
        }

        drop(state);
        self.rewrite_snapshot(snapshot)?;
        self.update_save_state(snapshot);
        Ok(())
    }

    fn load(&self) -> Result<Vec<TranscriptEntry>, PersistenceError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(PersistenceError::Io(error)),
        };
        if bytes.is_empty() {
            return Ok(Vec::new());
        }

        let plaintext = self.maybe_decrypt(bytes)?;
        let content = String::from_utf8(plaintext)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
        let mut entries = Vec::new();

        for (idx, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<TranscriptEntry>(line) {
                Ok(entry) => entries.push(entry),
                Err(source) => {
                    return Err(PersistenceError::Deserialize {
                        line: idx + 1,
                        source,
                    });
                }
            }
        }

        let normalized = self.normalize_entries(entries);
        self.update_save_state(&normalized);
        Ok(normalized)
    }

    fn purge(&self) -> Result<(), PersistenceError> {
        match fs::remove_file(&self.path) {
            Ok(()) => {
                self.reset_save_state();
                Ok(())
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.reset_save_state();
                Ok(())
            }
            Err(error) => Err(PersistenceError::Io(error)),
        }
    }

    fn entry_count(&self) -> Result<usize, PersistenceError> {
        Ok(self.load()?.len())
    }
}

/// Memory-only persistence backend.
pub struct NullPersistence;

impl HistoryPersistence for NullPersistence {
    fn save(&self, _entries: &[TranscriptEntry]) -> Result<(), PersistenceError> {
        Ok(())
    }

    fn load(&self) -> Result<Vec<TranscriptEntry>, PersistenceError> {
        Ok(Vec::new())
    }

    fn purge(&self) -> Result<(), PersistenceError> {
        Ok(())
    }

    fn entry_count(&self) -> Result<usize, PersistenceError> {
        Ok(0)
    }
}

/// Build the history persistence backend from config gating rules.
///
/// - `persistence_mode != "disk"` always returns memory-only persistence.
/// - `persistence_mode == "disk" && encrypt_at_rest == false` returns plaintext JSONL.
/// - `persistence_mode == "disk" && encrypt_at_rest == true` returns encrypted JSONL
///   when keychain is available; otherwise falls back to memory-only mode.
pub fn build_history_persistence(
    history_path: PathBuf,
    max_entries: usize,
    persistence_mode: &str,
    encrypt_at_rest: bool,
) -> Box<dyn HistoryPersistence> {
    if persistence_mode != "disk" {
        return Box::new(NullPersistence);
    }

    if !encrypt_at_rest {
        log::warn!(
            "History disk persistence is explicitly configured without encryption \
            (history.encrypt_at_rest=false)."
        );
        return Box::new(JsonlPersistence::new(history_path, None, max_entries));
    }

    match EncryptionProvider::from_keychain() {
        Ok(encryption) => Box::new(JsonlPersistence::new(
            history_path,
            Some(encryption),
            max_entries,
        )),
        Err(error) => {
            log::warn!(
                "History encryption requested but keychain is unavailable ({}). \
                Falling back to memory-only history for privacy. \
                Set history.encrypt_at_rest=false to explicitly allow unencrypted disk persistence.",
                error
            );
            Box::new(NullPersistence)
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::history::HistoryInjectionResult;

    fn sample_entry(text: &str) -> TranscriptEntry {
        TranscriptEntry::new(
            text.to_string(),
            1000,
            200,
            HistoryInjectionResult::Injected,
        )
    }

    #[test]
    fn resolve_policy_memory_mode_ignores_encrypt_flag() {
        assert_eq!(
            resolve_history_persistence_policy("memory", true, true),
            HistoryPersistencePolicy::MemoryOnly
        );
        assert_eq!(
            resolve_history_persistence_policy("memory", false, false),
            HistoryPersistencePolicy::MemoryOnly
        );
    }

    #[test]
    fn resolve_policy_disk_without_encryption_uses_plaintext() {
        assert_eq!(
            resolve_history_persistence_policy("disk", false, false),
            HistoryPersistencePolicy::DiskPlaintext
        );
        assert_eq!(
            resolve_history_persistence_policy("disk", false, true),
            HistoryPersistencePolicy::DiskPlaintext
        );
    }

    #[test]
    fn resolve_policy_disk_with_encryption_requires_keychain() {
        assert_eq!(
            resolve_history_persistence_policy("disk", true, true),
            HistoryPersistencePolicy::DiskEncrypted
        );
        assert_eq!(
            resolve_history_persistence_policy("disk", true, false),
            HistoryPersistencePolicy::MemoryOnly
        );
    }

    #[test]
    fn null_persistence_is_noop() {
        let persistence = NullPersistence;
        persistence
            .save(&[sample_entry("one"), sample_entry("two")])
            .expect("null save should succeed");
        assert!(persistence
            .load()
            .expect("null load should succeed")
            .is_empty());
        assert_eq!(
            persistence.entry_count().expect("entry_count should work"),
            0
        );
        persistence.purge().expect("null purge should succeed");
    }

    #[test]
    fn jsonl_persistence_round_trip() {
        let dir = tempdir().expect("temp dir should be available");
        let path = dir.path().join("history.jsonl");
        let persistence = JsonlPersistence::new(path.clone(), None, 10);
        let entries = vec![sample_entry("alpha"), sample_entry("beta")];

        persistence.save(&entries).expect("save should succeed");
        let loaded = persistence.load().expect("load should succeed");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].text, "alpha");
        assert_eq!(loaded[1].text, "beta");
        assert_eq!(
            persistence.entry_count().expect("entry_count should work"),
            2
        );
        assert!(path.exists());
    }

    #[test]
    fn encrypted_provider_round_trip() {
        let provider = EncryptionProvider::from_raw_key([7u8; ENCRYPTION_KEY_BYTES]);
        let plaintext = b"history-line-1\nhistory-line-2\n";

        let ciphertext = provider.encrypt(plaintext).expect("encrypt should succeed");
        assert_ne!(ciphertext, plaintext);

        let decrypted = provider
            .decrypt(&ciphertext)
            .expect("decrypt should succeed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypted_provider_rejects_corrupt_payload() {
        let provider = EncryptionProvider::from_raw_key([7u8; ENCRYPTION_KEY_BYTES]);
        let error = provider
            .decrypt(b"OVH1")
            .expect_err("truncated payload should fail");

        assert!(matches!(error, PersistenceError::Crypto(_)));
    }

    #[test]
    fn jsonl_persistence_round_trip_encrypted() {
        let dir = tempdir().expect("temp dir should be available");
        let path = dir.path().join("history.enc");
        let provider = EncryptionProvider::from_raw_key([11u8; ENCRYPTION_KEY_BYTES]);
        let persistence = JsonlPersistence::new(path.clone(), Some(provider), 10);
        let entries = vec![sample_entry("alpha"), sample_entry("beta")];

        persistence.save(&entries).expect("save should succeed");

        let on_disk = fs::read(&path).expect("encrypted payload should exist");
        assert!(!on_disk
            .windows("alpha".len())
            .any(|window| window == b"alpha"));

        let loaded = persistence.load().expect("load should succeed");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].text, "alpha");
        assert_eq!(loaded[1].text, "beta");
    }

    #[test]
    fn jsonl_persistence_enforces_max_entries() {
        let dir = tempdir().expect("temp dir should be available");
        let path = dir.path().join("history.jsonl");
        let persistence = JsonlPersistence::new(path, None, 2);
        let entries = vec![
            sample_entry("entry-1"),
            sample_entry("entry-2"),
            sample_entry("entry-3"),
        ];

        persistence.save(&entries).expect("save should succeed");
        let loaded = persistence.load().expect("load should succeed");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].text, "entry-2");
        assert_eq!(loaded[1].text, "entry-3");
    }

    #[test]
    fn jsonl_persistence_purge_removes_file() {
        let dir = tempdir().expect("temp dir should be available");
        let path = dir.path().join("history.jsonl");
        let persistence = JsonlPersistence::new(path.clone(), None, 10);
        persistence
            .save(&[sample_entry("kept")])
            .expect("save should succeed");
        assert!(path.exists());

        persistence.purge().expect("purge should succeed");
        assert!(!path.exists());
    }

    #[test]
    fn encrypted_file_not_readable_with_wrong_key() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("history.enc");
        let provider_a = EncryptionProvider::from_raw_key([11u8; ENCRYPTION_KEY_BYTES]);
        let provider_b = EncryptionProvider::from_raw_key([99u8; ENCRYPTION_KEY_BYTES]);

        let persistence_a = JsonlPersistence::new(path.clone(), Some(provider_a), 10);
        persistence_a
            .save(&[sample_entry("secret")])
            .expect("save should succeed");

        let persistence_b = JsonlPersistence::new(path.clone(), Some(provider_b), 10);
        let result = persistence_b.load();
        assert!(result.is_err(), "wrong key should not decrypt");
        let error = result.unwrap_err();
        assert!(matches!(error, PersistenceError::Crypto(_)));
    }

    #[test]
    fn encrypted_file_is_binary_on_disk() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("history.enc");
        let provider = EncryptionProvider::from_raw_key([42u8; ENCRYPTION_KEY_BYTES]);
        let persistence = JsonlPersistence::new(path.clone(), Some(provider), 10);
        persistence
            .save(&[sample_entry("sensitive data")])
            .expect("save should succeed");

        let raw_bytes = fs::read(&path).expect("file should exist");
        // Must start with magic header
        assert!(raw_bytes.starts_with(ENCRYPTION_MAGIC));
        // Must not contain plaintext
        let raw_str = String::from_utf8_lossy(&raw_bytes);
        assert!(
            !raw_str.contains("sensitive data"),
            "plaintext should not appear in encrypted file"
        );
    }

    #[test]
    fn fresh_instance_loads_persisted_entries() {
        // Simulates app restart: save with one instance, load with a fresh one.
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("history.jsonl");

        let persistence_1 = JsonlPersistence::new(path.clone(), None, 10);
        persistence_1
            .save(&[sample_entry("from-session-1"), sample_entry("from-session-2")])
            .expect("save should succeed");

        // Drop and create new instance (simulating restart)
        drop(persistence_1);
        let persistence_2 = JsonlPersistence::new(path, None, 10);
        let loaded = persistence_2.load().expect("load should succeed");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].text, "from-session-1");
        assert_eq!(loaded[1].text, "from-session-2");
    }

    #[test]
    fn corrupt_file_returns_error_for_graceful_recovery() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("history.jsonl");

        // Write garbage data
        fs::write(&path, "not-valid-json\n").expect("write should succeed");

        let persistence = JsonlPersistence::new(path, None, 10);
        let result = persistence.load();
        assert!(result.is_err(), "corrupt file should return error");
        assert!(matches!(
            result.unwrap_err(),
            PersistenceError::Deserialize { line: 1, .. }
        ));
    }

    #[test]
    fn null_persistence_does_not_write_files() {
        let dir = tempdir().expect("temp dir");
        let persistence = NullPersistence;
        persistence
            .save(&[sample_entry("should not be written")])
            .expect("null save should succeed");

        let entries: Vec<_> = fs::read_dir(dir.path())
            .expect("read_dir should succeed")
            .collect();
        assert!(entries.is_empty(), "no files should be created");
    }

    #[test]
    fn build_memory_mode_returns_null_persistence() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("history.jsonl");
        let persistence = build_history_persistence(path.clone(), 100, "memory", false);
        persistence
            .save(&[sample_entry("test")])
            .expect("save should succeed");
        assert!(!path.exists(), "memory mode should not write files");

        let loaded = persistence.load().expect("load should succeed");
        assert!(loaded.is_empty());
    }

    #[test]
    fn build_disk_plaintext_returns_jsonl_persistence() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("history.jsonl");
        let persistence = build_history_persistence(path.clone(), 100, "disk", false);
        persistence
            .save(&[sample_entry("on-disk")])
            .expect("save should succeed");
        assert!(path.exists(), "disk mode should write file");

        let loaded = persistence.load().expect("load should succeed");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].text, "on-disk");
    }

    #[test]
    fn encrypted_restart_round_trip_preserves_entries() {
        // Simulates app restart with encrypted persistence: save with one instance,
        // drop it, create a new instance with the same key, and verify load.
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("history.enc");
        let key = [55u8; ENCRYPTION_KEY_BYTES];

        let provider_1 = EncryptionProvider::from_raw_key(key);
        let persistence_1 = JsonlPersistence::new(path.clone(), Some(provider_1), 10);
        persistence_1
            .save(&[sample_entry("encrypted-session-1"), sample_entry("encrypted-session-2")])
            .expect("save should succeed");
        drop(persistence_1);

        // New instance with same key (simulating restart)
        let provider_2 = EncryptionProvider::from_raw_key(key);
        let persistence_2 = JsonlPersistence::new(path, Some(provider_2), 10);
        let loaded = persistence_2.load().expect("load should succeed");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].text, "encrypted-session-1");
        assert_eq!(loaded[1].text, "encrypted-session-2");
    }

    #[test]
    fn jsonl_persistence_empty_history_save_and_load() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("history.jsonl");
        let persistence = JsonlPersistence::new(path.clone(), None, 10);

        // Save empty snapshot
        persistence.save(&[]).expect("save empty should succeed");
        let loaded = persistence.load().expect("load should succeed");
        assert!(loaded.is_empty(), "loaded entries should be empty");
        assert_eq!(
            persistence.entry_count().expect("entry_count should work"),
            0
        );
    }

    #[test]
    fn jsonl_persistence_load_nonexistent_file_returns_empty() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("does-not-exist.jsonl");
        let persistence = JsonlPersistence::new(path, None, 10);

        let loaded = persistence.load().expect("load should succeed");
        assert!(loaded.is_empty());
    }

    #[test]
    fn build_disk_encrypted_falls_back_to_memory_when_keychain_unavailable() {
        // The build_history_persistence function calls from_keychain() which
        // may fail in CI/test environments. Either way, it should NOT produce
        // unencrypted disk files when encrypt_at_rest=true.
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("history.enc");
        let persistence = build_history_persistence(path.clone(), 100, "disk", true);

        // Save an entry
        persistence
            .save(&[sample_entry("maybe-encrypted")])
            .expect("save should succeed");

        if path.exists() {
            // If keychain was available, file should be encrypted (not plaintext)
            let raw = fs::read(&path).expect("file should be readable");
            assert!(
                raw.starts_with(ENCRYPTION_MAGIC),
                "disk file with encrypt_at_rest=true must be encrypted"
            );
            let raw_str = String::from_utf8_lossy(&raw);
            assert!(
                !raw_str.contains("maybe-encrypted"),
                "plaintext must not appear on disk when encrypt_at_rest=true"
            );
        } else {
            // Keychain unavailable: fell back to NullPersistence (memory-only)
            // This is correct behavior - no unencrypted data on disk
            let loaded = persistence.load().expect("load should succeed");
            assert!(
                loaded.is_empty(),
                "memory-only fallback should have no persisted entries"
            );
        }
    }

    #[test]
    fn jsonl_persistence_appends_only_new_entries_across_snapshots() {
        let dir = tempdir().expect("temp dir should be available");
        let path = dir.path().join("history.jsonl");
        let persistence = JsonlPersistence::new(path.clone(), None, 3);
        let entry_a = sample_entry("a");
        let entry_b = sample_entry("b");
        let entry_c = sample_entry("c");
        let entry_d = sample_entry("d");

        persistence
            .save(&[entry_a.clone(), entry_b.clone()])
            .expect("first save should succeed");
        persistence
            .save(&[entry_a.clone(), entry_b.clone(), entry_c.clone()])
            .expect("second save should succeed");
        persistence
            .save(&[entry_b.clone(), entry_c.clone(), entry_d.clone()])
            .expect("third save should append rollover entry");

        let on_disk = fs::read_to_string(&path).expect("history file should be readable");
        assert_eq!(on_disk.lines().count(), 4);

        let loaded = persistence.load().expect("load should succeed");
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].id, entry_b.id);
        assert_eq!(loaded[1].id, entry_c.id);
        assert_eq!(loaded[2].id, entry_d.id);
    }
}
