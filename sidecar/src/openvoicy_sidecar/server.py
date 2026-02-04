"""JSON-RPC server loop for the sidecar."""

from __future__ import annotations

import platform
import sys
from typing import Any

from . import __version__
from .audio import (
    DeviceNotFoundError,
    MicPermissionError,
    handle_audio_list_devices,
    handle_audio_set_device,
)
from .protocol import (
    ERROR_DEVICE_NOT_FOUND,
    ERROR_INTERNAL,
    ERROR_INVALID_REQUEST,
    ERROR_METHOD_NOT_FOUND,
    ERROR_MIC_PERMISSION,
    ERROR_PARSE_ERROR,
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


def handle_system_ping(request: Request) -> dict[str, Any]:
    """Handle system.ping request."""
    return {
        "version": __version__,
        "protocol": PROTOCOL_VERSION,
    }


def handle_system_info(request: Request) -> dict[str, Any]:
    """Handle system.info request."""
    # Check for CUDA availability (placeholder - will be enhanced later)
    cuda_available = False
    try:
        import torch  # noqa: F401

        cuda_available = torch.cuda.is_available()
    except ImportError:
        pass

    return {
        "version": __version__,
        "protocol": PROTOCOL_VERSION,
        "capabilities": {
            "cuda_available": cuda_available,
            "supports_progress": True,
            "supports_model_purge": True,
            "supports_silence_trim": True,
            "supports_audio_meter": True,
        },
        "runtime": {
            "python": platform.python_version(),
            "platform": sys.platform,
        },
    }


def handle_system_shutdown(request: Request) -> dict[str, Any]:
    """Handle system.shutdown request."""
    reason = request.params.get("reason", "requested")
    log(f"Shutdown requested: {reason}")
    return {"status": "shutting_down"}


# Method dispatch table
HANDLERS: dict[str, Any] = {
    "system.ping": handle_system_ping,
    "system.info": handle_system_info,
    "system.shutdown": handle_system_shutdown,
    "audio.list_devices": handle_audio_list_devices,
    "audio.set_device": handle_audio_set_device,
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
