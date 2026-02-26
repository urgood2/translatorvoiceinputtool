"""Tests for Whisper backend language handling and initialization dispatch."""

from __future__ import annotations

from pathlib import Path

import pytest

import openvoicy_sidecar.asr as asr_module
from openvoicy_sidecar.asr.base import ASRError, ModelLoadError
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


@pytest.fixture(autouse=True)
def reset_asr_singletons() -> None:
    asr_module._engine = None
    asr_module.ASREngine._instance = None
    yield
    asr_module._engine = None
    asr_module.ASREngine._instance = None


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


def test_asr_initialize_language_en_configures_whisper_backend(
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
                "language": "en",
            }
        )
    )

    assert result["status"] == "ready"
    assert backend.language == "en"
    assert backend.initialized


def test_asr_initialize_language_auto_enables_auto_detection(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    backend = _FakeWhisperBackend()
    _patch_fast_initialize_path(monkeypatch)
    monkeypatch.setattr(asr_module, "resolve_model_family", lambda _model_id: "whisper")
    monkeypatch.setattr(asr_module, "create_backend", lambda _family, _config=None: backend)

    asr_module.handle_asr_initialize(
        _initialize_request(
            {
                "model_id": "openai/whisper-small",
                "device_pref": "cpu",
                "language": "auto",
            }
        )
    )

    assert backend.language is None


def test_asr_initialize_without_language_uses_model_default(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    backend = _FakeWhisperBackend()
    _patch_fast_initialize_path(monkeypatch)
    monkeypatch.setattr(asr_module, "resolve_model_family", lambda _model_id: "whisper")
    monkeypatch.setattr(asr_module, "resolve_default_language", lambda _model_id: "de")
    monkeypatch.setattr(asr_module, "create_backend", lambda _family, _config=None: backend)

    asr_module.handle_asr_initialize(
        _initialize_request({"model_id": "openai/whisper-small", "device_pref": "cpu"})
    )

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


def test_whisper_backend_missing_optional_dependency_has_clear_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    backend = WhisperBackend()
    monkeypatch.setattr("openvoicy_sidecar.asr.whisper._FASTER_WHISPER_AVAILABLE", False)

    with pytest.raises(ModelLoadError) as exc_info:
        backend.initialize(Path("/tmp/whisper-model"), "cpu")

    assert "faster-whisper is not installed" in str(exc_info.value)


def test_whisper_backend_is_available_reflects_dependency_probe(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr("openvoicy_sidecar.asr.whisper._FASTER_WHISPER_AVAILABLE", False)
    assert WhisperBackend.is_available() is False

    monkeypatch.setattr("openvoicy_sidecar.asr.whisper._FASTER_WHISPER_AVAILABLE", True)
    assert WhisperBackend.is_available() is True
