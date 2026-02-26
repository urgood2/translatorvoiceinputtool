"""Language extension tests for ASR initialization and recording pipeline."""

from __future__ import annotations

from pathlib import Path
import uuid
from unittest.mock import MagicMock, patch

import pytest

import openvoicy_sidecar.asr as asr_module
from openvoicy_sidecar.asr.base import ASRError
from openvoicy_sidecar.notifications import emit_transcription_complete, get_session_tracker
from openvoicy_sidecar.protocol import Request
from openvoicy_sidecar.recording import handle_recording_start


class _FakeWhisperBackend:
    def __init__(self, *, fail_language: bool = False) -> None:
        self.language = None
        self.fail_language = fail_language
        self.initialized = False

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

    def transcribe(self, audio, sample_rate: int = 16000):
        raise NotImplementedError

    def is_ready(self) -> bool:
        return self.initialized

    def get_device(self) -> str:
        return "cpu"

    def unload(self) -> None:
        self.initialized = False


@pytest.fixture(autouse=True)
def reset_asr_singletons() -> None:
    asr_module._engine = None
    asr_module.ASREngine._instance = None
    yield
    asr_module._engine = None
    asr_module.ASREngine._instance = None


def _patch_fast_initialize_path(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(asr_module, "load_manifest", lambda _model_id: object())
    monkeypatch.setattr(asr_module, "select_device", lambda _device_pref: "cpu")
    monkeypatch.setattr(asr_module.ModelCacheManager, "check_cache", lambda _self, _manifest: True)
    monkeypatch.setattr(
        asr_module.ModelCacheManager,
        "get_model_path",
        lambda _self, _manifest: Path("/tmp/whisper-model"),
    )
    monkeypatch.setattr(asr_module, "resolve_model_family", lambda _model_id: "whisper")


def _initialize_request(language):
    params = {"model_id": "openai/whisper-small", "device_pref": "cpu"}
    if language is not ...:
        params["language"] = language
    return Request(method="asr.initialize", id=1, params=params)


def test_asr_initialize_accepts_en_auto_and_null_language(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _patch_fast_initialize_path(monkeypatch)

    backend_en = _FakeWhisperBackend()
    monkeypatch.setattr(asr_module, "create_backend", lambda _family, _config=None: backend_en)
    asr_module.handle_asr_initialize(_initialize_request("en"))
    assert backend_en.language == "en"

    asr_module._engine = None
    asr_module.ASREngine._instance = None
    backend_auto = _FakeWhisperBackend()
    monkeypatch.setattr(asr_module, "create_backend", lambda _family, _config=None: backend_auto)
    asr_module.handle_asr_initialize(_initialize_request("auto"))
    assert backend_auto.language is None

    asr_module._engine = None
    asr_module.ASREngine._instance = None
    backend_null = _FakeWhisperBackend()
    monkeypatch.setattr(asr_module, "create_backend", lambda _family, _config=None: backend_null)
    asr_module.handle_asr_initialize(_initialize_request(None))
    assert backend_null.language is None


def test_asr_initialize_unsupported_language_returns_structured_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _patch_fast_initialize_path(monkeypatch)
    backend = _FakeWhisperBackend(fail_language=True)
    monkeypatch.setattr(asr_module, "create_backend", lambda _family, _config=None: backend)

    with pytest.raises(ASRError) as exc_info:
        asr_module.handle_asr_initialize(_initialize_request("english"))

    assert exc_info.value.code == "E_LANGUAGE_UNSUPPORTED"
    assert "Unsupported language" in str(exc_info.value)


def test_recording_start_forwards_language_param_to_transcription_pipeline(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    mock_recorder = MagicMock()
    mock_recorder.start.return_value = "session-language"
    monkeypatch.setattr("openvoicy_sidecar.recording.get_recorder", lambda: mock_recorder)

    with patch("openvoicy_sidecar.notifications.emit_status_changed"):
        handle_recording_start(
            Request(
                method="recording.start",
                id=1,
                params={
                    "session_id": "session-language",
                    "language": "en",
                    "vad": {"enabled": False},
                },
            )
        )

    mock_recorder.start.assert_called_once()
    _, kwargs = mock_recorder.start.call_args
    assert kwargs["preprocess"]["language"] == "en"


def test_transcription_complete_event_includes_language_when_available(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    captured = {}

    def _capture(notification):
        captured["notification"] = notification

    monkeypatch.setattr("openvoicy_sidecar.notifications.write_notification", _capture)
    session_id = str(uuid.uuid4())
    get_session_tracker().register(session_id)

    emitted = emit_transcription_complete(
        session_id=session_id,
        text="hello",
        duration_ms=50,
        confidence=0.9,
        language="en",
        raw_text="hello",
        final_text="hello",
    )

    assert emitted is True
    params = captured["notification"].params
    assert params["language"] == "en"
    assert params["confidence"] == 0.9
