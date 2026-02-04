"""NeMo Parakeet TDT ASR backend.

This module implements the ASR backend using NVIDIA's Parakeet TDT model
via the NeMo framework. It supports both CPU and CUDA inference.
"""

from __future__ import annotations

import time
from pathlib import Path
from typing import Any, Optional

import numpy as np

from ..protocol import log
from .base import (
    ASRBackend,
    ASRState,
    DeviceUnavailableError,
    InitProgress,
    ModelLoadError,
    NotInitializedError,
    ProgressCallback,
    TranscriptionError,
    TranscriptionResult,
)


def check_cuda_available() -> bool:
    """Check if CUDA is available via PyTorch."""
    try:
        import torch

        return torch.cuda.is_available()
    except ImportError:
        return False


def select_device(device_pref: str) -> str:
    """Select device based on preference and availability.

    Args:
        device_pref: "auto", "cuda", or "cpu"

    Returns:
        Selected device ("cuda" or "cpu")

    Raises:
        DeviceUnavailableError: If requested device is not available
    """
    if device_pref == "cuda":
        if check_cuda_available():
            return "cuda"
        raise DeviceUnavailableError(
            "CUDA requested but not available. Install PyTorch with CUDA support.",
            "cuda",
        )
    elif device_pref == "cpu":
        return "cpu"
    else:  # auto
        return "cuda" if check_cuda_available() else "cpu"


class ParakeetBackend:
    """ASR backend using NVIDIA Parakeet TDT model.

    This backend uses the NeMo framework to load and run inference
    with the Parakeet TDT (Token Duration Transducer) model.
    """

    def __init__(self):
        self._model: Any = None
        self._device: str = "cpu"
        self._state: ASRState = ASRState.UNINITIALIZED
        self._model_path: Optional[Path] = None

    def initialize(
        self,
        model_path: Path,
        device: str,
        progress_callback: Optional[ProgressCallback] = None,
    ) -> None:
        """Initialize the Parakeet model.

        Args:
            model_path: Path to the model directory containing .nemo file.
            device: Device to use ("cpu" or "cuda").
            progress_callback: Optional callback for progress updates.
        """
        if progress_callback:
            progress_callback(
                InitProgress(state="loading_model", detail="Checking dependencies...")
            )

        # Check for required dependencies
        try:
            import torch
        except ImportError as e:
            self._state = ASRState.ERROR
            raise ModelLoadError(
                "PyTorch not installed. Install with: pip install torch"
            ) from e

        try:
            import nemo.collections.asr as nemo_asr
        except ImportError as e:
            self._state = ASRState.ERROR
            raise ModelLoadError(
                "NeMo not installed. Install with: pip install nemo_toolkit[asr]"
            ) from e

        # Validate device
        if device == "cuda" and not torch.cuda.is_available():
            raise DeviceUnavailableError("CUDA not available", "cuda")

        # Find the .nemo file
        nemo_file = self._find_nemo_file(model_path)
        if not nemo_file:
            self._state = ASRState.ERROR
            raise ModelLoadError(f"No .nemo file found in {model_path}")

        if progress_callback:
            progress_callback(
                InitProgress(
                    state="loading_model",
                    detail=f"Loading model from {nemo_file.name}...",
                )
            )

        self._state = ASRState.LOADING

        try:
            log(f"Loading Parakeet model from {nemo_file}")
            start_time = time.time()

            # Load the model
            # NeMo's restore_from handles .nemo archives
            self._model = nemo_asr.models.ASRModel.restore_from(
                str(nemo_file),
                map_location=device,
            )

            # Move to device and set to eval mode
            self._model = self._model.to(device)
            self._model.eval()

            # Disable gradient computation for inference
            for param in self._model.parameters():
                param.requires_grad = False

            load_time = time.time() - start_time
            log(f"Model loaded in {load_time:.2f}s on {device}")

            self._device = device
            self._model_path = model_path
            self._state = ASRState.READY

            if progress_callback:
                progress_callback(
                    InitProgress(
                        state="ready",
                        detail=f"Model ready on {device}",
                    )
                )

        except Exception as e:
            self._state = ASRState.ERROR
            raise ModelLoadError(f"Failed to load model: {e}") from e

    def _find_nemo_file(self, model_path: Path) -> Optional[Path]:
        """Find the .nemo file in the model directory."""
        if model_path.is_file() and model_path.suffix == ".nemo":
            return model_path

        if model_path.is_dir():
            nemo_files = list(model_path.glob("*.nemo"))
            if nemo_files:
                return nemo_files[0]

        return None

    def transcribe(self, audio: np.ndarray, sample_rate: int = 16000) -> TranscriptionResult:
        """Transcribe audio using the Parakeet model.

        Args:
            audio: Audio data as float32 array, expected to be 16kHz mono.
            sample_rate: Sample rate (should be 16000 for Parakeet).

        Returns:
            TranscriptionResult with transcribed text.
        """
        if not self.is_ready():
            raise NotInitializedError("Model not initialized. Call initialize() first.")

        if sample_rate != 16000:
            log(f"Warning: Parakeet expects 16kHz audio, got {sample_rate}Hz")

        try:
            import torch

            start_time = time.time()

            # Ensure audio is the right shape and type
            if audio.ndim > 1:
                audio = audio.flatten()
            audio = audio.astype(np.float32)

            # NeMo expects audio as a list of arrays or paths
            # Using transcribe() method which handles preprocessing
            with torch.no_grad():
                # The transcribe method can take numpy arrays directly
                transcriptions = self._model.transcribe([audio])

            # Get the transcription text
            if isinstance(transcriptions, list) and len(transcriptions) > 0:
                text = transcriptions[0]
                # Handle case where transcriptions returns list of tuples/objects
                if hasattr(text, "text"):
                    text = text.text
                elif isinstance(text, tuple):
                    text = text[0]
            else:
                text = ""

            duration_ms = int((time.time() - start_time) * 1000)

            return TranscriptionResult(
                text=str(text).strip(),
                duration_ms=duration_ms,
            )

        except Exception as e:
            raise TranscriptionError(f"Transcription failed: {e}") from e

    def is_ready(self) -> bool:
        """Check if the model is loaded and ready."""
        return self._state == ASRState.READY and self._model is not None

    def get_device(self) -> str:
        """Get the device the model is running on."""
        return self._device

    def get_state(self) -> ASRState:
        """Get the current state."""
        return self._state

    def unload(self) -> None:
        """Unload the model and free memory."""
        if self._model is not None:
            try:
                import torch

                # Clear CUDA cache if using GPU
                if self._device == "cuda":
                    del self._model
                    torch.cuda.empty_cache()
                else:
                    del self._model

            except Exception as e:
                log(f"Warning: Error during model unload: {e}")

            self._model = None

        self._state = ASRState.UNINITIALIZED
        self._model_path = None
        log("Model unloaded")
