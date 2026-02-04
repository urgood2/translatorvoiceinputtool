"""JSON-RPC 2.0 protocol implementation for IPC communication."""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass, field
from typing import Any

# Maximum line length for incoming messages (1 MiB)
MAX_LINE_LENGTH = 1024 * 1024

# JSON-RPC 2.0 error codes
ERROR_PARSE_ERROR = -32700
ERROR_INVALID_REQUEST = -32600
ERROR_METHOD_NOT_FOUND = -32601
ERROR_INVALID_PARAMS = -32602
ERROR_INTERNAL = -32603

# Application-specific error codes
ERROR_NOT_READY = -32001
ERROR_MIC_PERMISSION = -32002
ERROR_DEVICE_NOT_FOUND = -32003
ERROR_AUDIO_IO = -32004
ERROR_NETWORK = -32005
ERROR_DISK_FULL = -32006
ERROR_CACHE_CORRUPT = -32007
ERROR_MODEL_LOAD = -32008
ERROR_TRANSCRIBE = -32009


@dataclass
class Request:
    """JSON-RPC 2.0 request."""

    method: str
    id: str | int | None
    params: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Request:
        """Parse a request from a dictionary."""
        return cls(
            method=data.get("method", ""),
            id=data.get("id"),
            params=data.get("params", {}),
        )


@dataclass
class Response:
    """JSON-RPC 2.0 response."""

    id: str | int | None
    result: Any | None = None
    error: dict[str, Any] | None = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to JSON-RPC 2.0 response dict."""
        response: dict[str, Any] = {"jsonrpc": "2.0", "id": self.id}
        if self.error is not None:
            response["error"] = self.error
        else:
            response["result"] = self.result
        return response

    def to_json(self) -> str:
        """Serialize to JSON string."""
        return json.dumps(self.to_dict(), separators=(",", ":"))


@dataclass
class Notification:
    """JSON-RPC 2.0 notification (no id, no response expected)."""

    method: str
    params: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        """Convert to JSON-RPC 2.0 notification dict."""
        return {"jsonrpc": "2.0", "method": self.method, "params": self.params}

    def to_json(self) -> str:
        """Serialize to JSON string."""
        return json.dumps(self.to_dict(), separators=(",", ":"))


def make_error(
    request_id: str | int | None,
    code: int,
    message: str,
    kind: str,
    details: Any | None = None,
) -> Response:
    """Create an error response."""
    error_data: dict[str, Any] = {"kind": kind}
    if details is not None:
        error_data["details"] = details

    return Response(
        id=request_id,
        error={
            "code": code,
            "message": message,
            "data": error_data,
        },
    )


def make_success(request_id: str | int | None, result: Any) -> Response:
    """Create a success response."""
    return Response(id=request_id, result=result)


def parse_line(line: str) -> Request | None:
    """Parse a line of NDJSON into a Request.

    Returns None if the line is empty or whitespace only.
    Raises ValueError for parse errors.
    """
    line = line.strip()
    if not line:
        return None

    try:
        data = json.loads(line)
    except json.JSONDecodeError as e:
        raise ValueError(f"Invalid JSON: {e}") from e

    if not isinstance(data, dict):
        raise ValueError("Request must be a JSON object")

    if data.get("jsonrpc") != "2.0":
        raise ValueError("Invalid or missing jsonrpc version")

    if "method" not in data:
        raise ValueError("Missing method field")

    return Request.from_dict(data)


def write_response(response: Response) -> None:
    """Write a response to stdout and flush."""
    sys.stdout.write(response.to_json())
    sys.stdout.write("\n")
    sys.stdout.flush()


def write_notification(notification: Notification) -> None:
    """Write a notification to stdout and flush."""
    sys.stdout.write(notification.to_json())
    sys.stdout.write("\n")
    sys.stdout.flush()


def log(message: str) -> None:
    """Log a message to stderr."""
    print(message, file=sys.stderr, flush=True)
