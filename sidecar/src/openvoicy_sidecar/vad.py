"""Voice activity detection helpers for optional auto-stop behavior.

This module provides a lightweight voice activity detector that can be fed
audio chunks during recording. It defaults to deterministic energy-based
detection and can optionally use `webrtcvad` or `silero_vad` when available.
"""

from __future__ import annotations

import importlib
from dataclasses import dataclass
from enum import Enum
from typing import Any

import numpy as np

from .protocol import log

DEFAULT_SAMPLE_RATE = 16000
DEFAULT_SILENCE_MS = 1200
DEFAULT_MIN_SPEECH_MS = 250
DEFAULT_ENERGY_THRESHOLD = 0.015

MIN_SILENCE_MS = 400
MAX_SILENCE_MS = 5000
MIN_MIN_SPEECH_MS = 100
MAX_MIN_SPEECH_MS = 2000

_SUPPORTED_BACKENDS = {"auto", "energy", "webrtcvad", "silero"}
_WEBRTC_SUPPORTED_SAMPLE_RATES = {8000, 16000, 32000, 48000}


class VadState(Enum):
    """Detector output state for each processed chunk."""

    WAITING_FOR_SPEECH = "waiting_for_speech"
    SPEECH = "speech"
    SILENCE = "silence"
    AUTO_STOP = "auto_stop"


@dataclass
class VadConfig:
    """Config for voice activity detection behavior."""

    sample_rate: int = DEFAULT_SAMPLE_RATE
    silence_ms: int = DEFAULT_SILENCE_MS
    min_speech_ms: int = DEFAULT_MIN_SPEECH_MS
    energy_threshold: float = DEFAULT_ENERGY_THRESHOLD
    backend: str = "auto"
    webrtc_aggressiveness: int = 2

    def __post_init__(self) -> None:
        self.sample_rate = max(1, int(self.sample_rate))
        self.silence_ms = _clamp(int(self.silence_ms), MIN_SILENCE_MS, MAX_SILENCE_MS)
        self.min_speech_ms = _clamp(
            int(self.min_speech_ms),
            MIN_MIN_SPEECH_MS,
            MAX_MIN_SPEECH_MS,
        )
        self.energy_threshold = max(0.0, float(self.energy_threshold))
        self.backend = self.backend.lower().strip()
        if self.backend not in _SUPPORTED_BACKENDS:
            log(f"Unsupported VAD backend '{self.backend}', falling back to auto")
            self.backend = "auto"
        self.webrtc_aggressiveness = _clamp(int(self.webrtc_aggressiveness), 0, 3)


def _clamp(value: int, minimum: int, maximum: int) -> int:
    return max(minimum, min(maximum, value))


class VoiceActivityDetector:
    """Chunk-based VAD with optional backend acceleration.

    Call `feed_audio()` with PCM chunks from the recording stream. The detector
    returns a `VadState` and transitions to `AUTO_STOP` only after:
    1) at least `min_speech_ms` of speech is observed, and
    2) trailing silence reaches `silence_ms`.
    """

    def __init__(self, config: VadConfig | None = None):
        self._config = config or VadConfig()

        self._state = VadState.WAITING_FOR_SPEECH
        self._speech_ms = 0.0
        self._silence_ms = 0.0
        self._backend_name, self._backend_impl = self._initialize_backend()

    @property
    def state(self) -> VadState:
        return self._state

    @property
    def backend(self) -> str:
        return self._backend_name

    @property
    def speech_ms(self) -> float:
        return self._speech_ms

    @property
    def silence_ms(self) -> float:
        return self._silence_ms

    def reset(self) -> None:
        self._state = VadState.WAITING_FOR_SPEECH
        self._speech_ms = 0.0
        self._silence_ms = 0.0

    def feed_audio(self, chunk: np.ndarray) -> VadState:
        """Consume an audio chunk and return current VAD state."""
        if self._state == VadState.AUTO_STOP:
            return self._state

        audio = _normalize_chunk(chunk)
        if audio.size == 0:
            return self._state

        duration_ms = (audio.size * 1000.0) / self._config.sample_rate
        is_speech = self._detect_speech(audio)

        if is_speech:
            self._speech_ms += duration_ms
            self._silence_ms = 0.0
            self._state = VadState.SPEECH
            return self._state

        # Ignore silence before we have enough speech to avoid stopping on
        # short accidental noise blips.
        if self._speech_ms < self._config.min_speech_ms:
            self._silence_ms = 0.0
            self._state = VadState.WAITING_FOR_SPEECH
            return self._state

        self._silence_ms += duration_ms
        if self._silence_ms >= self._config.silence_ms:
            self._state = VadState.AUTO_STOP
        else:
            self._state = VadState.SILENCE
        return self._state

    def _initialize_backend(self) -> tuple[str, Any | None]:
        requested = self._config.backend

        if requested in {"auto", "webrtcvad"}:
            webrtc = self._load_webrtc()
            if webrtc is not None:
                return "webrtcvad", webrtc
            if requested == "webrtcvad":
                log("webrtcvad backend unavailable, falling back to energy VAD")

        if requested in {"auto", "silero"}:
            silero = self._load_silero()
            if silero is not None:
                return "silero", silero
            if requested == "silero":
                log("silero backend unavailable, falling back to energy VAD")

        return "energy", None

    def _load_webrtc(self) -> Any | None:
        if self._config.sample_rate not in _WEBRTC_SUPPORTED_SAMPLE_RATES:
            return None
        try:
            module = importlib.import_module("webrtcvad")
            return module.Vad(self._config.webrtc_aggressiveness)
        except Exception:
            return None

    def _load_silero(self) -> dict[str, Any] | None:
        try:
            module = importlib.import_module("silero_vad")
        except Exception:
            return None

        load_silero_vad = getattr(module, "load_silero_vad", None)
        get_speech_timestamps = getattr(module, "get_speech_timestamps", None)
        if not callable(load_silero_vad) or not callable(get_speech_timestamps):
            return None

        try:
            model = load_silero_vad()
        except Exception:
            return None

        return {
            "model": model,
            "get_speech_timestamps": get_speech_timestamps,
        }

    def _detect_speech(self, audio: np.ndarray) -> bool:
        if self._backend_name == "webrtcvad":
            result = self._detect_speech_webrtc(audio)
            if result is not None:
                return result
        elif self._backend_name == "silero":
            result = self._detect_speech_silero(audio)
            if result is not None:
                return result
        return self._detect_speech_energy(audio)

    def _detect_speech_energy(self, audio: np.ndarray) -> bool:
        rms = float(np.sqrt(np.mean(np.square(audio, dtype=np.float64))))
        return rms >= self._config.energy_threshold

    def _detect_speech_webrtc(self, audio: np.ndarray) -> bool | None:
        if self._backend_impl is None:
            return None

        frame_ms = 30
        frame_samples = int(self._config.sample_rate * frame_ms / 1000)
        if frame_samples <= 0:
            return None

        pcm16 = (np.clip(audio, -1.0, 1.0) * 32767.0).astype(np.int16)
        frame_count = pcm16.size // frame_samples
        if frame_count == 0:
            return None

        for index in range(frame_count):
            start = index * frame_samples
            stop = start + frame_samples
            frame = pcm16[start:stop]
            if self._backend_impl.is_speech(frame.tobytes(), self._config.sample_rate):
                return True
        return False

    def _detect_speech_silero(self, audio: np.ndarray) -> bool | None:
        if not isinstance(self._backend_impl, dict):
            return None

        get_speech_timestamps = self._backend_impl["get_speech_timestamps"]
        model = self._backend_impl["model"]
        try:
            speech = get_speech_timestamps(
                audio,
                model,
                sampling_rate=self._config.sample_rate,
            )
            return bool(speech)
        except Exception:
            # Disable backend after first hard failure to avoid repeated cost.
            self._backend_name = "energy"
            self._backend_impl = None
            return None


def _normalize_chunk(chunk: np.ndarray) -> np.ndarray:
    audio = np.asarray(chunk)
    if audio.size == 0:
        return np.array([], dtype=np.float32)

    # Downmix channels if needed.
    if audio.ndim > 1:
        if audio.shape[1] <= 8:
            audio = audio.mean(axis=1)
        else:
            audio = audio.reshape(-1)

    if audio.dtype == np.float32:
        return audio
    if audio.dtype == np.float64:
        return audio.astype(np.float32)
    if audio.dtype == np.int16:
        return (audio.astype(np.float32) / 32768.0)
    if audio.dtype == np.int32:
        return (audio.astype(np.float32) / 2147483648.0)
    if audio.dtype == np.uint8:
        return ((audio.astype(np.float32) - 128.0) / 128.0)
    return audio.astype(np.float32)
