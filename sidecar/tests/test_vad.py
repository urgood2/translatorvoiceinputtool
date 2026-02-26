"""Tests for VAD auto-stop detector module."""

from __future__ import annotations

from unittest.mock import patch

import numpy as np

from openvoicy_sidecar.vad import VadConfig, VadState, VoiceActivityDetector


def _speech_chunk(samples: int = 1600, amplitude: float = 0.1) -> np.ndarray:
    """Generate synthetic speech-like audio."""
    return np.full(samples, amplitude, dtype=np.float32)


def _silence_chunk(samples: int = 1600) -> np.ndarray:
    return np.zeros(samples, dtype=np.float32)


class TestVadConfig:
    def test_clamps_silence_and_min_speech_ranges(self):
        config = VadConfig(
            sample_rate=0,
            silence_ms=1,
            min_speech_ms=5,
            energy_threshold=-1.0,
            backend="bogus",
            webrtc_aggressiveness=8,
        )

        assert config.sample_rate == 1
        assert config.silence_ms == 400
        assert config.min_speech_ms == 100
        assert config.energy_threshold == 0.0
        assert config.backend == "auto"
        assert config.webrtc_aggressiveness == 3


class TestVoiceActivityDetector:
    def test_stays_waiting_during_initial_silence(self):
        detector = VoiceActivityDetector(
            VadConfig(sample_rate=16000, min_speech_ms=250, silence_ms=1200)
        )

        state = detector.feed_audio(_silence_chunk())
        assert state == VadState.WAITING_FOR_SPEECH
        assert detector.speech_ms == 0.0

    def test_detects_speech_and_then_auto_stop_after_silence_window(self):
        detector = VoiceActivityDetector(
            VadConfig(sample_rate=16000, min_speech_ms=200, silence_ms=400, energy_threshold=0.01)
        )

        # Two speech chunks (~200ms total at 16kHz with 1600-sample chunks).
        assert detector.feed_audio(_speech_chunk()) == VadState.SPEECH
        assert detector.feed_audio(_speech_chunk()) == VadState.SPEECH

        # Trailing silence should eventually trigger auto-stop.
        assert detector.feed_audio(_silence_chunk()) == VadState.SILENCE
        assert detector.feed_audio(_silence_chunk()) == VadState.SILENCE
        assert detector.feed_audio(_silence_chunk()) == VadState.SILENCE
        assert detector.feed_audio(_silence_chunk()) == VadState.AUTO_STOP
        assert detector.feed_audio(_silence_chunk()) == VadState.AUTO_STOP

    def test_does_not_trigger_auto_stop_when_speech_shorter_than_minimum(self):
        detector = VoiceActivityDetector(
            VadConfig(sample_rate=16000, min_speech_ms=300, silence_ms=200, energy_threshold=0.01)
        )

        # Only 100ms speech, below min speech threshold.
        assert detector.feed_audio(_speech_chunk()) == VadState.SPEECH
        assert detector.feed_audio(_silence_chunk()) == VadState.WAITING_FOR_SPEECH
        assert detector.feed_audio(_silence_chunk()) == VadState.WAITING_FOR_SPEECH
        assert detector.feed_audio(_silence_chunk()) == VadState.WAITING_FOR_SPEECH

    def test_reset_clears_accumulators_and_state(self):
        detector = VoiceActivityDetector(
            VadConfig(sample_rate=16000, min_speech_ms=100, silence_ms=400, energy_threshold=0.01)
        )

        detector.feed_audio(_speech_chunk())
        detector.feed_audio(_silence_chunk())
        detector.feed_audio(_silence_chunk())
        detector.feed_audio(_silence_chunk())
        detector.feed_audio(_silence_chunk())
        assert detector.state == VadState.AUTO_STOP

        detector.reset()
        assert detector.state == VadState.WAITING_FOR_SPEECH
        assert detector.speech_ms == 0.0
        assert detector.silence_ms == 0.0

    def test_handles_stereo_input_by_downmixing(self):
        detector = VoiceActivityDetector(
            VadConfig(sample_rate=16000, min_speech_ms=100, silence_ms=200, energy_threshold=0.01)
        )
        mono = _speech_chunk()
        stereo = np.stack([mono, mono * 0.8], axis=1)

        state = detector.feed_audio(stereo)
        assert state == VadState.SPEECH

    def test_webrtc_backend_request_falls_back_when_module_missing(self):
        with patch("openvoicy_sidecar.vad.importlib.import_module", side_effect=ImportError):
            detector = VoiceActivityDetector(VadConfig(backend="webrtcvad"))
        assert detector.backend == "energy"
