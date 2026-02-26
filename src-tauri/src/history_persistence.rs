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
