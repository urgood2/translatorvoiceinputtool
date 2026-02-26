"""Tests for ASR backend dispatch by model family."""

from __future__ import annotations

import pytest

from openvoicy_sidecar.asr.base import ASRBackend
from openvoicy_sidecar.asr.dispatch import (
    UnsupportedFamilyError,
    _REGISTRY,
    get_backend,
    register_backend,
    registered_families,
)
from openvoicy_sidecar.asr.parakeet import ParakeetBackend
from openvoicy_sidecar.asr.whisper import WhisperBackend


@pytest.fixture(autouse=True)
def restore_registry() -> None:
    snapshot = _REGISTRY.copy()
    yield
    _REGISTRY.clear()
    _REGISTRY.update(snapshot)


def test_backend_dispatch_whisper_family_selects_whisper_backend() -> None:
    backend = get_backend("whisper")
    assert isinstance(backend, WhisperBackend)


def test_backend_dispatch_parakeet_family_selects_parakeet_backend() -> None:
    backend = get_backend("parakeet")
    assert isinstance(backend, ParakeetBackend)


def test_model_family_validation_unknown_family_has_clear_error() -> None:
    with pytest.raises(UnsupportedFamilyError) as exc_info:
        get_backend("unknown-family")

    message = str(exc_info.value)
    assert "unknown-family" in message
    assert "Known families" in message
    assert exc_info.value.code == "E_UNSUPPORTED_FAMILY"


def test_unknown_family_error_lists_registered_families() -> None:
    with pytest.raises(UnsupportedFamilyError) as exc_info:
        get_backend("does-not-exist")

    message = str(exc_info.value)
    assert "parakeet" in message
    assert "whisper" in message


def test_parakeet_and_whisper_implement_the_same_backend_protocol() -> None:
    assert isinstance(get_backend("parakeet"), ASRBackend)
    assert isinstance(get_backend("whisper"), ASRBackend)


def test_register_backend_allows_new_family() -> None:
    class StubBackend:
        def initialize(self, model_path, device, progress_callback=None) -> None:
            return None

        def transcribe(self, audio, sample_rate: int = 16000):
            return None

        def is_ready(self) -> bool:
            return True

        def get_device(self) -> str:
            return "cpu"

        def unload(self) -> None:
            return None

    register_backend("stub", StubBackend)
    backend = get_backend("stub")
    assert isinstance(backend, StubBackend)
    assert "stub" in registered_families()
