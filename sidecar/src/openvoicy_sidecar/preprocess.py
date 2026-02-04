"""Audio preprocessing pipeline for ASR input.

This module provides deterministic audio preprocessing to prepare
audio for ASR inference. All output is standardized to 16kHz mono float32.

Pipeline Order (locked):
1. Convert to float32
2. Downmix to mono
3. Resample to 16kHz
4. DC offset removal
5. Peak clamp to [-1, 1]
6. Optional: peak normalization
7. Optional: silence trim

Output Specification:
- Sample rate: 16000 Hz
- Channels: 1 (mono)
- Format: float32
- Range: [-1.0, 1.0]
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

import numpy as np

from .protocol import log

# === Constants ===

TARGET_SAMPLE_RATE = 16000
TARGET_DTYPE = np.float32

# Silence detection defaults
DEFAULT_SILENCE_THRESHOLD_DB = -40.0
DEFAULT_SILENCE_WINDOW_MS = 20
DEFAULT_SILENCE_PADDING_MS = 100


@dataclass
class PreprocessConfig:
    """Configuration for audio preprocessing."""

    silence_trim_enabled: bool = True
    silence_threshold_db: float = DEFAULT_SILENCE_THRESHOLD_DB
    normalize_enabled: bool = False


def convert_to_float32(audio: np.ndarray) -> np.ndarray:
    """Convert audio to float32 in range [-1, 1].

    Handles various input dtypes:
    - int16: divide by 32768
    - int32: divide by 2147483648
    - float32/float64: pass through (or cast)
    - uint8: subtract 128, divide by 128
    """
    if audio.dtype == np.float32:
        return audio
    elif audio.dtype == np.float64:
        return audio.astype(np.float32)
    elif audio.dtype == np.int16:
        return (audio / 32768.0).astype(np.float32)
    elif audio.dtype == np.int32:
        return (audio / 2147483648.0).astype(np.float32)
    elif audio.dtype == np.uint8:
        return ((audio.astype(np.float32) - 128) / 128.0)
    else:
        # Try direct cast
        return audio.astype(np.float32)


def downmix_to_mono(audio: np.ndarray) -> np.ndarray:
    """Downmix multi-channel audio to mono by averaging channels.

    Args:
        audio: Input audio array. Shape can be:
            - (samples,) - already mono, return as-is
            - (samples, channels) - average across axis 1
            - (channels, samples) - transpose and average

    Returns:
        Mono audio array with shape (samples,)
    """
    if audio.ndim == 1:
        return audio

    if audio.ndim == 2:
        # Assume (samples, channels) if second dim is small
        if audio.shape[1] <= 8:  # Up to 7.1 surround
            return audio.mean(axis=1).astype(TARGET_DTYPE)
        elif audio.shape[0] <= 8:  # (channels, samples) format
            return audio.mean(axis=0).astype(TARGET_DTYPE)
        else:
            # Ambiguous, assume (samples, channels)
            return audio.mean(axis=1).astype(TARGET_DTYPE)

    # Higher dimensions - flatten and hope for the best
    return audio.flatten().astype(TARGET_DTYPE)


def resample(audio: np.ndarray, original_sr: int, target_sr: int = TARGET_SAMPLE_RATE) -> np.ndarray:
    """Resample audio to target sample rate using polyphase filter.

    Uses scipy's resample_poly for high-quality resampling.

    Args:
        audio: Input audio array (mono, float32).
        original_sr: Original sample rate.
        target_sr: Target sample rate (default: 16000).

    Returns:
        Resampled audio array.
    """
    if original_sr == target_sr:
        return audio

    try:
        from scipy import signal

        # Calculate resampling ratio
        gcd = np.gcd(original_sr, target_sr)
        up = target_sr // gcd
        down = original_sr // gcd

        # Use polyphase filter for quality
        resampled = signal.resample_poly(audio, up, down)
        return resampled.astype(TARGET_DTYPE)

    except ImportError:
        # Fallback to simple linear interpolation if scipy unavailable
        log("scipy not available, using linear interpolation for resampling")
        ratio = target_sr / original_sr
        new_length = int(len(audio) * ratio)
        indices = np.linspace(0, len(audio) - 1, new_length)
        return np.interp(indices, np.arange(len(audio)), audio).astype(TARGET_DTYPE)


def remove_dc_offset(audio: np.ndarray) -> np.ndarray:
    """Remove DC offset by subtracting the mean.

    This is a simple but effective method for removing DC bias
    from audio signals.

    Args:
        audio: Input audio array.

    Returns:
        Audio with DC offset removed.
    """
    return (audio - np.mean(audio)).astype(TARGET_DTYPE)


def peak_clamp(audio: np.ndarray) -> np.ndarray:
    """Clamp audio values to [-1, 1] range.

    Values outside this range are saturated (hard clipped).

    Args:
        audio: Input audio array.

    Returns:
        Audio with values clamped to [-1, 1].
    """
    return np.clip(audio, -1.0, 1.0).astype(TARGET_DTYPE)


def peak_normalize(audio: np.ndarray, target_peak: float = 1.0) -> np.ndarray:
    """Normalize audio so peak amplitude equals target.

    Args:
        audio: Input audio array.
        target_peak: Target peak amplitude (default: 1.0).

    Returns:
        Normalized audio array. Returns unchanged if max is 0.
    """
    max_val = np.abs(audio).max()
    if max_val == 0:
        return audio
    return (audio * (target_peak / max_val)).astype(TARGET_DTYPE)


def compute_rms_energy(audio: np.ndarray, window_samples: int) -> np.ndarray:
    """Compute RMS energy in sliding windows.

    Args:
        audio: Input audio array.
        window_samples: Window size in samples.

    Returns:
        Array of RMS values, one per window.
    """
    # Pad to make even windows
    padded_length = int(np.ceil(len(audio) / window_samples) * window_samples)
    padded = np.zeros(padded_length, dtype=TARGET_DTYPE)
    padded[:len(audio)] = audio

    # Reshape to (num_windows, window_samples)
    num_windows = padded_length // window_samples
    windows = padded.reshape(num_windows, window_samples)

    # Compute RMS for each window
    rms = np.sqrt(np.mean(windows ** 2, axis=1))
    return rms


def db_to_linear(db: float) -> float:
    """Convert decibels to linear amplitude."""
    return 10 ** (db / 20)


def trim_silence(
    audio: np.ndarray,
    sample_rate: int,
    threshold_db: float = DEFAULT_SILENCE_THRESHOLD_DB,
    window_ms: float = DEFAULT_SILENCE_WINDOW_MS,
    padding_ms: float = DEFAULT_SILENCE_PADDING_MS,
) -> np.ndarray:
    """Trim leading and trailing silence from audio.

    Uses RMS energy in sliding windows to detect speech boundaries.

    Args:
        audio: Input audio array.
        sample_rate: Sample rate of the audio.
        threshold_db: Energy threshold in dB below peak (negative value).
        window_ms: Analysis window size in milliseconds.
        padding_ms: Padding to keep around detected speech.

    Returns:
        Audio with silence trimmed.
    """
    if len(audio) == 0:
        return audio

    # Convert parameters to samples
    window_samples = int(sample_rate * window_ms / 1000)
    padding_samples = int(sample_rate * padding_ms / 1000)

    # Ensure minimum window size
    window_samples = max(window_samples, 1)

    # Compute RMS energy per window
    rms = compute_rms_energy(audio, window_samples)

    if len(rms) == 0:
        return audio  # Empty input

    if rms.max() == 0:
        # All silence - return empty array
        return np.array([], dtype=TARGET_DTYPE)

    # Convert threshold from dB below peak to absolute
    peak_rms = rms.max()
    threshold_linear = peak_rms * db_to_linear(threshold_db)

    # Find first and last windows above threshold
    above_threshold = rms > threshold_linear

    if not above_threshold.any():
        # All silence - return very short array
        return np.array([], dtype=TARGET_DTYPE)

    first_window = np.argmax(above_threshold)
    last_window = len(above_threshold) - 1 - np.argmax(above_threshold[::-1])

    # Convert window indices to sample indices
    start_sample = max(0, first_window * window_samples - padding_samples)
    end_sample = min(len(audio), (last_window + 1) * window_samples + padding_samples)

    return audio[start_sample:end_sample].astype(TARGET_DTYPE)


def preprocess(
    audio: np.ndarray,
    sample_rate: int,
    config: Optional[PreprocessConfig] = None,
) -> np.ndarray:
    """Apply full preprocessing pipeline to audio.

    Pipeline order:
    1. Convert to float32
    2. Downmix to mono
    3. Resample to 16kHz
    4. DC offset removal
    5. Peak clamp to [-1, 1]
    6. Optional: peak normalization
    7. Optional: silence trim

    Args:
        audio: Input audio array (any dtype, any channels).
        sample_rate: Original sample rate.
        config: Preprocessing configuration (uses defaults if None).

    Returns:
        Preprocessed audio (16kHz mono float32 in [-1, 1]).
    """
    config = config or PreprocessConfig()

    # 1. Convert to float32
    audio = convert_to_float32(audio)

    # 2. Downmix to mono
    audio = downmix_to_mono(audio)

    # 3. Resample to 16kHz
    audio = resample(audio, sample_rate, TARGET_SAMPLE_RATE)

    # 4. DC offset removal
    audio = remove_dc_offset(audio)

    # 5. Peak clamp
    audio = peak_clamp(audio)

    # 6. Optional: peak normalization
    if config.normalize_enabled:
        audio = peak_normalize(audio)

    # 7. Optional: silence trim
    if config.silence_trim_enabled:
        audio = trim_silence(
            audio,
            TARGET_SAMPLE_RATE,
            threshold_db=config.silence_threshold_db,
        )

    return audio.astype(TARGET_DTYPE)


def get_audio_info(audio: np.ndarray, sample_rate: int) -> dict:
    """Get information about audio array.

    Useful for debugging and logging.

    Args:
        audio: Input audio array.
        sample_rate: Sample rate.

    Returns:
        Dictionary with audio information.
    """
    duration_sec = len(audio) / sample_rate if sample_rate > 0 else 0
    peak = np.abs(audio).max() if len(audio) > 0 else 0

    return {
        "samples": len(audio),
        "channels": 1 if audio.ndim == 1 else audio.shape[1] if audio.ndim == 2 else None,
        "sample_rate": sample_rate,
        "duration_sec": duration_sec,
        "dtype": str(audio.dtype),
        "peak_amplitude": float(peak),
        "rms": float(np.sqrt(np.mean(audio ** 2))) if len(audio) > 0 else 0,
    }
