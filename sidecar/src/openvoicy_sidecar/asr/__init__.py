"""ASR module - speech recognition with pluggable backends.

This module provides idempotent ASR initialization and inference
with support for CPU and CUDA devices.
"""

from __future__ import annotations

import json
import threading
import time
from pathlib import Path
from typing import Any, Optional

from ..model_cache import ModelCacheManager, ModelManifest, ModelStatus
from ..protocol import Request, log, write_event
from .base import (
    ASRBackend,
    ASRError,
    ASRState,
    DeviceUnavailableError,
    InitProgress,
    ModelLoadError,
    ModelNotFoundError,
    NotInitializedError,
    ProgressCallback,
    TranscriptionError,
    TranscriptionResult,
)
from .parakeet import ParakeetBackend, check_cuda_available, select_device

# Re-export public API
__all__ = [
    "ASRBackend",
    "ASREngine",
    "ASRError",
    "ASRState",
    "DeviceUnavailableError",
    "InitProgress",
    "ModelLoadError",
    "ModelNotFoundError",
    "NotInitializedError",
    "TranscriptionError",
    "TranscriptionResult",
    "handle_asr_initialize",
    "handle_asr_transcribe",
    "handle_asr_status",
]

# Default manifest path
DEFAULT_MANIFEST_PATH = Path(__file__).parent.parent.parent.parent.parent / "shared" / "model" / "MODEL_MANIFEST.json"


def load_manifest(model_id: str) -> ModelManifest:
    """Load model manifest from shared directory.

    Args:
        model_id: Model identifier (e.g., "parakeet-tdt-0.6b-v3")

    Returns:
        ModelManifest for the requested model.

    Raises:
        ModelNotFoundError: If manifest not found.
    """
    manifest_path = DEFAULT_MANIFEST_PATH

    if not manifest_path.exists():
        raise ModelNotFoundError(f"Model manifest not found: {manifest_path}")

    try:
        with open(manifest_path) as f:
            data = json.load(f)

        # Verify model ID matches
        if data.get("model_id") != model_id:
            raise ModelNotFoundError(
                f"Manifest model_id '{data.get('model_id')}' does not match requested '{model_id}'"
            )

        return ModelManifest.from_dict(data)

    except json.JSONDecodeError as e:
        raise ModelNotFoundError(f"Invalid manifest JSON: {e}") from e


class ASREngine:
    """ASR engine with idempotent initialization.

    This class manages the ASR backend lifecycle, providing:
    - Idempotent initialization (fast path if already ready)
    - Automatic model download if not cached
    - Progress events during initialization
    - Thread-safe operation
    """

    _instance: Optional[ASREngine] = None
    _lock = threading.Lock()

    def __new__(cls) -> ASREngine:
        """Singleton pattern for global ASR engine."""
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._initialized = False
        return cls._instance

    def __init__(self):
        if self._initialized:
            return

        self._backend: Optional[ASRBackend] = None
        self._model_id: Optional[str] = None
        self._state: ASRState = ASRState.UNINITIALIZED
        self._cache_manager = ModelCacheManager()
        self._init_lock = threading.Lock()
        self._initialized = True

    @property
    def state(self) -> ASRState:
        """Get current engine state."""
        return self._state

    def is_ready(self) -> bool:
        """Check if engine is ready for transcription."""
        return self._state == ASRState.READY and self._backend is not None

    def initialize(
        self,
        model_id: str,
        device_pref: str = "auto",
        progress_callback: Optional[ProgressCallback] = None,
    ) -> dict[str, Any]:
        """Initialize ASR with the specified model.

        This method is idempotent - if already initialized with the same
        model and device, returns immediately.

        Args:
            model_id: Model identifier (e.g., "parakeet-tdt-0.6b-v3")
            device_pref: Device preference ("auto", "cuda", "cpu")
            progress_callback: Optional callback for progress updates

        Returns:
            Status dictionary with model_id, device, and status.
        """
        start_time = time.time()

        # Fast path: already initialized with same model
        if (
            self._state == ASRState.READY
            and self._model_id == model_id
            and self._backend is not None
        ):
            elapsed = (time.time() - start_time) * 1000
            log(f"ASR already initialized (fast path: {elapsed:.1f}ms)")
            return {
                "status": "ready",
                "model_id": model_id,
                "device": self._backend.get_device(),
            }

        with self._init_lock:
            # Double-check after acquiring lock
            if (
                self._state == ASRState.READY
                and self._model_id == model_id
                and self._backend is not None
            ):
                elapsed = (time.time() - start_time) * 1000
                log(f"ASR already initialized (fast path after lock: {elapsed:.1f}ms)")
                return {
                    "status": "ready",
                    "model_id": model_id,
                    "device": self._backend.get_device(),
                }

            # Select device
            device = select_device(device_pref)
            log(f"Initializing ASR: model={model_id}, device_pref={device_pref}, selected={device}")

            # Load manifest
            manifest = load_manifest(model_id)

            # Check cache / download model
            if not self._cache_manager.check_cache(manifest):
                self._state = ASRState.DOWNLOADING

                def download_progress(prog):
                    if progress_callback:
                        progress_callback(
                            InitProgress(
                                state="loading_model",
                                detail=f"Downloading {prog.current_file}...",
                                progress=prog.to_dict(),
                            )
                        )

                model_path = self._cache_manager.download_model(
                    manifest, progress_callback=download_progress
                )
            else:
                model_path = self._cache_manager.get_model_path(manifest)

            # Create and initialize backend
            self._state = ASRState.LOADING

            if progress_callback:
                progress_callback(
                    InitProgress(state="loading_model", detail="Loading model into memory...")
                )

            backend = ParakeetBackend()
            backend.initialize(model_path, device, progress_callback)

            # Unload previous backend if different model
            if self._backend is not None and self._model_id != model_id:
                log("Unloading previous model")
                self._backend.unload()

            self._backend = backend
            self._model_id = model_id
            self._state = ASRState.READY

            elapsed = (time.time() - start_time) * 1000
            log(f"ASR initialized in {elapsed:.1f}ms")

            return {
                "status": "ready",
                "model_id": model_id,
                "device": device,
            }

    def transcribe(self, audio, sample_rate: int = 16000) -> TranscriptionResult:
        """Transcribe audio to text.

        Args:
            audio: Audio data as numpy array.
            sample_rate: Sample rate (default: 16000).

        Returns:
            TranscriptionResult with text.
        """
        if not self.is_ready() or self._backend is None:
            raise NotInitializedError("ASR not initialized. Call initialize() first.")

        return self._backend.transcribe(audio, sample_rate)

    def get_status(self) -> dict[str, Any]:
        """Get current ASR status."""
        status: dict[str, Any] = {
            "state": self._state.value,
            "model_id": self._model_id,
        }

        if self._backend is not None:
            status["device"] = self._backend.get_device()
            status["ready"] = self._backend.is_ready()
        else:
            status["ready"] = False

        return status

    def unload(self) -> None:
        """Unload the model and free resources."""
        with self._init_lock:
            if self._backend is not None:
                self._backend.unload()
                self._backend = None

            self._model_id = None
            self._state = ASRState.UNINITIALIZED


# Global engine instance
_engine: Optional[ASREngine] = None


def get_engine() -> ASREngine:
    """Get the global ASR engine instance."""
    global _engine
    if _engine is None:
        _engine = ASREngine()
    return _engine


# === JSON-RPC Handlers ===


def handle_asr_initialize(request: Request) -> dict[str, Any]:
    """Handle asr.initialize request.

    Params:
        model_id: Model identifier (default: "parakeet-tdt-0.6b-v3")
        device_pref: Device preference - "auto", "cuda", or "cpu" (default: "auto")

    Returns:
        { status: "ready", model_id: string, device: string }
    """
    params = request.params
    model_id = params.get("model_id", "parakeet-tdt-0.6b-v3")
    device_pref = params.get("device_pref", "auto")

    # Validate device_pref
    if device_pref not in ("auto", "cuda", "cpu"):
        raise ASRError(f"Invalid device_pref: {device_pref}. Must be 'auto', 'cuda', or 'cpu'.")

    engine = get_engine()

    # Progress callback that emits events
    def emit_progress(progress: InitProgress):
        write_event("status_changed", progress.to_dict())

    return engine.initialize(model_id, device_pref, progress_callback=emit_progress)


def handle_asr_transcribe(request: Request) -> dict[str, Any]:
    """Handle asr.transcribe request.

    This is primarily for testing - normal flow uses recording.stop which
    automatically transcribes.

    Params:
        audio_path: Path to audio file (WAV/FLAC)

    Returns:
        TranscriptionResult dict
    """
    import numpy as np

    params = request.params
    audio_path = params.get("audio_path")

    if not audio_path:
        raise ASRError("audio_path parameter required")

    path = Path(audio_path)
    if not path.exists():
        raise ASRError(f"Audio file not found: {audio_path}")

    # Load audio file
    try:
        from scipy.io import wavfile

        sample_rate, audio = wavfile.read(str(path))

        # Convert to float32 if needed
        if audio.dtype == np.int16:
            audio = audio.astype(np.float32) / 32768.0
        elif audio.dtype == np.int32:
            audio = audio.astype(np.float32) / 2147483648.0
        elif audio.dtype != np.float32:
            audio = audio.astype(np.float32)

        # Ensure mono
        if audio.ndim > 1:
            audio = audio.mean(axis=1)

    except Exception as e:
        raise ASRError(f"Failed to load audio: {e}") from e

    engine = get_engine()
    result = engine.transcribe(audio, sample_rate)
    return result.to_dict()


def handle_asr_status(request: Request) -> dict[str, Any]:
    """Handle asr.status request.

    Returns current ASR engine status.
    """
    engine = get_engine()
    return engine.get_status()
