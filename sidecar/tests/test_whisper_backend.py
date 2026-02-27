"""Tests for Whisper backend behavior, dispatch, and observability."""

from __future__ import annotations

import sys
import types
from pathlib import Path

import numpy as np
import pytest

import openvoicy_sidecar.asr as asr_module
import openvoicy_sidecar.asr.whisper as whisper_module
from openvoicy_sidecar.asr.base import ASRError, LegacyASRBackend, ModelLoadError
from openvoicy_sidecar.asr.dispatch import UnsupportedFamilyError
from openvoicy_sidecar.asr.parakeet import ParakeetBackend
from openvoicy_sidecar.asr.whisper import WhisperBackend
from openvoicy_sidecar.model_cache import ModelManifest
from openvoicy_sidecar.protocol import Request


class _FakeWhisperBackend:
    def __init__(self, *, fail_language: bool = False) -> None:
        self.language = None
        self.device = "cpu"
        self.initialized = False
        self.fail_language = fail_language

    def set_language(self, language):
        if self.fail_language:
            raise ValueError(
                f"Invalid language code '{language}'. Use an ISO 639-1 code (e.g. 'en') or 'auto'."
            )
        if language is None or language == "auto":
            self.language = None
        else:
            self.language = str(language).lower()

    def initialize(self, model_path: Path, device: str, progress_callback=None) -> None:
        self.initialized = True
        self.device = device

    def is_ready(self) -> bool:
        return self.initialized

    def get_device(self) -> str:
        return self.device

    def transcribe(self, audio, sample_rate: int = 16000):
        raise NotImplementedError

    def unload(self) -> None:
        self.initialized = False


class _FakeSegment:
    def __init__(self, text: str) -> None:
        self.text = text


class _FakeInfo:
    def __init__(self, language: str, language_probability: float) -> None:
        self.language = language
        self.language_probability = language_probability


class _FakeWhisperModel:
    init_calls: list[dict] = []
    transcribe_calls: list[dict] = []

    def __init__(self, model_path: str, device: str, compute_type: str) -> None:
        self.init_calls.append(
            {
                "model_path": model_path,
                "device": device,
                "compute_type": compute_type,
            }
        )

    def transcribe(self, audio, language=None, beam_size=5):
        self.transcribe_calls.append(
            {
                "length": len(audio),
                "language": language,
                "beam_size": beam_size,
            }
        )
        detected = "en" if language is None else language
        return [_FakeSegment("hello"), _FakeSegment("world")], _FakeInfo(detected, 0.98)


@pytest.fixture(autouse=True)
def reset_asr_singletons() -> None:
    asr_module._engine = None
    asr_module.ASREngine._instance = None
    yield
    asr_module._engine = None
    asr_module.ASREngine._instance = None


@pytest.fixture
def fake_faster_whisper(monkeypatch: pytest.MonkeyPatch):
    _FakeWhisperModel.init_calls.clear()
    _FakeWhisperModel.transcribe_calls.clear()
    monkeypatch.setitem(
        sys.modules,
        "faster_whisper",
        types.SimpleNamespace(WhisperModel=_FakeWhisperModel),
    )
    monkeypatch.setattr("openvoicy_sidecar.asr.whisper._FASTER_WHISPER_AVAILABLE", True)
    return _FakeWhisperModel


def _manifest(model_id: str = "openai/whisper-small") -> ModelManifest:
    return ModelManifest(
        model_id=model_id,
        revision="unit-test",
        display_name="Unit Test Model",
        total_size_bytes=1,
        files=[],
        source_url="",
    )


def _initialize_request(params: dict) -> Request:
    return Request(method="asr.initialize", id=1, params=params)


def _patch_fast_initialize_path(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(asr_module, "load_manifest", lambda _model_id: _manifest())
    monkeypatch.setattr(asr_module, "select_device", lambda _device_pref: "cpu")
    monkeypatch.setattr(asr_module.ModelCacheManager, "check_cache", lambda _self, _manifest: True)
    monkeypatch.setattr(
        asr_module.ModelCacheManager,
        "get_model_path",
        lambda _self, _manifest: Path("/tmp/whisper-model"),
    )


def test_whisper_backend_implements_legacy_asr_protocol() -> None:
    assert isinstance(WhisperBackend(), LegacyASRBackend)


def test_whisper_backend_initialize_loads_model_and_logs_timing_and_memory(
    capsys: pytest.CaptureFixture[str],
    fake_faster_whisper,
) -> None:
    backend = WhisperBackend()
    backend.initialize(Path("/tmp/whisper-model"), "cpu")

    assert backend.is_ready()
    assert backend.get_device() == "cpu"
    assert fake_faster_whisper.init_calls[0]["model_path"] == "/tmp/whisper-model"
    assert fake_faster_whisper.init_calls[0]["device"] == "cpu"

    stderr_output = capsys.readouterr().err
    assert "Whisper model loaded in" in stderr_output
    if whisper_module._resource is not None:
        assert "rss=" in stderr_output


def test_whisper_backend_transcribe_short_audio_returns_text_and_logs_metrics(
    capsys: pytest.CaptureFixture[str],
    fake_faster_whisper,
) -> None:
    backend = WhisperBackend()
    backend.initialize(Path("/tmp/whisper-model"), "cpu")

    audio = np.zeros(3200, dtype=np.float32)
    result = backend.transcribe(audio, sample_rate=16000)

    assert result.text == "hello world"
    assert result.language == "en"
    assert result.duration_ms >= 0

    stderr_output = capsys.readouterr().err
    assert "Whisper transcription finished in" in stderr_output
    assert "chars=" in stderr_output


def test_whisper_backend_language_parameter_controls_transcribe_mode(
    fake_faster_whisper,
) -> None:
    backend = WhisperBackend()
    backend.initialize(Path("/tmp/whisper-model"), "cpu")
    audio = np.zeros(1024, dtype=np.float32)

    backend.set_language("en")
    backend.transcribe(audio, sample_rate=16000)

    backend.set_language("auto")
    backend.transcribe(audio, sample_rate=16000)

    languages = [call["language"] for call in fake_faster_whisper.transcribe_calls]
    assert languages == ["en", None]


def test_handle_asr_initialize_whisper_model_id_loads_backend_and_logs_selection(
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    fake_faster_whisper,
) -> None:
    _patch_fast_initialize_path(monkeypatch)
    monkeypatch.setattr(asr_module, "resolve_model_family", lambda _model_id: "whisper")

    result = asr_module.handle_asr_initialize(
        _initialize_request(
            {
                "model_id": "openai/whisper-small",
                "device_pref": "cpu",
                "language": "en",
            }
        )
    )

    assert result["status"] == "ready"
    engine = asr_module.get_engine()
    assert isinstance(engine._backend, WhisperBackend)

    stderr_output = capsys.readouterr().err
    assert "Family=whisper -> WhisperBackend" in stderr_output


def test_handle_asr_initialize_explicit_auto_language_sets_detection_mode(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    backend = _FakeWhisperBackend()
    _patch_fast_initialize_path(monkeypatch)
    monkeypatch.setattr(asr_module, "resolve_model_family", lambda _model_id: "whisper")
    monkeypatch.setattr(asr_module, "create_backend", lambda _family, _config=None: backend)

    result = asr_module.handle_asr_initialize(
        _initialize_request(
            {
                "model_id": "openai/whisper-small",
                "device_pref": "cpu",
                "language": "auto",
            }
        )
    )

    assert result["status"] == "ready"
    assert backend.initialized
    assert backend.language is None


def test_handle_asr_initialize_without_language_uses_model_default(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    backend = _FakeWhisperBackend()
    _patch_fast_initialize_path(monkeypatch)
    monkeypatch.setattr(asr_module, "resolve_model_family", lambda _model_id: "whisper")
    monkeypatch.setattr(asr_module, "resolve_default_language", lambda _model_id: "de")
    monkeypatch.setattr(asr_module, "create_backend", lambda _family, _config=None: backend)

    result = asr_module.handle_asr_initialize(
        _initialize_request(
            {
                "model_id": "openai/whisper-small",
                "device_pref": "cpu",
            }
        )
    )

    assert result["status"] == "ready"
    assert backend.initialized
    assert backend.language == "de"


def test_asr_initialize_invalid_language_returns_clear_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    backend = _FakeWhisperBackend(fail_language=True)
    _patch_fast_initialize_path(monkeypatch)
    monkeypatch.setattr(asr_module, "resolve_model_family", lambda _model_id: "whisper")
    monkeypatch.setattr(asr_module, "create_backend", lambda _family, _config=None: backend)

    with pytest.raises(ASRError) as exc_info:
        asr_module.handle_asr_initialize(
            _initialize_request(
                {
                    "model_id": "openai/whisper-small",
                    "device_pref": "cpu",
                    "language": "english",
                }
            )
        )

    assert "Unsupported language" in str(exc_info.value)
    assert "english" in str(exc_info.value)
    assert exc_info.value.code == "E_LANGUAGE_UNSUPPORTED"
    assert not backend.initialized


def test_backend_dispatch_routes_whisper_and_parakeet_families() -> None:
    whisper_backend = asr_module.create_backend("whisper", {})
    parakeet_backend = asr_module.create_backend("parakeet", {})

    assert isinstance(whisper_backend, WhisperBackend)
    assert isinstance(parakeet_backend, ParakeetBackend)


def test_backend_dispatch_unsupported_family_returns_clear_error() -> None:
    with pytest.raises(UnsupportedFamilyError) as exc_info:
        asr_module.create_backend("unsupported-family", {})

    message = str(exc_info.value)
    assert "unsupported-family" in message
    assert "Known families" in message
    assert exc_info.value.code == "E_UNSUPPORTED_FAMILY"


def test_whisper_backend_missing_optional_dependency_has_install_guidance(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    backend = WhisperBackend()
    monkeypatch.setattr("openvoicy_sidecar.asr.whisper._FASTER_WHISPER_AVAILABLE", False)

    with pytest.raises(ModelLoadError) as exc_info:
        backend.initialize(Path("/tmp/whisper-model"), "cpu")

    message = str(exc_info.value)
    assert "faster-whisper is not installed" in message
    assert "pip install faster-whisper" in message


def test_whisper_backend_is_available_reflects_dependency_probe(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr("openvoicy_sidecar.asr.whisper._FASTER_WHISPER_AVAILABLE", False)
    assert WhisperBackend.is_available() is False

    monkeypatch.setattr("openvoicy_sidecar.asr.whisper._FASTER_WHISPER_AVAILABLE", True)
    assert WhisperBackend.is_available() is True
