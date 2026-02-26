"""ASR backend protocol and error definitions.

This module defines the interface that all ASR backends must implement,
allowing different model backends to be swapped without changing
the rest of the application.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Any, Callable, Optional, Protocol, runtime_checkable

import numpy as np


class ASRState(Enum):
    """ASR engine state."""

    UNINITIALIZED = "uninitialized"
    DOWNLOADING = "downloading"
    LOADING = "loading"
    READY = "ready"
    ERROR = "error"


@dataclass
class TranscriptionResult:
    """Result of transcription."""

    text: str
    language: Optional[str] = None
    confidence: Optional[float] = None
    duration_ms: int = 0

    def to_dict(self) -> dict[str, Any]:
        """Convert to API response format."""
        result: dict[str, Any] = {"text": self.text}
        if self.language:
            result["language"] = self.language
        if self.confidence is not None:
            result["confidence"] = self.confidence
        if self.duration_ms > 0:
            result["duration_ms"] = self.duration_ms
        return result


@dataclass
class InitProgress:
    """Progress during initialization."""

    state: str
    detail: str = ""
    progress: Optional[dict[str, Any]] = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to event format."""
        result: dict[str, Any] = {"state": self.state}
        if self.detail:
            result["detail"] = self.detail
        if self.progress:
            result["progress"] = self.progress
        return result


# Progress callback type
ProgressCallback = Callable[[InitProgress], None]


class ASRError(Exception):
    """Base exception for ASR errors."""

    code: str = "E_ASR"

    def __init__(self, message: str, code: Optional[str] = None):
        super().__init__(message)
        if code:
            self.code = code


class ModelNotFoundError(ASRError):
    """Model not found in cache or registry."""

    code = "E_MODEL_NOT_FOUND"


class ModelLoadError(ASRError):
    """Error loading model into memory."""

    code = "E_MODEL_LOAD"


class DeviceUnavailableError(ASRError):
    """Requested device (e.g., CUDA) not available."""

    code = "E_DEVICE_UNAVAILABLE"

    def __init__(self, message: str, requested_device: str):
        super().__init__(message)
        self.requested_device = requested_device


class TranscriptionError(ASRError):
    """Error during transcription."""

    code = "E_TRANSCRIPTION"


class NotInitializedError(ASRError):
    """ASR backend not initialized."""

    code = "E_NOT_INITIALIZED"


class ASRBackend(ABC):
    """Formal async ASR backend interface for multi-backend dispatch.

    This interface is additive and intended for the model-family dispatch path
    where backends are initialized/transcribed via async RPC-oriented methods.
    """

    @abstractmethod
    async def initialize(
        self,
        model_id: str,
        device_pref: str,
        language: str | None = None,
    ) -> None:
        """Initialize backend state for the selected model/device."""
        raise NotImplementedError

    @abstractmethod
    async def transcribe(
        self,
        audio_path: str,
        session_id: str | None = None,
        language: str | None = None,
    ) -> dict[str, Any]:
        """Transcribe the given audio file path."""
        raise NotImplementedError

    @abstractmethod
    async def get_status(self) -> dict[str, Any]:
        """Return status payload (for example: model_id/status/language)."""
        raise NotImplementedError

    @abstractmethod
    def supports_language(self, language: str) -> bool:
        """Return True when the backend supports forced language selection."""
        raise NotImplementedError


@runtime_checkable
class LegacyASRBackend(Protocol):
    """Legacy synchronous backend protocol used by the current ASR engine.

    Kept for brownfield compatibility while the async ASRBackend interface is
    rolled out to existing implementations.
    """

    def initialize(
        self,
        model_path: Path,
        device: str,
        progress_callback: Optional[ProgressCallback] = None,
    ) -> None:
        """Initialize the ASR backend with a model.

        Args:
            model_path: Path to the model directory.
            device: Device to use ("cpu" or "cuda").
            progress_callback: Optional callback for progress updates.

        Raises:
            ModelLoadError: If model cannot be loaded.
            DeviceUnavailableError: If requested device is not available.
        """
        ...

    def transcribe(self, audio: np.ndarray, sample_rate: int = 16000) -> TranscriptionResult:
        """Transcribe audio to text.

        Args:
            audio: Audio data as float32 array in range [-1, 1].
            sample_rate: Sample rate of the audio (default: 16000 Hz).

        Returns:
            TranscriptionResult with the transcribed text.

        Raises:
            NotInitializedError: If backend not initialized.
            TranscriptionError: If transcription fails.
        """
        ...

    def is_ready(self) -> bool:
        """Check if the backend is ready for transcription."""
        ...

    def get_device(self) -> str:
        """Get the device the model is loaded on."""
        ...

    def unload(self) -> None:
        """Unload the model and free resources."""
        ...
