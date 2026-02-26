"""JSON-RPC server loop for the sidecar."""

from __future__ import annotations

import os
import platform
import sys
from pathlib import Path
from typing import Any

from . import __version__
from .resources import (
    CONTRACTS_DIR_REL,
    MODEL_CATALOG_REL,
    MODEL_MANIFEST_REL,
    PRESETS_REL,
    list_shared_candidates,
    resolve_shared_path_optional,
)
from .asr import (
    ASRError,
    DeviceUnavailableError,
    ModelLoadError,
    ModelNotFoundError,
    NotInitializedError,
    TranscriptionError,
    get_engine,
    handle_asr_initialize,
    handle_asr_status,
    handle_asr_transcribe,
)
from .audio import (
    DeviceNotFoundError,
    MicPermissionError,
    handle_audio_list_devices,
    handle_audio_set_device,
)
from .audio_meter import (
    MeterAlreadyRunningError,
    MeterError,
    handle_audio_meter_start,
    handle_audio_meter_status,
    handle_audio_meter_stop,
)
from .model_cache import (
    CacheCorruptError,
    DiskFullError,
    LockError,
    ModelCacheError,
    ModelInUseError,
    NetworkError,
    handle_model_download,
    handle_model_get_status,
    handle_model_install,
    handle_model_purge_cache,
)
from .notifications import get_session_tracker
from .protocol import (
    ERROR_ALREADY_RECORDING,
    ERROR_AUDIO_IO,
    ERROR_CACHE_CORRUPT,
    ERROR_DEVICE_NOT_FOUND,
    ERROR_DISK_FULL,
    ERROR_INTERNAL,
    ERROR_INVALID_PARAMS,
    ERROR_INVALID_REQUEST,
    ERROR_INVALID_SESSION,
    ERROR_METHOD_NOT_FOUND,
    ERROR_MIC_PERMISSION,
    ERROR_MODEL_LOAD,
    ERROR_NETWORK,
    ERROR_NOT_READY,
    ERROR_NOT_RECORDING,
    ERROR_PARSE_ERROR,
    ERROR_TRANSCRIBE,
    MAX_LINE_LENGTH,
    InvalidRequestError,
    ParseError,
    Request,
    log,
    make_error,
    make_success,
    parse_line,
    write_response,
)
from .recording import (
    AlreadyRecordingError,
    InvalidSessionError,
    NotRecordingError,
    RecordingError,
    get_recorder,
    handle_recording_cancel,
    handle_recording_start,
    handle_recording_status,
    handle_recording_stop,
)
from .replacements import (
    ReplacementError,
    handle_replacements_get_preset_rules,
    handle_replacements_get_presets,
    handle_replacements_get_rules,
    handle_replacements_preview,
    handle_replacements_set_rules,
    load_presets_from_file,
)

# Protocol version
PROTOCOL_VERSION = "v1"


def get_startup_preset_candidates() -> list[Path]:
    """Return candidate preset paths for dev and packaged runtime layouts."""
    return list_shared_candidates(PRESETS_REL)


def load_startup_presets() -> None:
    """Load replacement presets on startup without crashing on missing/invalid files."""
    # Direct file override takes top priority (backward compat)
    env_path = os.environ.get("OPENVOICY_PRESETS_PATH")
    if env_path:
        direct = Path(env_path).expanduser()
        if direct.exists():
            log(f"Checking preset path: {direct}")
            presets = load_presets_from_file(direct)
            if presets:
                log(f"Loaded {len(presets)} preset(s) from {direct}")
            else:
                log(f"Preset file found at {direct}, but no presets were loaded")
            return

    preset_path = resolve_shared_path_optional(PRESETS_REL)
    if preset_path is None:
        candidates = get_startup_preset_candidates()
        log(
            "Preset file not found on startup; continuing with empty presets. "
            f"Checked {len(candidates)} path(s)."
        )
        return

    log(f"Checking preset path: {preset_path}")
    presets = load_presets_from_file(preset_path)
    if presets:
        log(f"Loaded {len(presets)} preset(s) from {preset_path}")
    else:
        log(f"Preset file found at {preset_path}, but no presets were loaded")


def handle_system_ping(request: Request) -> dict[str, Any]:
    """Handle system.ping request."""
    return {
        "version": __version__,
        "protocol": PROTOCOL_VERSION,
    }


def _whisper_backend_available() -> bool:
    """Return whether optional Whisper backend dependency is available."""
    try:
        from .asr.whisper import WhisperBackend

        return WhisperBackend.is_available()
    except Exception as error:
        log(f"Whisper capability probe failed: {error}")
        return False


def handle_system_info(request: Request) -> dict[str, Any]:
    """Handle system.info request."""
    cuda_available = False
    try:
        import torch

        cuda_available = torch.cuda.is_available()
    except ImportError:
        pass

    whisper_available = _whisper_backend_available()
    capabilities = ["asr", "replacements", "meter"]
    if whisper_available:
        capabilities.append("whisper")

    presets_path = resolve_shared_path_optional(PRESETS_REL)
    model_manifest_path = resolve_shared_path_optional(MODEL_MANIFEST_REL)
    model_catalog_path = resolve_shared_path_optional(MODEL_CATALOG_REL)
    contracts_dir_path = resolve_shared_path_optional(CONTRACTS_DIR_REL)
    shared_root = None
    for candidate in (
        presets_path,
        model_manifest_path,
        model_catalog_path,
        contracts_dir_path,
    ):
        if candidate is not None:
            shared_root = str(candidate.parent.parent if candidate.is_file() else candidate.parent)
            break

    return {
        "version": __version__,
        "protocol": PROTOCOL_VERSION,
        "capabilities": capabilities,
        "capabilities_detail": {
            "cuda_available": cuda_available,
            "whisper_available": whisper_available,
            "supports_progress": True,
            "supports_model_purge": True,
            "supports_silence_trim": True,
            "supports_audio_meter": True,
        },
        "runtime": {
            "python_version": platform.python_version(),
            "platform": sys.platform,
            "cuda_available": cuda_available,
        },
        "resource_paths": {
            "shared_root": shared_root,
            "presets": str(presets_path) if presets_path else None,
            "model_manifest": str(model_manifest_path) if model_manifest_path else None,
            "model_catalog": str(model_catalog_path) if model_catalog_path else None,
            "contracts_dir": str(contracts_dir_path) if contracts_dir_path else None,
        },
    }


def handle_system_shutdown(request: Request) -> dict[str, Any]:
    """Handle system.shutdown request."""
    reason = request.params.get("reason", "requested")
    log(f"Shutdown requested: {reason}")
    return {"status": "shutting_down"}


def handle_status_get(request: Request) -> dict[str, Any]:
    """Handle status.get request."""
    asr_status = get_engine().get_status()
    asr_state = asr_status.get("state")

    result: dict[str, Any] = {"state": "idle"}

    if asr_state == "error":
        result["state"] = "error"
        result["detail"] = "ASR engine error"
    elif asr_state in {"downloading", "loading"}:
        result["state"] = "loading_model"
        result["detail"] = "Downloading model..." if asr_state == "downloading" else "Loading model..."
    else:
        recording_state = get_recorder().state.value
        if recording_state in ("recording", "stopping"):
            result["state"] = "recording"
        elif get_session_tracker().has_pending():
            result["state"] = "transcribing"

    model_id = asr_status.get("model_id")
    model_status = {
        "uninitialized": "missing",
        "ready": "ready",
        "loading": "verifying",
        "downloading": "downloading",
        "error": "error",
    }.get(asr_state)
    if model_id is not None and model_status is not None:
        result["model"] = {
            "model_id": model_id,
            "status": model_status,
        }

    return result


# Method dispatch table
HANDLERS: dict[str, Any] = {
    "system.ping": handle_system_ping,
    "system.info": handle_system_info,
    "system.shutdown": handle_system_shutdown,
    "status.get": handle_status_get,
    "audio.list_devices": handle_audio_list_devices,
    "audio.set_device": handle_audio_set_device,
    "audio.meter_start": handle_audio_meter_start,
    "audio.meter_stop": handle_audio_meter_stop,
    "audio.meter_status": handle_audio_meter_status,
    "recording.start": handle_recording_start,
    "recording.stop": handle_recording_stop,
    "recording.cancel": handle_recording_cancel,
    "recording.status": handle_recording_status,
    "replacements.get_rules": handle_replacements_get_rules,
    "replacements.set_rules": handle_replacements_set_rules,
    "replacements.get_presets": handle_replacements_get_presets,
    "replacements.get_preset_rules": handle_replacements_get_preset_rules,
    "replacements.preview": handle_replacements_preview,
    "model.get_status": handle_model_get_status,
    "model.download": handle_model_download,
    "model.install": handle_model_install,
    "model.purge_cache": handle_model_purge_cache,
    "asr.initialize": handle_asr_initialize,
    "asr.status": handle_asr_status,
    "asr.transcribe": handle_asr_transcribe,
}


def dispatch(request: Request) -> dict[str, Any] | None:
    """Dispatch a request to the appropriate handler.

    Returns the result dict on success.
    Raises KeyError if method not found.
    """
    handler = HANDLERS.get(request.method)
    if handler is None:
        raise KeyError(f"Method not found: {request.method}")
    return handler(request)


def run_server() -> None:
    """Run the main JSON-RPC server loop.

    Reads NDJSON from stdin, processes requests, writes responses to stdout.
    Exits on EOF or shutdown request.
    """
    log(f"Sidecar starting (version {__version__}, protocol {PROTOCOL_VERSION})")
    load_startup_presets()

    shutdown_requested = False

    try:
        for line in sys.stdin:
            # Check line length limit
            if len(line) > MAX_LINE_LENGTH:
                log(
                    f"Line exceeds maximum length ({len(line)} > {MAX_LINE_LENGTH}); "
                    "returning invalid request and continuing"
                )
                response = make_error(
                    None,
                    ERROR_INVALID_REQUEST,
                    f"Request line exceeds maximum length ({MAX_LINE_LENGTH})",
                    "E_INVALID_PARAMS",
                    {
                        "reason": "line_too_long",
                        "max_line_length": MAX_LINE_LENGTH,
                        "line_length": len(line),
                    },
                )
                write_response(response)
                continue

            # Parse the request
            try:
                request = parse_line(line)
            except ParseError as e:
                log(f"Parse error: {e}")
                response = make_error(
                    None,
                    ERROR_PARSE_ERROR,
                    str(e),
                    "E_INTERNAL",
                    {"reason": "JSON syntax error"},
                )
                write_response(response)
                continue
            except InvalidRequestError as e:
                log(f"Invalid request: {e}")
                response = make_error(
                    None,
                    ERROR_INVALID_REQUEST,
                    str(e),
                    "E_INVALID_PARAMS",
                    {"reason": "Invalid JSON-RPC structure"},
                )
                write_response(response)
                continue

            # Skip empty lines
            if request is None:
                continue

            log(f"Received: {request.method} (id={request.id})")

            # Dispatch and handle
            try:
                result = dispatch(request)
                response = make_success(request.id, result)

                # Check for shutdown
                if request.method == "system.shutdown":
                    shutdown_requested = True

            except KeyError:
                response = make_error(
                    request.id,
                    ERROR_METHOD_NOT_FOUND,
                    f"Method not found: {request.method}",
                    "E_METHOD_NOT_FOUND",
                    {"method": request.method},
                )
            except MicPermissionError as e:
                log(f"Microphone permission denied: {e}")
                response = make_error(
                    request.id,
                    ERROR_MIC_PERMISSION,
                    str(e),
                    "E_MIC_PERMISSION",
                )
            except DeviceNotFoundError as e:
                log(f"Device not found: {e}")
                response = make_error(
                    request.id,
                    ERROR_DEVICE_NOT_FOUND,
                    str(e),
                    "E_DEVICE_NOT_FOUND",
                    {"device_uid": e.device_uid} if e.device_uid else None,
                )
            except AlreadyRecordingError as e:
                log(f"Already recording: {e}")
                response = make_error(
                    request.id,
                    ERROR_ALREADY_RECORDING,
                    str(e),
                    "E_ALREADY_RECORDING",
                )
            except NotRecordingError as e:
                log(f"Not recording: {e}")
                response = make_error(
                    request.id,
                    ERROR_NOT_RECORDING,
                    str(e),
                    "E_NOT_RECORDING",
                )
            except InvalidSessionError as e:
                log(f"Invalid session: {e}")
                response = make_error(
                    request.id,
                    ERROR_INVALID_SESSION,
                    str(e),
                    "E_INVALID_SESSION",
                )
            except RecordingError as e:
                log(f"Recording error: {e}")
                response = make_error(
                    request.id,
                    ERROR_AUDIO_IO,
                    str(e),
                    e.code,
                )
            except MeterAlreadyRunningError as e:
                log(f"Meter already running: {e}")
                response = make_error(
                    request.id,
                    ERROR_AUDIO_IO,
                    str(e),
                    "E_METER_RUNNING",
                )
            except MeterError as e:
                log(f"Meter error: {e}")
                response = make_error(
                    request.id,
                    ERROR_AUDIO_IO,
                    str(e),
                    e.code,
                )
            except ReplacementError as e:
                log(f"Replacement error: {e}")
                response = make_error(
                    request.id,
                    ERROR_INVALID_PARAMS,
                    str(e),
                    e.code,
                )
            except DiskFullError as e:
                log(f"Disk full error: {e}")
                response = make_error(
                    request.id,
                    ERROR_DISK_FULL,
                    str(e),
                    "E_DISK_FULL",
                    {"required_bytes": e.required, "available_bytes": e.available},
                )
            except NetworkError as e:
                log(f"Network error: {e}")
                response = make_error(
                    request.id,
                    ERROR_NETWORK,
                    str(e),
                    "E_NETWORK",
                    {"url": e.url} if e.url else None,
                )
            except CacheCorruptError as e:
                log(f"Cache corrupt error: {e}")
                details = dict(getattr(e, "details", {}) or {})
                if e.file_path and "file_path" not in details:
                    details["file_path"] = e.file_path
                details.setdefault("recoverable", getattr(e, "recoverable", True))
                response = make_error(
                    request.id,
                    ERROR_CACHE_CORRUPT,
                    str(e),
                    "E_CACHE_CORRUPT",
                    details or None,
                )
            except ModelInUseError as e:
                log(f"Model in use error: {e}")
                response = make_error(
                    request.id,
                    ERROR_NOT_READY,
                    str(e),
                    "E_NOT_READY",
                )
            except (LockError, ModelCacheError) as e:
                log(f"Model cache error: {e}")
                response = make_error(
                    request.id,
                    ERROR_MODEL_LOAD,
                    str(e),
                    e.code if hasattr(e, "code") else "E_MODEL",
                )
            except ModelNotFoundError as e:
                log(f"Model not found: {e}")
                response = make_error(
                    request.id,
                    ERROR_MODEL_LOAD,
                    str(e),
                    "E_MODEL_NOT_FOUND",
                )
            except ModelLoadError as e:
                log(f"Model load error: {e}")
                response = make_error(
                    request.id,
                    ERROR_MODEL_LOAD,
                    str(e),
                    e.code,
                )
            except DeviceUnavailableError as e:
                log(f"Device unavailable: {e}")
                response = make_error(
                    request.id,
                    ERROR_NOT_READY,
                    str(e),
                    "E_DEVICE_UNAVAILABLE",
                    {"requested_device": e.requested_device},
                )
            except NotInitializedError as e:
                log(f"ASR not initialized: {e}")
                response = make_error(
                    request.id,
                    ERROR_NOT_READY,
                    str(e),
                    "E_NOT_INITIALIZED",
                )
            except TranscriptionError as e:
                log(f"Transcription error: kind={getattr(e, 'code', 'E_TRANSCRIPTION')}")
                response = make_error(
                    request.id,
                    ERROR_TRANSCRIBE,
                    str(e),
                    "E_TRANSCRIPTION",
                )
            except ASRError as e:
                log(f"ASR error: {e}")
                response = make_error(
                    request.id,
                    ERROR_MODEL_LOAD,
                    str(e),
                    e.code,
                )
            except Exception as e:
                log(f"Internal error handling {request.method}: {e}")
                response = make_error(
                    request.id,
                    ERROR_INTERNAL,
                    f"Internal error: {e}",
                    "E_INTERNAL",
                )

            if request.id is not None:
                write_response(response)
            else:
                log(f"Notification handled without response: {request.method}")

            # Exit after handling shutdown request.
            if shutdown_requested:
                log("Shutdown complete")
                break

    except KeyboardInterrupt:
        log("Interrupted")
    except EOFError:
        log("EOF received, shutting down")

    log("Server exiting")
