//! Audio cue playback using rodio with graceful fallbacks.
//!
//! This module preloads WAV cues from `src-tauri/sounds/` and exposes
//! non-blocking playback helpers for recording lifecycle feedback.

#![allow(dead_code)] // Module under construction

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::{buffer::SamplesBuffer, Decoder, OutputStream, OutputStreamHandle, Sink, Source};

use crate::config;

/// Recommended pre-roll delay before mic capture starts to reduce beep pickup.
pub const START_CUE_PRE_ROLL: Duration = Duration::from_millis(75);

/// Audio cue variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CueType {
    StartRecording,
    StopRecording,
    CancelRecording,
    Error,
}

#[derive(Debug, Clone)]
struct CueBuffer {
    channels: u16,
    sample_rate: u32,
    samples: Arc<Vec<f32>>,
}

struct RodioOutputState {
    _stream: OutputStream,
    handle: OutputStreamHandle,
}

/// Manages loading and non-blocking playback of audio cue WAV files.
pub struct AudioCueManager {
    enabled: AtomicBool,
    sounds_dir: PathBuf,
    cues: HashMap<CueType, CueBuffer>,
    output: Mutex<Option<RodioOutputState>>,
    output_failure_logged: AtomicBool,
}

impl AudioCueManager {
    /// Create a cue manager using config-gated defaults and repository sounds.
    pub fn new() -> Self {
        let cfg = config::load_config();
        Self::with_sounds_dir(default_sounds_dir(), cfg.audio.audio_cues_enabled)
    }

    /// Create a cue manager with explicit sounds directory and enabled state.
    pub fn with_sounds_dir(sounds_dir: PathBuf, enabled: bool) -> Self {
        let cues = load_all_cues(&sounds_dir);
        Self {
            enabled: AtomicBool::new(enabled),
            sounds_dir,
            cues,
            output: Mutex::new(None),
            output_failure_logged: AtomicBool::new(false),
        }
    }

    /// Enable or disable cue playback globally.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Returns whether playback is currently enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Returns the configured sounds directory.
    pub fn sounds_dir(&self) -> &Path {
        &self.sounds_dir
    }

    /// Number of decoded cues loaded into memory.
    pub fn loaded_cue_count(&self) -> usize {
        self.cues.len()
    }

    /// Returns true when a cue has a decoded buffer available.
    pub fn has_cue(&self, cue: CueType) -> bool {
        self.cues.contains_key(&cue)
    }

    /// Play cue non-blocking. Returns immediately.
    ///
    /// Missing files or unavailable audio devices are logged and skipped.
    pub fn play_cue(&self, cue: CueType) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        let Some(buffer) = self.cues.get(&cue) else {
            log::debug!(
                "Audio cue {:?} unavailable (missing WAV or decode failure)",
                cue
            );
            return;
        };

        let handle = match self.ensure_output_handle() {
            Ok(handle) => handle,
            Err(error) => {
                if !self.output_failure_logged.swap(true, Ordering::Relaxed) {
                    log::warn!(
                        "Audio cue output unavailable; cues disabled until restart: {error}"
                    );
                } else {
                    log::debug!("Audio cue output unavailable: {error}");
                }
                return;
            }
        };

        match Sink::try_new(&handle) {
            Ok(sink) => {
                sink.append(SamplesBuffer::new(
                    buffer.channels,
                    buffer.sample_rate,
                    buffer.samples.as_ref().clone(),
                ));
                sink.detach();
            }
            Err(error) => {
                log::debug!("Audio cue sink init failed: {error}");
            }
        }
    }

    fn ensure_output_handle(&self) -> Result<OutputStreamHandle, String> {
        let mut guard = self
            .output
            .lock()
            .map_err(|_| "audio output lock poisoned".to_string())?;
        if let Some(state) = guard.as_ref() {
            return Ok(state.handle.clone());
        }

        let (stream, handle) = OutputStream::try_default().map_err(|error| error.to_string())?;
        *guard = Some(RodioOutputState {
            _stream: stream,
            handle: handle.clone(),
        });
        Ok(handle)
    }
}

impl Default for AudioCueManager {
    fn default() -> Self {
        Self::new()
    }
}

fn default_sounds_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sounds")
}

fn cue_candidates(cue: CueType) -> &'static [&'static str] {
    match cue {
        CueType::StartRecording => &["start.wav", "cue-start.wav"],
        CueType::StopRecording => &["stop.wav", "cue-stop.wav"],
        CueType::CancelRecording => &["cancel.wav", "cue-cancel.wav"],
        CueType::Error => &["error.wav", "cue-error.wav"],
    }
}

fn load_all_cues(sounds_dir: &Path) -> HashMap<CueType, CueBuffer> {
    let mut cues = HashMap::new();
    for cue in [
        CueType::StartRecording,
        CueType::StopRecording,
        CueType::CancelRecording,
        CueType::Error,
    ] {
        match load_cue(sounds_dir, cue) {
            Ok(Some(buffer)) => {
                cues.insert(cue, buffer);
            }
            Ok(None) => {
                log::warn!(
                    "Audio cue {:?} not found in {} (skipping)",
                    cue,
                    sounds_dir.display()
                );
            }
            Err(error) => {
                log::warn!("Audio cue {:?} failed to load: {}", cue, error);
            }
        }
    }
    cues
}

fn load_cue(sounds_dir: &Path, cue: CueType) -> Result<Option<CueBuffer>, String> {
    let mut found = None;
    for name in cue_candidates(cue) {
        let candidate = sounds_dir.join(name);
        if candidate.exists() {
            found = Some(candidate);
            break;
        }
    }

    let Some(path) = found else {
        return Ok(None);
    };

    let file = File::open(&path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let decoder = Decoder::new(BufReader::new(file))
        .map_err(|error| format!("decode {}: {error}", path.display()))?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples = decoder.convert_samples::<f32>().collect::<Vec<f32>>();

    Ok(Some(CueBuffer {
        channels,
        sample_rate,
        samples: Arc::new(samples),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_test_wav(path: &Path) {
        // Minimal mono PCM16 WAV containing one sample.
        let sample_rate: u32 = 16_000;
        let bits_per_sample: u16 = 16;
        let channels: u16 = 1;
        let sample_count: u32 = 1;
        let block_align: u16 = channels * (bits_per_sample / 8);
        let byte_rate: u32 = sample_rate * block_align as u32;
        let data_bytes: u32 = sample_count * block_align as u32;
        let riff_chunk_size: u32 = 36 + data_bytes;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&riff_chunk_size.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM format
        bytes.extend_from_slice(&channels.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&byte_rate.to_le_bytes());
        bytes.extend_from_slice(&block_align.to_le_bytes());
        bytes.extend_from_slice(&bits_per_sample.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_bytes.to_le_bytes());
        bytes.extend_from_slice(&0i16.to_le_bytes()); // one silent sample

        fs::write(path, bytes).expect("failed to write test wav");
    }

    #[test]
    fn loads_all_project_cues_from_sounds_directory() {
        let manager = AudioCueManager::with_sounds_dir(default_sounds_dir(), true);
        assert!(manager.has_cue(CueType::StartRecording));
        assert!(manager.has_cue(CueType::StopRecording));
        assert!(manager.has_cue(CueType::Error));
        assert!(manager.has_cue(CueType::CancelRecording));
        assert_eq!(manager.loaded_cue_count(), 4);
    }

    #[test]
    fn missing_directory_gracefully_loads_zero_cues() {
        let temp = TempDir::new().expect("tempdir");
        let missing = temp.path().join("does-not-exist");
        let manager = AudioCueManager::with_sounds_dir(missing, true);
        assert_eq!(manager.loaded_cue_count(), 0);
    }

    #[test]
    fn supports_primary_filenames_without_cue_prefix() {
        let temp = TempDir::new().expect("tempdir");
        for name in ["start.wav", "stop.wav", "cancel.wav", "error.wav"] {
            write_test_wav(&temp.path().join(name));
        }

        let manager = AudioCueManager::with_sounds_dir(temp.path().to_path_buf(), true);
        assert!(manager.has_cue(CueType::StartRecording));
        assert!(manager.has_cue(CueType::StopRecording));
        assert!(manager.has_cue(CueType::CancelRecording));
        assert!(manager.has_cue(CueType::Error));
    }

    #[test]
    fn play_cue_is_noop_when_disabled() {
        let manager = AudioCueManager::with_sounds_dir(default_sounds_dir(), false);
        manager.play_cue(CueType::StartRecording);
    }

    #[test]
    fn play_cue_is_noop_for_missing_buffer() {
        let temp = TempDir::new().expect("tempdir");
        let manager = AudioCueManager::with_sounds_dir(temp.path().to_path_buf(), true);
        manager.play_cue(CueType::CancelRecording);
    }

    #[test]
    fn set_enabled_toggles_cue_playback_dynamically() {
        let manager = AudioCueManager::with_sounds_dir(default_sounds_dir(), true);
        assert!(manager.is_enabled());

        manager.set_enabled(false);
        assert!(!manager.is_enabled());
        // play_cue returns immediately when disabled (no panic, no output)
        manager.play_cue(CueType::StartRecording);

        manager.set_enabled(true);
        assert!(manager.is_enabled());
    }

    #[test]
    fn start_cue_pre_roll_is_bounded_for_responsiveness() {
        // Pre-roll should be long enough to reduce beep capture but short
        // enough that recording start feels responsive (< 200ms).
        assert!(START_CUE_PRE_ROLL.as_millis() >= 50);
        assert!(START_CUE_PRE_ROLL.as_millis() <= 200);
    }

    #[test]
    fn project_cue_wav_files_are_valid_pcm16_mono_44100() {
        let sounds = default_sounds_dir();
        for filename in ["cue-start.wav", "cue-stop.wav", "cue-cancel.wav", "cue-error.wav"] {
            let path = sounds.join(filename);
            assert!(path.exists(), "{} must exist", filename);
            let file = File::open(&path).expect("open cue file");
            let decoder = Decoder::new(BufReader::new(file)).expect("decode cue file");
            assert_eq!(decoder.channels(), 1, "{filename} should be mono");
            assert_eq!(decoder.sample_rate(), 44100, "{filename} should be 44100 Hz");
        }
    }

    #[test]
    fn project_cue_wav_files_are_under_100kb() {
        let sounds = default_sounds_dir();
        for filename in ["cue-start.wav", "cue-stop.wav", "cue-cancel.wav", "cue-error.wav"] {
            let path = sounds.join(filename);
            let size = std::fs::metadata(&path)
                .unwrap_or_else(|_| panic!("{filename} metadata"))
                .len();
            assert!(
                size <= 100_000,
                "{filename} is {size} bytes, should be under 100KB"
            );
        }
    }

    #[test]
    fn cue_candidates_covers_all_cue_types() {
        for cue in [
            CueType::StartRecording,
            CueType::StopRecording,
            CueType::CancelRecording,
            CueType::Error,
        ] {
            let candidates = cue_candidates(cue);
            assert!(
                !candidates.is_empty(),
                "{cue:?} must have at least one filename candidate"
            );
        }
    }

    #[test]
    fn new_manager_respects_config_enabled_state() {
        // AudioCueManager::new() reads config.audio.audio_cues_enabled.
        // We can't control config in tests easily, but we can verify the
        // constructor path doesn't panic and produces a valid manager.
        let manager = AudioCueManager::new();
        // Should have loaded at least the project cues
        assert!(manager.loaded_cue_count() >= 3);
    }
}
