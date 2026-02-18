"""JSON-RPC server loop for the sidecar."""

from __future__ import annotations

import os
import platform
import sys
from pathlib import Path
from typing import Any

from . import __version__
from .audio import (
    DeviceNotFoundError,
    MicPermissionError,
    handle_audio_list_devices,
    handle_audio_set_device,
)
from .audio_meter import (
    MeterAlreadyRunningError,
    MeterError,
    MeterNotRunningError,
    handle_audio_meter_start,
    handle_audio_meter_status,
    handle_audio_meter_stop,
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
    load_presets_from_file,
    handle_replacements_get_presets,
    handle_replacements_get_preset_rules,
    handle_replacements_get_rules,
    handle_replacements_preview,
    handle_replacements_set_rules,
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
    handle_model_purge_cache,
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

# Protocol version
PROTOCOL_VERSION = "v1"


def get_startup_preset_candidates() -> list[Path]:
    """Return candidate preset paths for dev and packaged runtime layouts."""
    candidates: list[Path] = []

    env_path = os.environ.get("OPENVOICY_PRESETS_PATH")
    if env_path:
        candidates.append(Path(env_path).expanduser())

    # Dev mode: repository layout
    candidates.append(Path(__file__).resolve().parents[3] / "shared" / "replacements" / "PRESETS.json")

    # Packaged layouts: sidecar next to resources or in app bundle resources
    exe_dir = Path(sys.executable).resolve().parent
    candidates.append(exe_dir / "shared" / "replacements" / "PRESETS.json")
    candidates.append(exe_dir.parent / "Resources" / "shared" / "replacements" / "PRESETS.json")

    # PyInstaller onefile extraction directory
    meipass_dir = getattr(sys, "_MEIPASS", None)
    if meipass_dir:
        candidates.append(Path(meipass_dir) / "shared" / "replacements" / "PRESETS.json")

    # Working-directory fallback
    candidates.append(Path.cwd() / "shared" / "replacements" / "PRESETS.json")

    # De-duplicate while preserving order
    unique: list[Path] = []
    seen: set[str] = set()
    for candidate in candidates:
        key = str(candidate)
        if key in seen:
            continue
        seen.add(key)
        unique.append(candidate)

    return unique


def load_startup_presets() -> None:
    """Load replacement presets on startup without crashing on missing/invalid files."""
    candidates = get_startup_preset_candidates()
    for preset_path in candidates:
        log(f"Checking preset path: {preset_path}")
        if not preset_path.exists():
            log(f"Preset path missing: {preset_path}")
            continue

        presets = load_presets_from_file(preset_path)
        if presets:
            log(f"Loaded {len(presets)} preset(s) from {preset_path}")
        else:
            log(f"Preset file found at {preset_path}, but no presets were loaded")
        return

    log(
        "Preset file not found on startup; continuing with empty presets. "
        f"Checked {len(candidates)} path(s)."
    )


def handle_system_ping(request: Request) -> dict[str, Any]:
    """Handle system.ping request."""
    return {
        "version": __version__,
        "protocol": PROTOCOL_VERSION,
    }


def handle_system_info(request: Request) -> dict[str, Any]:
    """Handle system.info request."""
    cuda_available = False
    try:
        import torch  # noqa: F401

        cuda_available = torch.cuda.is_available()
    except ImportError:
        pass

    return {
        "version": __version__,
        "protocol": PROTOCOL_VERSION,
        "capabilities": ["asr", "replacements", "meter"],
        "capabilities_detail": {
            "cuda_available": cuda_available,
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
    else:
        recording_state = get_recorder().state.value
        if recording_state in ("recording", "stopping"):
            result["state"] = "recording"
        elif get_session_tracker().has_pending():
            result["state"] = "transcribing"

    model_id = asr_status.get("model_id")
    model_status = {
        "ready": "ready",
        "loading": "loading",
        "downloading": "loading",
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
                log(f"Line exceeds maximum length ({len(line)} > {MAX_LINE_LENGTH}), fatal")
                sys.exit(1)

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
                response = make_error(
                    request.id,
                    ERROR_CACHE_CORRUPT,
                    str(e),
                    "E_CACHE_CORRUPT",
                    {"file_path": e.file_path} if e.file_path else None,
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
                log(f"Transcription error: {e}")
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

            write_response(response)

            # Exit after sending shutdown response
            if shutdown_requested:
                log("Shutdown complete")
                break

    except KeyboardInterrupt:
        log("Interrupted")
    except EOFError:
        log("EOF received, shutting down")

    log("Server exiting")
