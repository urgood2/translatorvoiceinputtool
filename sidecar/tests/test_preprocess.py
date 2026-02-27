"""Tests for audio preprocessing pipeline."""

from __future__ import annotations

import time

import numpy as np
import pytest

from openvoicy_sidecar.preprocess import (
    DEFAULT_MIN_AUDIO_MS,
    DEFAULT_SILENCE_THRESHOLD_DB,
    TARGET_DTYPE,
    TARGET_SAMPLE_RATE,
    PreprocessAudioConfig,
    PreprocessConfig,
    compute_rms_energy,
    convert_to_float32,
    db_to_linear,
    downmix_to_mono,
    get_audio_info,
    peak_clamp,
    peak_normalize,
    preprocess_audio,
    preprocess,
    remove_dc_offset,
    resample,
    trim_silence,
)


# === Fixtures ===


@pytest.fixture
def sine_wave_16k():
    """Generate a 1-second 440Hz sine wave at 16kHz."""
    t = np.linspace(0, 1, 16000, dtype=np.float32)
    return np.sin(2 * np.pi * 440 * t)


@pytest.fixture
def sine_wave_44k():
    """Generate a 1-second 440Hz sine wave at 44.1kHz."""
    t = np.linspace(0, 1, 44100, dtype=np.float32)
    return np.sin(2 * np.pi * 440 * t)


@pytest.fixture
def stereo_audio():
    """Generate stereo audio with different L/R content."""
    left = np.sin(2 * np.pi * 440 * np.linspace(0, 1, 16000))  # 440Hz
    right = np.sin(2 * np.pi * 880 * np.linspace(0, 1, 16000))  # 880Hz
    return np.stack([left, right], axis=1).astype(np.float32)


@pytest.fixture
def audio_with_silence():
    """Generate audio with leading and trailing silence."""
    silence = np.zeros(8000, dtype=np.float32)  # 0.5s silence
    speech = np.sin(2 * np.pi * 440 * np.linspace(0, 1, 16000, dtype=np.float32))
    speech *= 0.5  # Reduce amplitude
    return np.concatenate([silence, speech, silence])


# === Unit Tests: convert_to_float32 ===


class TestConvertToFloat32:
    """Tests for dtype conversion."""

    def test_float32_passthrough(self):
        """Should return float32 unchanged."""
        audio = np.array([0.5, -0.5], dtype=np.float32)
        result = convert_to_float32(audio)
        assert result.dtype == np.float32
        np.testing.assert_array_equal(result, audio)

    def test_float64_to_float32(self):
        """Should convert float64 to float32."""
        audio = np.array([0.5, -0.5], dtype=np.float64)
        result = convert_to_float32(audio)
        assert result.dtype == np.float32
        np.testing.assert_array_almost_equal(result, [0.5, -0.5])

    def test_int16_to_float32(self):
        """Should convert int16 to float32 in [-1, 1]."""
        audio = np.array([0, 16384, -16384, 32767], dtype=np.int16)
        result = convert_to_float32(audio)
        assert result.dtype == np.float32
        assert result[0] == pytest.approx(0.0)
        assert result[1] == pytest.approx(0.5, abs=0.01)
        assert result[2] == pytest.approx(-0.5, abs=0.01)
        assert result[3] == pytest.approx(1.0, abs=0.01)

    def test_int32_to_float32(self):
        """Should convert int32 to float32 in [-1, 1]."""
        audio = np.array([0, 1073741824], dtype=np.int32)  # 0, 2^30
        result = convert_to_float32(audio)
        assert result.dtype == np.float32
        assert result[0] == pytest.approx(0.0)
        assert result[1] == pytest.approx(0.5, abs=0.01)

    def test_uint8_to_float32(self):
        """Should convert uint8 (centered at 128) to float32."""
        audio = np.array([0, 128, 255], dtype=np.uint8)
        result = convert_to_float32(audio)
        assert result.dtype == np.float32
        assert result[0] == pytest.approx(-1.0)
        assert result[1] == pytest.approx(0.0)
        assert result[2] == pytest.approx(1.0, abs=0.01)


# === Unit Tests: downmix_to_mono ===


class TestDownmixToMono:
    """Tests for channel downmixing."""

    def test_mono_passthrough(self):
        """Should return mono unchanged."""
        audio = np.array([0.1, 0.2, 0.3], dtype=np.float32)
        result = downmix_to_mono(audio)
        np.testing.assert_array_equal(result, audio)

    def test_stereo_to_mono(self, stereo_audio):
        """Should average stereo channels."""
        result = downmix_to_mono(stereo_audio)
        assert result.ndim == 1
        assert len(result) == 16000
        # Check a few samples - should be average of L and R
        expected = stereo_audio.mean(axis=1)
        np.testing.assert_array_almost_equal(result, expected, decimal=5)

    def test_51_surround_to_mono(self):
        """Should handle 5.1 surround (6 channels)."""
        # Create 6-channel audio (L, R, C, LFE, Ls, Rs)
        channels = 6
        samples = 1000
        audio = np.random.random((samples, channels)).astype(np.float32)

        result = downmix_to_mono(audio)
        assert result.ndim == 1
        assert len(result) == samples
        np.testing.assert_array_almost_equal(result, audio.mean(axis=1), decimal=5)

    def test_channels_first_format(self):
        """Should handle (channels, samples) format."""
        # Create (2, 1000) shaped audio
        audio = np.array([
            np.ones(1000) * 0.5,  # Left
            np.ones(1000) * -0.5,  # Right
        ], dtype=np.float32)

        result = downmix_to_mono(audio)
        assert result.ndim == 1
        assert len(result) == 1000
        # Average of 0.5 and -0.5 should be 0.0
        np.testing.assert_array_almost_equal(result, np.zeros(1000), decimal=5)


# === Unit Tests: resample ===


class TestResample:
    """Tests for sample rate conversion."""

    def test_no_resample_at_target_rate(self, sine_wave_16k):
        """Should return unchanged if already at target rate."""
        result = resample(sine_wave_16k, 16000)
        np.testing.assert_array_equal(result, sine_wave_16k)

    def test_resample_44k_to_16k(self, sine_wave_44k):
        """Should resample 44.1kHz to 16kHz."""
        result = resample(sine_wave_44k, 44100, 16000)

        # Check output length (approximately 16000/44100 ratio)
        expected_length = int(44100 * 16000 / 44100)  # ~16000
        assert abs(len(result) - expected_length) < 10  # Allow small rounding

        # Check it's still a sine wave (peak amplitude preserved approximately)
        assert np.abs(result).max() > 0.9

    def test_resample_48k_to_16k(self):
        """Should resample 48kHz to 16kHz."""
        t = np.linspace(0, 1, 48000, dtype=np.float32)
        audio = np.sin(2 * np.pi * 440 * t)

        result = resample(audio, 48000, 16000)

        # Should be exactly 16000 samples (48000 * 16/48 = 16000)
        assert len(result) == 16000
        assert result.dtype == TARGET_DTYPE

    def test_resample_preserves_content(self):
        """Should preserve audio content after resample."""
        # Create impulse at 44.1kHz
        audio = np.zeros(4410, dtype=np.float32)
        audio[2205] = 1.0  # Impulse at middle

        result = resample(audio, 44100, 16000)

        # Polyphase resampling spreads impulse energy, so peak will be reduced
        # but should still be significant (> 0.1)
        assert result.max() > 0.1


# === Unit Tests: DC offset removal ===


class TestDCOffsetRemoval:
    """Tests for DC offset removal."""

    def test_remove_positive_offset(self):
        """Should remove positive DC offset."""
        audio = np.array([1.5, 1.6, 1.4, 1.5], dtype=np.float32)
        result = remove_dc_offset(audio)

        assert result.mean() == pytest.approx(0.0, abs=1e-6)
        assert result.dtype == TARGET_DTYPE

    def test_remove_negative_offset(self):
        """Should remove negative DC offset."""
        audio = np.array([-0.5, -0.4, -0.6, -0.5], dtype=np.float32)
        result = remove_dc_offset(audio)

        assert result.mean() == pytest.approx(0.0, abs=1e-6)

    def test_zero_offset_unchanged(self, sine_wave_16k):
        """Should not significantly alter zero-centered audio."""
        result = remove_dc_offset(sine_wave_16k)

        # Sine wave should be largely unchanged
        np.testing.assert_array_almost_equal(result, sine_wave_16k, decimal=4)


# === Unit Tests: peak_clamp ===


class TestPeakClamp:
    """Tests for peak clamping."""

    def test_clamp_positive_overflow(self):
        """Should clamp values > 1.0."""
        audio = np.array([0.5, 1.5, 2.0], dtype=np.float32)
        result = peak_clamp(audio)

        np.testing.assert_array_equal(result, [0.5, 1.0, 1.0])

    def test_clamp_negative_overflow(self):
        """Should clamp values < -1.0."""
        audio = np.array([-0.5, -1.5, -2.0], dtype=np.float32)
        result = peak_clamp(audio)

        np.testing.assert_array_equal(result, [-0.5, -1.0, -1.0])

    def test_no_clamp_in_range(self, sine_wave_16k):
        """Should not clamp audio already in range."""
        result = peak_clamp(sine_wave_16k)
        np.testing.assert_array_equal(result, sine_wave_16k)


# === Unit Tests: peak_normalize ===


class TestPeakNormalize:
    """Tests for peak normalization."""

    def test_normalize_quiet_audio(self):
        """Should scale quiet audio to peak = 1.0."""
        audio = np.array([0.0, 0.25, -0.25, 0.1], dtype=np.float32)
        result = peak_normalize(audio)

        assert np.abs(result).max() == pytest.approx(1.0)
        # Peak was at index 1 or 2 (0.25)
        assert result[1] == pytest.approx(1.0) or result[2] == pytest.approx(-1.0)

    def test_normalize_already_normalized(self):
        """Should not change already-normalized audio."""
        audio = np.array([0.0, 1.0, -0.5], dtype=np.float32)
        result = peak_normalize(audio)

        np.testing.assert_array_almost_equal(result, audio)

    def test_normalize_zero_audio(self):
        """Should handle all-zero audio."""
        audio = np.zeros(100, dtype=np.float32)
        result = peak_normalize(audio)

        np.testing.assert_array_equal(result, audio)

    def test_normalize_empty_audio(self):
        """Should handle empty audio without errors."""
        audio = np.array([], dtype=np.float32)
        result = peak_normalize(audio)

        assert result.dtype == np.float32
        assert len(result) == 0

    def test_custom_target_peak(self):
        """Should normalize to custom target peak."""
        audio = np.array([0.0, 0.5, -0.5], dtype=np.float32)
        result = peak_normalize(audio, target_peak=0.8)

        assert np.abs(result).max() == pytest.approx(0.8)


# === Unit Tests: silence trimming ===


class TestSilenceTrimming:
    """Tests for silence trimming."""

    def test_trim_leading_silence(self, audio_with_silence):
        """Should trim leading silence."""
        result = trim_silence(audio_with_silence, 16000)

        # Result should be shorter (silence removed)
        assert len(result) < len(audio_with_silence)

        # First non-zero sample should be near the beginning
        first_nonzero = np.argmax(np.abs(result) > 0.01)
        assert first_nonzero < 2000  # Within ~125ms

    def test_trim_trailing_silence(self, audio_with_silence):
        """Should trim trailing silence."""
        result = trim_silence(audio_with_silence, 16000)

        # Last non-zero sample should be near the end
        last_nonzero = len(result) - 1 - np.argmax(np.abs(result[::-1]) > 0.01)
        assert len(result) - last_nonzero < 2000  # Within ~125ms

    def test_preserve_with_padding(self, audio_with_silence):
        """Should preserve some padding around speech."""
        result = trim_silence(audio_with_silence, 16000, padding_ms=100)

        # Should have some leading samples before the speech
        # (100ms padding = 1600 samples at 16kHz)
        assert len(result) > 16000  # Speech duration

    def test_all_silence_returns_empty_when_long(self):
        """Should return empty array for long all-silence input."""
        silence = np.zeros(16000, dtype=np.float32)  # 1 second
        result = trim_silence(silence, 16000)

        assert len(result) == 0

    def test_all_silence_preserved_when_short(self):
        """Very short all-silence audio stays intact (min_audio_ms guard)."""
        min_samples = int(16000 * DEFAULT_MIN_AUDIO_MS / 1000)
        silence = np.zeros(min_samples, dtype=np.float32)
        result = trim_silence(silence, 16000)

        assert len(result) == min_samples

    def test_min_audio_ms_prevents_over_trimming(self):
        """Quiet speech shorter than min_audio_ms should not be trimmed away."""
        # Create quiet speech of 150ms (below default 200ms min)
        # surrounded by long silence.
        quiet_speech = np.sin(
            2 * np.pi * 440 * np.linspace(0, 0.15, int(16000 * 0.15), dtype=np.float32)
        ) * 0.001  # very quiet
        silence = np.zeros(16000, dtype=np.float32)
        audio = np.concatenate([silence, quiet_speech, silence])

        # With a very aggressive threshold the speech may get trimmed to
        # below min_audio_ms; the guard returns original in that case.
        result = trim_silence(audio, 16000, threshold_db=-10.0, min_audio_ms=500)
        assert len(result) == len(audio)

    def test_no_silence_unchanged(self, sine_wave_16k):
        """Should not trim if no silence to remove."""
        result = trim_silence(sine_wave_16k, 16000)

        # Should be approximately the same length (maybe slightly shorter due to padding)
        assert len(result) > len(sine_wave_16k) * 0.9


# === Unit Tests: full pipeline ===


class TestPreprocess:
    """Tests for full preprocessing pipeline."""

    def test_basic_pipeline(self, sine_wave_44k):
        """Should apply full pipeline."""
        result = preprocess(sine_wave_44k, 44100)

        assert result.dtype == TARGET_DTYPE
        # Should be resampled to ~16kHz length
        assert len(result) < len(sine_wave_44k)
        # Values should be in [-1, 1]
        assert result.min() >= -1.0
        assert result.max() <= 1.0

    def test_pipeline_with_config(self, audio_with_silence):
        """Should respect configuration."""
        config = PreprocessConfig(
            silence_trim_enabled=False,
            normalize_enabled=True,
        )
        result = preprocess(audio_with_silence, 16000, config)

        # Silence trim disabled, so length should be ~same
        assert len(result) > len(audio_with_silence) * 0.9
        # Normalize enabled, so peak should be 1.0
        assert np.abs(result).max() == pytest.approx(1.0)

    def test_pipeline_stereo_input(self, stereo_audio):
        """Should handle stereo input."""
        result = preprocess(stereo_audio, 16000)

        assert result.ndim == 1  # Mono output
        assert result.dtype == TARGET_DTYPE

    def test_pipeline_int16_input(self):
        """Should handle int16 input."""
        audio_int16 = (np.sin(2 * np.pi * 440 * np.linspace(0, 1, 16000)) * 16384).astype(np.int16)
        result = preprocess(audio_int16, 16000)

        assert result.dtype == TARGET_DTYPE
        assert result.min() >= -1.0
        assert result.max() <= 1.0

    def test_very_short_audio(self):
        """Should handle very short audio (< 100ms)."""
        short_audio = np.array([0.5, -0.5, 0.3, -0.3], dtype=np.float32)
        result = preprocess(short_audio, 16000)

        # Should not error
        assert result.dtype == TARGET_DTYPE

    def test_high_sample_rate(self):
        """Should handle very high sample rate (192kHz)."""
        t = np.linspace(0, 1, 192000, dtype=np.float32)
        audio = np.sin(2 * np.pi * 440 * t)

        result = preprocess(audio, 192000)

        # Should be resampled to ~16kHz
        assert 15000 < len(result) < 17000


# === Helper function tests ===


class TestHelperFunctions:
    """Tests for helper functions."""

    def test_db_to_linear(self):
        """Should convert dB to linear correctly."""
        assert db_to_linear(0) == pytest.approx(1.0)
        assert db_to_linear(-6) == pytest.approx(0.5, abs=0.01)
        assert db_to_linear(-20) == pytest.approx(0.1)
        assert db_to_linear(-40) == pytest.approx(0.01)

    def test_compute_rms_energy(self, sine_wave_16k):
        """Should compute RMS energy correctly."""
        rms = compute_rms_energy(sine_wave_16k, 1600)  # 100ms windows

        # Sine wave RMS should be ~0.707 * peak = ~0.707
        assert all(r > 0.5 for r in rms)
        assert all(r < 0.9 for r in rms)

    def test_get_audio_info(self, sine_wave_16k):
        """Should return correct audio info."""
        info = get_audio_info(sine_wave_16k, 16000)

        assert info["samples"] == 16000
        assert info["sample_rate"] == 16000
        assert info["duration_sec"] == pytest.approx(1.0)
        assert info["peak_amplitude"] == pytest.approx(1.0, abs=0.01)
        assert info["rms"] > 0.5  # Sine wave RMS


# === Skeleton pipeline tests ===


class TestPreprocessAudioSkeleton:
    """Tests for config-gated preprocess_audio behavior."""

    def test_normalize_step_gated_by_config(self):
        audio = np.array([0.1, -0.2, 0.15], dtype=np.float32)
        no_normalize = preprocess_audio(
            audio,
            PreprocessAudioConfig(
                input_sample_rate=16000,
                target_sample_rate=16000,
                normalize=False,
                trim_silence=False,
            ),
        )
        normalized = preprocess_audio(
            audio,
            PreprocessAudioConfig(
                input_sample_rate=16000,
                target_sample_rate=16000,
                normalize=True,
                trim_silence=False,
            ),
        )

        assert np.abs(no_normalize).max() < 0.3
        assert np.abs(normalized).max() == pytest.approx(1.0, abs=1e-4)

    def test_trim_silence_step_uses_audio_trim_silence_config(self, audio_with_silence):
        not_trimmed = preprocess_audio(
            audio_with_silence,
            {
                "input_sample_rate": 16000,
                "target_sample_rate": 16000,
                "normalize": False,
                "audio": {"trim_silence": False},
            },
        )
        trimmed = preprocess_audio(
            audio_with_silence,
            {
                "input_sample_rate": 16000,
                "target_sample_rate": 16000,
                "normalize": False,
                "audio": {"trim_silence": True},
            },
        )

        assert len(not_trimmed) == len(audio_with_silence)
        assert len(trimmed) < len(audio_with_silence)

    def test_resample_step_gated_by_target_sample_rate(self, sine_wave_44k):
        resampled = preprocess_audio(
            sine_wave_44k,
            {
                "input_sample_rate": 44100,
                "target_sample_rate": 16000,
                "normalize": False,
                "audio": {"trim_silence": False},
            },
        )
        not_resampled = preprocess_audio(
            sine_wave_44k,
            {
                "input_sample_rate": 44100,
                "target_sample_rate": 44100,
                "normalize": False,
                "audio": {"trim_silence": False},
            },
        )

        assert len(resampled) < len(sine_wave_44k)
        assert len(not_resampled) == len(sine_wave_44k)


# === Performance tests ===


class TestPerformance:
    """Performance tests for preprocessing."""

    def test_60_second_audio_under_1_second(self):
        """Should process 60s audio in < 1 second."""
        # 60 seconds at 44.1kHz
        audio = np.random.random(44100 * 60).astype(np.float32) * 2 - 1

        start = time.monotonic()
        result = preprocess(audio, 44100)
        elapsed = time.monotonic() - start

        assert elapsed < 1.0, f"Processing took {elapsed:.2f}s, expected < 1s"
        assert len(result) > 0


# === Golden test fixtures ===


class TestGoldenOutputs:
    """Tests comparing against known golden outputs."""

    def test_synthetic_sine_preserves_frequency(self):
        """Sine wave should preserve dominant frequency after processing."""
        # Create 1s of 440Hz sine at 44.1kHz
        t = np.linspace(0, 1, 44100, dtype=np.float32)
        audio = np.sin(2 * np.pi * 440 * t)

        result = preprocess(
            audio, 44100,
            PreprocessConfig(silence_trim_enabled=False, normalize_enabled=False)
        )

        # Check FFT for 440Hz peak
        fft = np.abs(np.fft.rfft(result))
        freqs = np.fft.rfftfreq(len(result), 1/16000)

        # Find peak frequency
        peak_idx = np.argmax(fft)
        peak_freq = freqs[peak_idx]

        # Should be close to 440Hz
        assert 400 < peak_freq < 480

    def test_stereo_mix_preserves_content(self):
        """Stereo mix should preserve content from both channels."""
        # Left: 440Hz, Right: 880Hz
        t = np.linspace(0, 0.5, 8000, dtype=np.float32)
        left = np.sin(2 * np.pi * 440 * t)
        right = np.sin(2 * np.pi * 880 * t)
        stereo = np.stack([left, right], axis=1).astype(np.float32)

        result = preprocess(
            stereo, 16000,
            PreprocessConfig(silence_trim_enabled=False, normalize_enabled=False)
        )

        # FFT should show peaks at both 440Hz and 880Hz
        fft = np.abs(np.fft.rfft(result))
        freqs = np.fft.rfftfreq(len(result), 1/16000)

        # Find indices for 440Hz and 880Hz (with tolerance)
        idx_440 = np.argmin(np.abs(freqs - 440))
        idx_880 = np.argmin(np.abs(freqs - 880))

        # Both should have significant energy
        assert fft[idx_440] > 0.1 * fft.max()
        assert fft[idx_880] > 0.1 * fft.max()

    def test_dc_offset_removed(self):
        """DC offset should be completely removed."""
        audio = np.ones(16000, dtype=np.float32) * 0.5  # Constant 0.5 (DC)
        audio += np.sin(2 * np.pi * 440 * np.linspace(0, 1, 16000)) * 0.1  # Small AC

        result = preprocess(
            audio, 16000,
            PreprocessConfig(silence_trim_enabled=False, normalize_enabled=False)
        )

        # Mean should be ~0
        assert np.abs(result.mean()) < 0.01

    def test_clipping_handled(self):
        """Clipped audio should be clamped to [-1, 1]."""
        # Audio with values outside [-1, 1]
        audio = np.array([0.5, 1.5, -1.5, 2.0, -2.0], dtype=np.float32)

        result = preprocess(
            audio, 16000,
            PreprocessConfig(silence_trim_enabled=False, normalize_enabled=False)
        )

        assert result.min() >= -1.0
        assert result.max() <= 1.0
