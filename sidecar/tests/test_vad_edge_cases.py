"""VAD edge case tests: short utterances, background noise, disabled baseline."""

from __future__ import annotations

import numpy as np

from openvoicy_sidecar.vad import VadConfig, VadState, VoiceActivityDetector


# === Helpers ===

SAMPLE_RATE = 16000
# 100ms chunk at 16kHz = 1600 samples
CHUNK_SAMPLES = 1600


def _speech_chunk(amplitude: float = 0.1) -> np.ndarray:
    """Loud-enough chunk to be detected as speech."""
    return np.full(CHUNK_SAMPLES, amplitude, dtype=np.float32)


def _silence_chunk() -> np.ndarray:
    return np.zeros(CHUNK_SAMPLES, dtype=np.float32)


def _noise_chunk(amplitude: float = 0.005) -> np.ndarray:
    """Low-level background noise (below default energy threshold)."""
    rng = np.random.default_rng(42)
    return (rng.random(CHUNK_SAMPLES, dtype=np.float32) - 0.5) * 2 * amplitude


def _feed_n(detector: VoiceActivityDetector, chunk: np.ndarray, n: int) -> VadState:
    """Feed the same chunk n times, return last state."""
    state = VadState.WAITING_FOR_SPEECH
    for _ in range(n):
        state = detector.feed_audio(chunk)
    return state


# === Short utterance tests ===


class TestShortUtterances:
    """Edge cases around min_speech_ms threshold."""

    def test_utterance_shorter_than_min_speech_does_not_auto_stop(self):
        """1. Speech < min_speech_ms → silence resets to WAITING, never AUTO_STOP."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                min_speech_ms=300,
                silence_ms=400,
                energy_threshold=0.01,
            )
        )
        # 200ms of speech (2 chunks of 100ms)
        _feed_n(detector, _speech_chunk(), 2)
        assert detector.speech_ms < 300

        # Now lots of silence — should stay WAITING, not AUTO_STOP.
        for _ in range(20):
            state = detector.feed_audio(_silence_chunk())
            assert state == VadState.WAITING_FOR_SPEECH

    def test_utterance_at_exactly_min_speech_triggers_auto_stop(self):
        """2. Speech >= min_speech_ms, then silence_ms of silence → AUTO_STOP."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                min_speech_ms=200,
                silence_ms=400,
                energy_threshold=0.01,
            )
        )
        # Exactly 200ms speech (2 × 100ms chunks)
        _feed_n(detector, _speech_chunk(), 2)
        assert detector.speech_ms >= 200

        # 400ms silence → AUTO_STOP
        _feed_n(detector, _silence_chunk(), 3)
        state = detector.feed_audio(_silence_chunk())
        assert state == VadState.AUTO_STOP

    def test_short_pause_within_speech_does_not_auto_stop(self):
        """3. Short gap (< silence_ms) between speech → SILENCE then back to SPEECH."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                min_speech_ms=200,
                silence_ms=1200,
                energy_threshold=0.01,
            )
        )
        # Speech → short pause → more speech
        _feed_n(detector, _speech_chunk(), 3)  # 300ms speech
        state = _feed_n(detector, _silence_chunk(), 2)  # 200ms silence
        assert state == VadState.SILENCE

        # Resume speaking — goes back to SPEECH
        state = detector.feed_audio(_speech_chunk())
        assert state == VadState.SPEECH

    def test_long_pause_after_sufficient_speech_triggers_auto_stop(self):
        """4. Long silence (>= silence_ms) after enough speech → AUTO_STOP."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                min_speech_ms=250,
                silence_ms=400,
                energy_threshold=0.01,
            )
        )
        # 300ms speech
        _feed_n(detector, _speech_chunk(), 3)
        assert detector.speech_ms >= 250

        # 500ms silence (5 × 100ms chunks) exceeds silence_ms=400
        for _ in range(5):
            state = detector.feed_audio(_silence_chunk())
        assert state == VadState.AUTO_STOP

    def test_quick_speech_bursts_treated_as_single_utterance(self):
        """5. Speech–pause–speech below silence_ms → continuous utterance."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                min_speech_ms=200,
                silence_ms=1200,
                energy_threshold=0.01,
            )
        )
        # Burst 1: 200ms speech
        _feed_n(detector, _speech_chunk(), 2)
        # Short 300ms silence
        _feed_n(detector, _silence_chunk(), 3)
        # Burst 2: 200ms more speech
        _feed_n(detector, _speech_chunk(), 2)

        # Total accumulated speech should be >= 400ms
        assert detector.speech_ms >= 400
        assert detector.state == VadState.SPEECH

    def test_default_config_works_for_typical_dictation(self):
        """6. Default config: silence_ms=1200, min_speech_ms=250 works for dictation."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                energy_threshold=0.01,
            )
        )
        # Simulate typical dictation: ~1s speech → 1.2s silence → auto-stop
        _feed_n(detector, _speech_chunk(), 10)  # 1000ms speech
        assert detector.speech_ms >= 250

        # Feed silence until auto-stop
        states = []
        for i in range(20):
            state = detector.feed_audio(_silence_chunk())
            states.append(state)
            if state == VadState.AUTO_STOP:
                break

        assert VadState.AUTO_STOP in states
        # Auto-stop should happen around 12-13 chunks (1200-1300ms)
        auto_stop_idx = states.index(VadState.AUTO_STOP)
        assert 10 <= auto_stop_idx <= 14  # ~1200ms at 100ms/chunk


# === Background noise tests ===


class TestBackgroundNoise:
    """VAD behavior under background noise conditions."""

    def test_low_noise_does_not_trigger_speech(self):
        """Background noise below threshold → stays WAITING."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                min_speech_ms=250,
                silence_ms=1200,
                energy_threshold=0.015,
            )
        )
        # Many chunks of low noise
        state = _feed_n(detector, _noise_chunk(amplitude=0.005), 50)
        assert state == VadState.WAITING_FOR_SPEECH
        assert detector.speech_ms == 0.0

    def test_speech_detected_above_noise_floor(self):
        """Speech well above noise floor → SPEECH, then AUTO_STOP on silence."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                min_speech_ms=200,
                silence_ms=400,
                energy_threshold=0.01,
            )
        )
        # Noise → speech → noise → auto stop
        _feed_n(detector, _noise_chunk(amplitude=0.005), 5)
        assert detector.state == VadState.WAITING_FOR_SPEECH

        _feed_n(detector, _speech_chunk(amplitude=0.1), 3)
        assert detector.state == VadState.SPEECH

        state = _feed_n(detector, _noise_chunk(amplitude=0.005), 5)
        assert state == VadState.AUTO_STOP

    def test_noise_after_speech_below_min_resets_to_waiting(self):
        """Short speech + noise → WAITING (not SILENCE/AUTO_STOP)."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                min_speech_ms=300,
                silence_ms=400,
                energy_threshold=0.01,
            )
        )
        # 100ms speech (below min)
        detector.feed_audio(_speech_chunk())
        # Then noise (counts as silence)
        state = _feed_n(detector, _noise_chunk(amplitude=0.005), 10)
        assert state == VadState.WAITING_FOR_SPEECH


# === Disabled VAD baseline ===


class TestDisabledBaseline:
    """VAD behavior when effectively disabled or with extreme configs."""

    def test_auto_stop_never_fires_with_very_high_silence_ms(self):
        """With silence_ms at max (5000ms), auto-stop is very delayed."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                min_speech_ms=100,
                silence_ms=5000,
                energy_threshold=0.01,
            )
        )
        _feed_n(detector, _speech_chunk(), 2)

        # Feed 4 seconds of silence (40 × 100ms) — still below 5000ms
        state = _feed_n(detector, _silence_chunk(), 40)
        assert state == VadState.SILENCE  # not AUTO_STOP yet

    def test_all_silence_never_triggers_auto_stop(self):
        """Pure silence without prior speech → stays WAITING forever."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                min_speech_ms=100,
                silence_ms=400,
                energy_threshold=0.01,
            )
        )
        state = _feed_n(detector, _silence_chunk(), 100)
        assert state == VadState.WAITING_FOR_SPEECH

    def test_reset_allows_reuse_for_new_session(self):
        """After reset, detector behaves as fresh for new recording session."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                min_speech_ms=100,
                silence_ms=400,
                energy_threshold=0.01,
            )
        )
        # Drive to AUTO_STOP
        _feed_n(detector, _speech_chunk(), 2)
        _feed_n(detector, _silence_chunk(), 5)
        assert detector.state == VadState.AUTO_STOP

        # Reset and verify fresh state
        detector.reset()
        assert detector.state == VadState.WAITING_FOR_SPEECH
        assert detector.speech_ms == 0.0
        assert detector.silence_ms == 0.0

        # New session should work from scratch
        detector.feed_audio(_speech_chunk())
        assert detector.state == VadState.SPEECH

    def test_empty_chunks_do_not_affect_state(self):
        """Empty audio chunks are ignored."""
        detector = VoiceActivityDetector(
            VadConfig(
                sample_rate=SAMPLE_RATE,
                energy_threshold=0.01,
            )
        )
        empty = np.array([], dtype=np.float32)
        state = detector.feed_audio(empty)
        assert state == VadState.WAITING_FOR_SPEECH
        assert detector.speech_ms == 0.0
