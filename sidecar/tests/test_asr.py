"""Tests for ASR initialization and inference."""

from __future__ import annotations

import json
import threading
import time
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import numpy as np
import pytest

from openvoicy_sidecar.asr import (
    ASREngine,
    ASRError,
    ASRState,
    DeviceUnavailableError,
    InitProgress,
    ModelLoadError,
    ModelNotFoundError,
    NotInitializedError,
    TranscriptionError,
    TranscriptionResult,
    handle_asr_initialize,
    handle_asr_status,
    load_manifest,
)
from openvoicy_sidecar.asr.parakeet import ParakeetBackend, check_cuda_available, select_device
from openvoicy_sidecar.protocol import Request


# === Fixtures ===


@pytest.fixture
def mock_manifest_path(tmp_path):
    """Create a mock manifest file."""
    manifest = {
        "model_id": "test-model",
        "display_name": "Test Model",
        "revision": "v1",
        "total_size_bytes": 1000,
        "files": [
            {
                "path": "model.nemo",
                "size_bytes": 1000,
                "sha256": "abc123",
            }
        ],
        "mirrors": [
            {"url": "http://example.com/model.nemo"}
        ],
    }
    manifest_path = tmp_path / "MODEL_MANIFEST.json"
    manifest_path.write_text(json.dumps(manifest))
    return manifest_path


@pytest.fixture
def sample_audio():
    """Create sample audio data."""
    # Generate 1 second of 16kHz audio
    duration = 1.0
    sample_rate = 16000
    t = np.linspace(0, duration, int(sample_rate * duration), dtype=np.float32)
    # Simple sine wave at 440Hz
    audio = np.sin(2 * np.pi * 440 * t).astype(np.float32)
    return audio


@pytest.fixture(autouse=True)
def reset_engine():
    """Reset the ASR engine singleton before each test."""
    # Reset the singleton
    ASREngine._instance = None
    yield
    # Clean up after test
    ASREngine._instance = None


# === Unit Tests: Base Types ===


class TestTranscriptionResult:
    """Tests for TranscriptionResult."""

    def test_to_dict_minimal(self):
        """Should convert minimal result to dict."""
        result = TranscriptionResult(text="hello world")
        d = result.to_dict()

        assert d["text"] == "hello world"
        assert "language" not in d
        assert "confidence" not in d

    def test_to_dict_full(self):
        """Should include all fields when present."""
        result = TranscriptionResult(
            text="hello",
            language="en",
            confidence=0.95,
            duration_ms=150,
        )
        d = result.to_dict()

        assert d["text"] == "hello"
        assert d["language"] == "en"
        assert d["confidence"] == 0.95
        assert d["duration_ms"] == 150


class TestInitProgress:
    """Tests for InitProgress."""

    def test_to_dict(self):
        """Should convert to event format."""
        progress = InitProgress(
            state="loading_model",
            detail="Loading weights...",
            progress={"current": 50, "total": 100},
        )
        d = progress.to_dict()

        assert d["state"] == "loading_model"
        assert d["detail"] == "Loading weights..."
        assert d["progress"]["current"] == 50


# === Unit Tests: Device Selection ===


class TestDeviceSelection:
    """Tests for device selection logic."""

    def test_select_device_cpu(self):
        """Should return cpu when requested."""
        device = select_device("cpu")
        assert device == "cpu"

    def test_select_device_cuda_unavailable(self):
        """Should raise when cuda requested but unavailable."""
        with patch("openvoicy_sidecar.asr.parakeet.check_cuda_available", return_value=False):
            with pytest.raises(DeviceUnavailableError) as exc_info:
                select_device("cuda")

            assert exc_info.value.requested_device == "cuda"

    def test_select_device_auto_fallback(self):
        """Should fallback to cpu when auto and cuda unavailable."""
        with patch("openvoicy_sidecar.asr.parakeet.check_cuda_available", return_value=False):
            device = select_device("auto")
            assert device == "cpu"

    def test_select_device_auto_cuda(self):
        """Should use cuda when auto and cuda available."""
        with patch("openvoicy_sidecar.asr.parakeet.check_cuda_available", return_value=True):
            device = select_device("auto")
            assert device == "cuda"


# === Unit Tests: Manifest Loading ===


class TestManifestLoading:
    """Tests for manifest loading."""

    def test_load_manifest_not_found(self):
        """Should raise when manifest file not found."""
        with patch("openvoicy_sidecar.asr.resolve_shared_path", side_effect=FileNotFoundError("not found")):
            with pytest.raises(ModelNotFoundError):
                load_manifest("test-model")

    def test_load_manifest_wrong_model_id(self, mock_manifest_path):
        """Should raise when model ID doesn't match."""
        with patch("openvoicy_sidecar.asr.resolve_shared_path", return_value=mock_manifest_path):
            with pytest.raises(ModelNotFoundError) as exc_info:
                load_manifest("wrong-model")

            assert "wrong-model" in str(exc_info.value)


# === Unit Tests: ParakeetBackend ===


class TestParakeetBackend:
    """Tests for ParakeetBackend."""

    def test_initial_state(self):
        """Should start uninitialized."""
        backend = ParakeetBackend()
        assert not backend.is_ready()
        assert backend.get_state() == ASRState.UNINITIALIZED

    def test_transcribe_not_initialized(self, sample_audio):
        """Should raise when transcribing without init."""
        backend = ParakeetBackend()

        with pytest.raises(NotInitializedError):
            backend.transcribe(sample_audio)

    def test_initialize_missing_torch(self, tmp_path):
        """Should raise helpful error when torch not installed."""
        backend = ParakeetBackend()

        with patch.dict("sys.modules", {"torch": None}):
            with patch("builtins.__import__", side_effect=ImportError("No module torch")):
                with pytest.raises(ModelLoadError) as exc_info:
                    backend.initialize(tmp_path, "cpu")

                assert "PyTorch" in str(exc_info.value)

    def test_find_nemo_file(self, tmp_path):
        """Should find .nemo file in directory."""
        backend = ParakeetBackend()

        # Create a .nemo file
        nemo_file = tmp_path / "model.nemo"
        nemo_file.write_bytes(b"test")

        found = backend._find_nemo_file(tmp_path)
        assert found == nemo_file

    def test_find_nemo_file_direct(self, tmp_path):
        """Should accept direct path to .nemo file."""
        backend = ParakeetBackend()

        nemo_file = tmp_path / "model.nemo"
        nemo_file.write_bytes(b"test")

        found = backend._find_nemo_file(nemo_file)
        assert found == nemo_file

    def test_unload(self):
        """Should reset state on unload."""
        backend = ParakeetBackend()
        backend._state = ASRState.READY
        backend._model = MagicMock()

        backend.unload()

        assert backend._state == ASRState.UNINITIALIZED
        assert backend._model is None


# === Unit Tests: ASREngine ===


class TestASREngine:
    """Tests for ASREngine singleton."""

    def test_singleton(self):
        """Should return same instance."""
        engine1 = ASREngine()
        engine2 = ASREngine()
        assert engine1 is engine2

    def test_initial_state(self):
        """Should start uninitialized."""
        engine = ASREngine()
        assert engine.state == ASRState.UNINITIALIZED
        assert not engine.is_ready()

    def test_get_status_uninitialized(self):
        """Should return status when uninitialized."""
        engine = ASREngine()
        status = engine.get_status()

        assert status["state"] == "uninitialized"
        assert status["ready"] is False

    def test_idempotent_fast_path(self):
        """Should return quickly if already initialized."""
        engine = ASREngine()

        # Simulate initialized state
        engine._state = ASRState.READY
        engine._model_id = "test-model"
        engine._backend = MagicMock()
        engine._backend.get_device.return_value = "cpu"
        engine._backend.is_ready.return_value = True

        start = time.time()
        result = engine.initialize("test-model", "auto")
        elapsed = (time.time() - start) * 1000

        assert elapsed < 250  # Should be very fast
        assert result["status"] == "ready"
        assert result["device"] == "cpu"


# === Unit Tests: JSON-RPC Handlers ===


class TestHandlers:
    """Tests for JSON-RPC handlers."""

    def test_asr_status_handler(self):
        """Should return current status."""
        request = Request(method="asr.status", id=1)
        result = handle_asr_status(request)

        assert "state" in result
        assert "ready" in result

    def test_asr_initialize_invalid_device(self):
        """Should reject invalid device_pref."""
        request = Request(
            method="asr.initialize",
            id=1,
            params={"model_id": "test", "device_pref": "invalid"},
        )

        with pytest.raises(ASRError):
            handle_asr_initialize(request)

    def test_asr_initialize_passes_normalized_language_to_engine(self):
        """Should normalize language and pass it through to engine.initialize."""
        request = Request(
            method="asr.initialize",
            id=1,
            params={
                "model_id": "test-model",
                "device_pref": "cpu",
                "language": "EN",
            },
        )

        mock_engine = MagicMock()
        mock_engine.initialize.return_value = {
            "status": "ready",
            "model_id": "test-model",
            "device": "cpu",
        }

        with patch("openvoicy_sidecar.asr.get_engine", return_value=mock_engine):
            result = handle_asr_initialize(request)

        assert result["status"] == "ready"
        assert mock_engine.initialize.call_count == 1
        call_kwargs = mock_engine.initialize.call_args.kwargs
        assert call_kwargs["language"] == "en"
        assert callable(call_kwargs["progress_callback"])

    def test_asr_initialize_accepts_auto_and_null_language(self):
        """Should accept 'auto' and null language values."""
        mock_engine = MagicMock()
        mock_engine.initialize.return_value = {
            "status": "ready",
            "model_id": "test-model",
            "device": "cpu",
        }

        with patch("openvoicy_sidecar.asr.get_engine", return_value=mock_engine):
            handle_asr_initialize(
                Request(
                    method="asr.initialize",
                    id=1,
                    params={"model_id": "test-model", "device_pref": "cpu", "language": "auto"},
                )
            )
            handle_asr_initialize(
                Request(
                    method="asr.initialize",
                    id=2,
                    params={"model_id": "test-model", "device_pref": "cpu", "language": None},
                )
            )

        first_kwargs = mock_engine.initialize.call_args_list[0].kwargs
        second_kwargs = mock_engine.initialize.call_args_list[1].kwargs
        assert first_kwargs["language"] == "auto"
        assert second_kwargs["language"] is None

    def test_asr_initialize_rejects_invalid_language_type(self):
        """Should reject non-string/non-null language values."""
        request = Request(
            method="asr.initialize",
            id=1,
            params={
                "model_id": "test-model",
                "device_pref": "cpu",
                "language": ["en"],
            },
        )

        with pytest.raises(ASRError) as exc_info:
            handle_asr_initialize(request)

        assert exc_info.value.code == "E_INVALID_PARAMS"


# === Integration Tests ===


class TestASRIntegration:
    """Integration tests for ASR pipeline."""

    def test_engine_unload(self):
        """Should reset state on unload."""
        engine = ASREngine()

        # Simulate initialized state
        engine._state = ASRState.READY
        engine._model_id = "test-model"
        engine._backend = MagicMock()

        engine.unload()

        assert engine.state == ASRState.UNINITIALIZED
        assert engine._backend is None

    def test_full_flow_mock(self, tmp_path):
        """Test full initialization flow with mocks."""
        engine = ASREngine()

        # Create mock manifest
        manifest_data = {
            "model_id": "test-model",
            "display_name": "Test",
            "revision": "v1",
            "total_size_bytes": 100,
            "files": [{"path": "model.nemo", "size_bytes": 100, "sha256": "abc"}],
            "mirrors": [{"url": "http://example.com/model.nemo"}],
        }
        manifest_path = tmp_path / "MODEL_MANIFEST.json"
        manifest_path.write_text(json.dumps(manifest_data))

        # Create mock model file
        model_dir = tmp_path / "cache" / "test-model"
        model_dir.mkdir(parents=True)
        (model_dir / "model.nemo").write_bytes(b"test")
        (model_dir / "manifest.json").write_text(json.dumps(manifest_data))

        # Mock the backend
        mock_backend = MagicMock()
        mock_backend.is_ready.return_value = True
        mock_backend.get_device.return_value = "cpu"

        with patch("openvoicy_sidecar.asr.resolve_shared_path", return_value=manifest_path):
            with patch("openvoicy_sidecar.asr.ParakeetBackend") as MockBackend:
                MockBackend.return_value = mock_backend
                with patch.object(engine._cache_manager, "check_cache", return_value=True):
                    with patch.object(engine._cache_manager, "get_model_path", return_value=model_dir):
                        result = engine.initialize("test-model", "cpu")

        assert result["status"] == "ready"
        assert result["model_id"] == "test-model"
        assert result["device"] == "cpu"

    def test_concurrent_init_lock_exists(self):
        """Should have a threading lock for concurrent protection."""
        engine = ASREngine()

        # Verify the lock exists and is a proper threading lock
        assert hasattr(engine, "_init_lock")
        assert isinstance(engine._init_lock, type(threading.Lock()))

        # Test that lock can be acquired and released
        assert engine._init_lock.acquire(blocking=False)
        engine._init_lock.release()

    def test_idempotent_returns_quickly(self):
        """Fast path should return within 250ms."""
        engine = ASREngine()

        # Set up as if already initialized
        engine._state = ASRState.READY
        engine._model_id = "test-model"
        mock_backend = MagicMock()
        mock_backend.get_device.return_value = "cpu"
        mock_backend.is_ready.return_value = True
        engine._backend = mock_backend

        # Time the fast path
        start = time.time()
        result = engine.initialize("test-model", "auto")
        elapsed_ms = (time.time() - start) * 1000

        assert result["status"] == "ready"
        assert elapsed_ms < 250, f"Fast path took {elapsed_ms}ms, should be < 250ms"


# === Error Tests ===


class TestErrors:
    """Tests for error handling."""

    def test_model_not_found_error(self):
        """ModelNotFoundError should have correct code."""
        error = ModelNotFoundError("Model not found")
        assert error.code == "E_MODEL_NOT_FOUND"

    def test_model_load_error(self):
        """ModelLoadError should have correct code."""
        error = ModelLoadError("Failed to load")
        assert error.code == "E_MODEL_LOAD"

    def test_device_unavailable_error(self):
        """DeviceUnavailableError should store device info."""
        error = DeviceUnavailableError("CUDA not available", "cuda")
        assert error.requested_device == "cuda"
        assert error.code == "E_DEVICE_UNAVAILABLE"

    def test_transcription_error(self):
        """TranscriptionError should have correct code."""
        error = TranscriptionError("Failed")
        assert error.code == "E_TRANSCRIPTION"

    def test_not_initialized_error(self):
        """NotInitializedError should have correct code."""
        error = NotInitializedError("Not ready")
        assert error.code == "E_NOT_INITIALIZED"
