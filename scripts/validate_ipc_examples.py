#!/usr/bin/env python3
"""
Validate IPC_V1_EXAMPLES.jsonl against the protocol specification.

This script:
1. Validates all JSONL lines parse correctly
2. Validates JSON-RPC 2.0 structure
3. Validates error codes are in valid ranges
4. Validates all error.data.kind values are from the allowed set
5. Validates message types (request, response, notification, error)

Exit codes:
  0 - All validations passed
  1 - Validation errors found
"""

import json
import sys
from pathlib import Path
from typing import Any

# Valid error kind strings from the protocol
VALID_ERROR_KINDS = {
    "E_METHOD_NOT_FOUND",
    "E_INVALID_PARAMS",
    "E_NOT_READY",
    "E_MIC_PERMISSION",
    "E_DEVICE_NOT_FOUND",
    "E_AUDIO_IO",
    "E_NETWORK",
    "E_DISK_FULL",
    "E_CACHE_CORRUPT",
    "E_MODEL_LOAD",
    "E_TRANSCRIBE",
    "E_INTERNAL",
}

# Valid JSON-RPC 2.0 error codes
JSONRPC_STANDARD_CODES = {-32700, -32600, -32601, -32602, -32603}
JSONRPC_SERVER_ERROR_RANGE = range(-32099, -31999)  # -32099 to -32000

# Valid message types
VALID_MESSAGE_TYPES = {"request", "response", "notification", "error"}

# Valid method prefixes
VALID_METHOD_PREFIXES = {"system", "audio", "model", "asr", "recording", "replacements", "status", "event"}

# Valid notification methods
VALID_NOTIFICATION_METHODS = {
    "event.status_changed",
    "event.audio_level",
    "event.transcription_complete",
    "event.transcription_error",
}

# Valid request methods
VALID_REQUEST_METHODS = {
    "system.ping",
    "system.info",
    "system.shutdown",
    "audio.list_devices",
    "audio.set_device",
    "audio.meter_start",
    "audio.meter_stop",
    "audio.meter_status",
    "model.get_status",
    "model.download",
    "model.purge_cache",
    "asr.initialize",
    "asr.status",
    "asr.transcribe",
    "recording.start",
    "recording.stop",
    "recording.cancel",
    "recording.status",
    "replacements.get_rules",
    "replacements.set_rules",
    "replacements.get_presets",
    "replacements.get_preset_rules",
    "replacements.preview",
    "status.get",
}


def validate_error_code(code: int) -> str | None:
    """Validate error code is in valid range."""
    if code in JSONRPC_STANDARD_CODES:
        return None
    if code in JSONRPC_SERVER_ERROR_RANGE:
        return None
    return f"Invalid error code: {code}"


def validate_error_kind(kind: str) -> str | None:
    """Validate error kind is in allowed set."""
    if kind in VALID_ERROR_KINDS:
        return None
    return f"Invalid error kind: {kind}"


def validate_jsonrpc_request(data: dict[str, Any], line_num: int) -> list[str]:
    """Validate JSON-RPC request structure."""
    errors = []

    if "id" not in data:
        errors.append(f"Line {line_num}: Request missing 'id' field")

    if "method" not in data:
        errors.append(f"Line {line_num}: Request missing 'method' field")
    else:
        method = data["method"]
        if method not in VALID_REQUEST_METHODS:
            errors.append(f"Line {line_num}: Unknown request method: {method}")

    return errors


def validate_jsonrpc_response(data: dict[str, Any], line_num: int) -> list[str]:
    """Validate JSON-RPC response structure."""
    errors = []

    if "id" not in data:
        errors.append(f"Line {line_num}: Response missing 'id' field")

    if "result" not in data and "error" not in data:
        errors.append(f"Line {line_num}: Response must have 'result' or 'error'")

    if "result" in data and "error" in data:
        errors.append(f"Line {line_num}: Response cannot have both 'result' and 'error'")

    return errors


def validate_jsonrpc_notification(data: dict[str, Any], line_num: int) -> list[str]:
    """Validate JSON-RPC notification structure."""
    errors = []

    if "id" in data:
        errors.append(f"Line {line_num}: Notification must not have 'id' field")

    if "method" not in data:
        errors.append(f"Line {line_num}: Notification missing 'method' field")
    else:
        method = data["method"]
        if method not in VALID_NOTIFICATION_METHODS:
            errors.append(f"Line {line_num}: Unknown notification method: {method}")

    if "params" not in data:
        errors.append(f"Line {line_num}: Notification missing 'params' field")

    return errors


def validate_jsonrpc_error(data: dict[str, Any], line_num: int) -> list[str]:
    """Validate JSON-RPC error response structure."""
    errors = []

    if "error" not in data:
        errors.append(f"Line {line_num}: Error response missing 'error' field")
        return errors

    error = data["error"]

    if "code" not in error:
        errors.append(f"Line {line_num}: Error missing 'code' field")
    else:
        code_err = validate_error_code(error["code"])
        if code_err:
            errors.append(f"Line {line_num}: {code_err}")

    if "message" not in error:
        errors.append(f"Line {line_num}: Error missing 'message' field")

    if "data" in error:
        error_data = error["data"]
        if "kind" in error_data:
            kind_err = validate_error_kind(error_data["kind"])
            if kind_err:
                errors.append(f"Line {line_num}: {kind_err}")

    return errors


def validate_example(obj: dict[str, Any], line_num: int) -> list[str]:
    """Validate a single example object."""
    errors = []

    # Check required fields
    if "_comment" not in obj:
        errors.append(f"Line {line_num}: Missing '_comment' field")

    if "type" not in obj:
        errors.append(f"Line {line_num}: Missing 'type' field")
        return errors

    msg_type = obj["type"]
    if msg_type not in VALID_MESSAGE_TYPES:
        errors.append(f"Line {line_num}: Invalid type '{msg_type}'")

    if "data" not in obj:
        errors.append(f"Line {line_num}: Missing 'data' field")
        return errors

    data = obj["data"]

    # Check JSON-RPC version
    if "jsonrpc" not in data or data["jsonrpc"] != "2.0":
        errors.append(f"Line {line_num}: Invalid or missing jsonrpc version")

    # Type-specific validation
    if msg_type == "request":
        errors.extend(validate_jsonrpc_request(data, line_num))
    elif msg_type == "response":
        errors.extend(validate_jsonrpc_response(data, line_num))
    elif msg_type == "notification":
        errors.extend(validate_jsonrpc_notification(data, line_num))
    elif msg_type == "error":
        errors.extend(validate_jsonrpc_error(data, line_num))

    return errors


def main() -> int:
    """Main validation function."""
    # Find the examples file
    script_dir = Path(__file__).parent
    repo_root = script_dir.parent
    examples_file = repo_root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"

    if not examples_file.exists():
        print(f"ERROR: Examples file not found: {examples_file}", file=sys.stderr)
        return 1

    all_errors: list[str] = []
    line_count = 0

    # Statistics
    stats = {
        "request": 0,
        "response": 0,
        "notification": 0,
        "error": 0,
    }

    with open(examples_file, "r") as f:
        for line_num, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue

            line_count += 1

            # Parse JSON
            try:
                obj = json.loads(line)
            except json.JSONDecodeError as e:
                all_errors.append(f"Line {line_num}: JSON parse error: {e}")
                continue

            # Validate structure
            errors = validate_example(obj, line_num)
            all_errors.extend(errors)

            # Update stats
            if "type" in obj and obj["type"] in stats:
                stats[obj["type"]] += 1

    # Print results
    if all_errors:
        print("VALIDATION FAILED", file=sys.stderr)
        print("-" * 40, file=sys.stderr)
        for err in all_errors:
            print(f"  {err}", file=sys.stderr)
        print("-" * 40, file=sys.stderr)
        print(f"Total errors: {len(all_errors)}", file=sys.stderr)
        return 1

    print("✓ IPC Examples Validation Passed")
    print(f"  Total lines: {line_count}")
    print(f"  Requests: {stats['request']}")
    print(f"  Responses: {stats['response']}")
    print(f"  Notifications: {stats['notification']}")
    print(f"  Errors: {stats['error']}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
