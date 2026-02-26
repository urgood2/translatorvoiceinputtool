"""Whisper ASR backend using faster-whisper.

Supports language parameter for forced language transcription
and auto-detection mode.
"""

from __future__ import annotations

import time
from pathlib import Path
from typing import Any, Optional

import numpy as np

from ..protocol import log
from .base import (
    ASRState,
    DeviceUnavailableError,
    InitProgress,
    ModelLoadError,
    NotInitializedError,
    ProgressCallback,
    TranscriptionError,
    TranscriptionResult,
)

_FASTER_WHISPER_AVAILABLE: bool
try:
    from faster_whisper import WhisperModel as _WhisperModel  # noqa: F401

    _FASTER_WHISPER_AVAILABLE = True
except ImportError:
    _FASTER_WHISPER_AVAILABLE = False


class WhisperBackend:
    """ASR backend using faster-whisper for Whisper models.

    Supports explicit language selection and auto-detection.
    """

    def __init__(self) -> None:
        self._model: Any = None
        self._device: str = "cpu"
        self._state: ASRState = ASRState.UNINITIALIZED
        self._model_path: Optional[Path] = None
        self._language: Optional[str] = None  # None => auto-detect

    @property
    def language(self) -> Optional[str]:
        """Return the configured language, or None for auto-detect."""
        return self._language

    def set_language(self, language: Optional[str]) -> None:
        """Configure the transcription language.

        Args:
            language: ISO 639-1 code (e.g. "en", "de") or "auto" for detection.
                      None also means auto-detect.

        Raises:
            ValueError: If language code is not valid.
        """
        if language is None or language == "auto":
            self._language = None
            log("Whisper language configured: auto-detect")
            return

        code = language.strip().lower()
        if len(code) != 2 or not code.isalpha():
            raise ValueError(
                f"Invalid language code '{language}'. "
                "Use an ISO 639-1 code (e.g. 'en') or 'auto'."
            )
        self._language = code
        log(f"Whisper language configured: {code}")

    def initialize(
        self,
        model_path: Path,
        device: str,
        progress_callback: Optional[ProgressCallback] = None,
    ) -> None:
        """Load the Whisper model.

        Args:
            model_path: Path to the model directory or model size string.
            device: "cpu", "cuda", or "auto".
            progress_callback: Optional progress emitter.

        Raises:
            ModelLoadError: If faster-whisper is not installed or model loading fails.
            DeviceUnavailableError: If CUDA is requested but unavailable.
        """
        if not _FASTER_WHISPER_AVAILABLE:
            self._state = ASRState.ERROR
            raise ModelLoadError(
                "faster-whisper is not installed. "
                "Install with: pip install faster-whisper"
            )

        if progress_callback:
            progress_callback(
                InitProgress(state="loading_model", detail="Loading Whisper model...")
            )

        # Resolve device
        compute_type = "float32"
        if device == "cuda":
            try:
                import torch

                if not torch.cuda.is_available():
                    raise DeviceUnavailableError("CUDA not available", "cuda")
                compute_type = "float16"
            except ImportError:
                raise DeviceUnavailableError(
                    "PyTorch required for CUDA device selection", "cuda"
                )

        self._state = ASRState.LOADING

        try:
            from faster_whisper import WhisperModel

            start = time.time()
            self._model = WhisperModel(
                str(model_path),
                device=device,
                compute_type=compute_type,
            )
            elapsed = time.time() - start
            log(f"Whisper model loaded in {elapsed:.2f}s on {device}")

            self._device = device
            self._model_path = model_path
            self._state = ASRState.READY

            if progress_callback:
                progress_callback(
                    InitProgress(state="ready", detail=f"Whisper ready on {device}")
                )

        except Exception as e:
            self._state = ASRState.ERROR
            raise ModelLoadError(f"Failed to load Whisper model: {e}") from e

    def transcribe(
        self, audio: np.ndarray, sample_rate: int = 16000
    ) -> TranscriptionResult:
        """Transcribe audio using Whisper.

        Args:
            audio: Float32 mono audio array.
            sample_rate: Sample rate (16 000 Hz expected).

        Returns:
            TranscriptionResult.
        """
        if not self.is_ready():
            raise NotInitializedError("Whisper not initialized. Call initialize() first.")

        try:
            start = time.time()
            segments, info = self._model.transcribe(
                audio,
                language=self._language,
                beam_size=5,
            )
            text = " ".join(seg.text.strip() for seg in segments)
            duration_ms = int((time.time() - start) * 1000)

            return TranscriptionResult(
                text=text,
                language=info.language,
                confidence=info.language_probability,
                duration_ms=duration_ms,
            )
        except Exception as e:
            raise TranscriptionError(f"Whisper transcription failed: {e}") from e

    def is_ready(self) -> bool:
        return self._state == ASRState.READY and self._model is not None

    def get_device(self) -> str:
        return self._device

    def unload(self) -> None:
        if self._model is not None:
            del self._model
            self._model = None
        self._state = ASRState.UNINITIALIZED
        self._model_path = None
        self._language = None
        log("Whisper model unloaded")
