#!/usr/bin/env python3
"""
Validate IPC_V1_EXAMPLES.jsonl against the protocol specification.

This script:
1. Validates all JSONL lines parse correctly
2. Validates JSON-RPC 2.0 structure
3. Validates error codes are in valid ranges
4. Validates all error.data.kind values are from the allowed set
5. Validates message types (request, response, notification, error)
6. Validates method-level request/response payloads against sidecar.rpc.v1 schemas
7. Validates status.get fixtures include idle behavior with and without a model object
8. Warns on duplicate fixture corpora and fails on conflicting duplicates

Exit codes:
  0 - All validations passed
  1 - Validation errors found
"""

import json
import re
import sys
from pathlib import Path
from typing import Any

from jsonschema import Draft7Validator, RefResolver

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

METHOD_NAME_RE = re.compile(r"([a-z]+\.[a-z_]+)")
CANONICAL_FIXTURE_REL = Path("shared/ipc/examples/IPC_V1_EXAMPLES.jsonl")
DERIVED_FIXTURE_DIR_REL = Path("shared/contracts/examples")


def normalize_text(text: str) -> str:
    return text if text.endswith("\n") else f"{text}\n"


def detect_duplicate_fixture_corpora(repo_root: Path) -> tuple[list[str], list[str]]:
    """Warn on duplicate fixture corpora; fail when duplicates conflict with canonical source."""
    warnings: list[str] = []
    errors: list[str] = []

    canonical = repo_root / CANONICAL_FIXTURE_REL
    derived_dir = repo_root / DERIVED_FIXTURE_DIR_REL

    if not derived_dir.exists():
        return warnings, errors

    derived_files = sorted(path for path in derived_dir.glob("*.jsonl") if path.is_file())
    if not derived_files:
        return warnings, errors

    if not canonical.exists():
        errors.append(f"Missing canonical fixture corpus: {CANONICAL_FIXTURE_REL}")
        return warnings, errors

    canonical_text = normalize_text(canonical.read_text(encoding="utf-8"))
    for path in derived_files:
        rel = path.relative_to(repo_root)
        warnings.append(
            f"Duplicate fixture corpus detected: {rel}; "
            f"canonical source remains {CANONICAL_FIXTURE_REL}"
        )

        derived_text = normalize_text(path.read_text(encoding="utf-8"))
        if derived_text != canonical_text:
            errors.append(
                f"Conflicting fixture corpus detected: {rel} diverges from canonical {CANONICAL_FIXTURE_REL}"
            )

    return warnings, errors


def load_contract_inventory(contract_file: Path) -> dict[str, Any]:
    """Load method/notification schemas and coverage metadata from sidecar.rpc.v1 contract."""
    contract_data = json.loads(contract_file.read_text())
    methods: dict[str, dict[str, Any]] = {}
    notifications: dict[str, dict[str, Any]] = {}
    required_methods: set[str] = set()

    for item in contract_data.get("items", []):
        item_type = item.get("type")
        name = item.get("name")
        if not isinstance(name, str):
            continue

        if item_type == "method":
            methods[name] = {
                "params_schema": item.get("params_schema"),
                "result_schema": item.get("result_schema"),
                "required": item.get("required") is True,
            }
            if item.get("required") is True:
                required_methods.add(name)
        elif item_type == "notification":
            notifications[name] = {
                "params_schema": item.get("params_schema"),
            }

    return {
        "contract": contract_data,
        "methods": methods,
        "notifications": notifications,
        "required_methods": required_methods,
        "optional_methods": set(methods.keys()) - required_methods,
    }


def load_contract_method_schemas(contract_file: Path) -> dict[str, dict[str, Any]]:
    """Load method -> params/result schema objects from sidecar.rpc.v1 contract."""
    return load_contract_inventory(contract_file)["methods"]


def _json_type_matches(value: Any, expected_type: str) -> bool:
    if expected_type == "object":
        return isinstance(value, dict)
    if expected_type == "array":
        return isinstance(value, list)
    if expected_type == "string":
        return isinstance(value, str)
    if expected_type == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if expected_type == "number":
        return (isinstance(value, int) or isinstance(value, float)) and not isinstance(value, bool)
    if expected_type == "boolean":
        return isinstance(value, bool)
    if expected_type == "null":
        return value is None
    return True


def _format_type_decl(schema: dict[str, Any]) -> str:
    schema_type = schema.get("type")
    if isinstance(schema_type, list):
        return "|".join(str(t) for t in schema_type)
    return str(schema_type)


def validate_schema_value(value: Any, schema: Any, path: str) -> list[str]:
    """Validate a value against the contract schema subset used by sidecar.rpc.v1."""
    if not isinstance(schema, dict):
        return []

    errors: list[str] = []

    if "enum" in schema and value not in schema["enum"]:
        errors.append(f"{path}: expected one of {schema['enum']}, got {value!r}")

    expected_types = schema.get("type")
    if expected_types is not None:
        type_list = expected_types if isinstance(expected_types, list) else [expected_types]
        if not any(_json_type_matches(value, t) for t in type_list):
            errors.append(f"{path}: expected type {_format_type_decl(schema)}, got {type(value).__name__}")
            return errors

    if isinstance(value, dict):
        required = schema.get("required", [])
        for key in required:
            if key not in value:
                errors.append(f"{path}: missing required field '{key}'")

        properties = schema.get("properties", {})
        if isinstance(properties, dict):
            for key, sub_schema in properties.items():
                if key in value:
                    errors.extend(validate_schema_value(value[key], sub_schema, f"{path}.{key}"))

            additional = schema.get("additionalProperties", True)
            if additional is False:
                extra_keys = sorted(key for key in value if key not in properties)
                for key in extra_keys:
                    errors.append(f"{path}: unexpected field '{key}'")
            elif isinstance(additional, dict):
                for key in value:
                    if key not in properties:
                        errors.extend(validate_schema_value(value[key], additional, f"{path}.{key}"))

    if isinstance(value, list):
        item_schema = schema.get("items")
        if item_schema is not None:
            for idx, item in enumerate(value):
                errors.extend(validate_schema_value(item, item_schema, f"{path}[{idx}]"))

    if isinstance(value, str) and "minLength" in schema:
        if len(value) < schema["minLength"]:
            errors.append(f"{path}: expected minLength {schema['minLength']}, got {len(value)}")

    if (isinstance(value, int) or isinstance(value, float)) and not isinstance(value, bool):
        if "minimum" in schema and value < schema["minimum"]:
            errors.append(f"{path}: expected minimum {schema['minimum']}, got {value}")
        if "maximum" in schema and value > schema["maximum"]:
            errors.append(f"{path}: expected maximum {schema['maximum']}, got {value}")

    return errors


def infer_response_method(
    msg_id: Any, comment: str, request_method_by_id: dict[Any, str], known_methods: set[str]
) -> str | None:
    """Infer method for response examples from request id mapping or comment text."""
    method_name = request_method_by_id.get(msg_id)
    if method_name:
        return method_name

    match = METHOD_NAME_RE.search(comment)
    if not match:
        return None

    candidate = match.group(1)
    if candidate in known_methods:
        return candidate
    return None


def _format_validation_error(err: Any, path_prefix: str) -> str:
    location = ".".join(str(p) for p in err.path)
    prefix = f"{path_prefix}.{location}" if location else path_prefix

    if err.validator == "required":
        marker = "'"
        if marker in err.message:
            field = err.message.split(marker, 2)[1]
            return f"{prefix}: missing required field '{field}'"
    if err.validator == "additionalProperties":
        marker = "('"
        if marker in err.message:
            field = err.message.split(marker, 1)[1].split("'", 1)[0]
            return f"{prefix}: unexpected field '{field}'"
    if err.validator == "enum":
        return f"{prefix}: expected one of {err.validator_value}, got {err.instance!r}"
    if err.validator == "minLength":
        return f"{prefix}: minLength violation ({err.message})"
    return f"{prefix}: {err.message}"


def validate_against_schema(instance: Any, schema: Any, root_schema: dict[str, Any], path_prefix: str) -> list[str]:
    if not isinstance(schema, dict):
        return []
    resolver = RefResolver.from_schema(root_schema)
    validator = Draft7Validator(schema, resolver=resolver)
    return [_format_validation_error(err, path_prefix) for err in validator.iter_errors(instance)]


def validate_method_level_contract_shapes(examples_file: Path, contract_file: Path) -> list[str]:
    """Validate request/response/notification payloads against sidecar.rpc.v1 schemas."""
    errors: list[str] = []

    try:
        inventory = load_contract_inventory(contract_file)
    except Exception as e:
        return [f"Contract parse error: {e}"]
    contract_data = inventory["contract"]
    method_schemas = inventory["methods"]
    notification_schemas = inventory["notifications"]

    request_method_by_id: dict[Any, str] = {}
    lines: list[tuple[int, dict[str, Any]]] = []

    for line_num, line in enumerate(examples_file.read_text().splitlines(), 1):
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue
        lines.append((line_num, obj))

    for _line_num, obj in lines:
        if obj.get("type") != "request":
            continue
        data = obj.get("data", {})
        msg_id = data.get("id")
        method_name = data.get("method")
        if msg_id is not None and isinstance(method_name, str):
            request_method_by_id[msg_id] = method_name

    known_methods = set(method_schemas.keys())
    known_notifications = set(notification_schemas.keys())

    for line_num, obj in lines:
        msg_type = obj.get("type")
        data = obj.get("data", {})
        comment = str(obj.get("_comment", ""))

        if msg_type == "request":
            method_name = data.get("method")
            if not isinstance(method_name, str):
                continue
            method_schema = method_schemas.get(method_name)
            if not method_schema:
                continue

            params = data.get("params", {})
            params_errors = validate_against_schema(
                params,
                method_schema.get("params_schema"),
                contract_data,
                f"{method_name}.params",
            )
            errors.extend(f"Line {line_num}: {err}" for err in params_errors)
            continue

        if msg_type == "response":
            if "result" not in data:
                continue
            method_name = infer_response_method(data.get("id"), comment, request_method_by_id, known_methods)
            if not method_name:
                continue
            method_schema = method_schemas.get(method_name)
            if not method_schema:
                continue

            result_errors = validate_against_schema(
                data["result"],
                method_schema.get("result_schema"),
                contract_data,
                f"{method_name}.result",
            )
            errors.extend(f"Line {line_num}: {err}" for err in result_errors)
            continue

        if msg_type == "notification":
            method_name = data.get("method")
            if not isinstance(method_name, str):
                continue
            if method_name not in known_notifications:
                continue
            params = data.get("params", {})
            params_errors = validate_against_schema(
                params,
                notification_schemas[method_name].get("params_schema"),
                contract_data,
                f"{method_name}.params",
            )
            errors.extend(f"Line {line_num}: {err}" for err in params_errors)

    return errors


def validate_contract_method_coverage(examples_file: Path, contract_file: Path) -> tuple[list[str], int, int]:
    """Validate fixture request coverage against contract method inventory."""
    inventory = load_contract_inventory(contract_file)
    methods = inventory["methods"]
    required_methods = inventory["required_methods"]
    errors: list[str] = []

    seen_request_methods: set[str] = set()
    for line_num, line in enumerate(examples_file.read_text().splitlines(), 1):
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue

        if obj.get("type") != "request":
            continue
        data = obj.get("data", {})
        method_name = data.get("method")
        if not isinstance(method_name, str):
            continue
        if method_name not in methods:
            errors.append(f"Line {line_num}: Unknown request method in fixture: {method_name}")
            continue
        seen_request_methods.add(method_name)

    missing_required = sorted(required_methods - seen_request_methods)
    for method in missing_required:
        errors.append(f"Missing fixture request for required method '{method}'")

    return errors, len(seen_request_methods), len(methods)


def validate_status_get_idle_fixture_variants(examples_file: Path) -> list[str]:
    """Validate status.get fixture coverage for idle state with and without model data."""
    errors: list[str] = []
    request_method_by_id: dict[Any, str] = {}
    response_entries: list[dict[str, Any]] = []

    for line in examples_file.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            continue

        data = obj.get("data", {})
        msg_type = obj.get("type")
        msg_id = data.get("id")
        if msg_type == "request" and msg_id is not None and "method" in data:
            request_method_by_id[msg_id] = data["method"]
        elif msg_type == "response":
            response_entries.append(obj)

    has_idle_with_model = False
    has_idle_without_model = False

    for obj in response_entries:
        data = obj.get("data", {})
        msg_id = data.get("id")
        result = data.get("result")
        if not isinstance(result, dict):
            continue

        if result.get("state") != "idle":
            continue

        method_name = request_method_by_id.get(msg_id)
        comment = str(obj.get("_comment", ""))
        if method_name != "status.get" and "status.get" not in comment:
            continue

        if "model" in result:
            has_idle_with_model = True
        else:
            has_idle_without_model = True

    if not has_idle_with_model:
        errors.append("Missing status.get idle response fixture with model object")
    if not has_idle_without_model:
        errors.append("Missing status.get idle response fixture without model object")

    return errors


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


def validate_jsonrpc_request(data: dict[str, Any], line_num: int, request_methods: set[str] | None = None) -> list[str]:
    """Validate JSON-RPC request structure."""
    errors = []

    if "id" not in data:
        errors.append(f"Line {line_num}: Request missing 'id' field")

    if "method" not in data:
        errors.append(f"Line {line_num}: Request missing 'method' field")
    else:
        method = data["method"]
        if request_methods is not None and method not in request_methods:
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


def validate_jsonrpc_notification(
    data: dict[str, Any],
    line_num: int,
    notification_methods: set[str] | None = None,
) -> list[str]:
    """Validate JSON-RPC notification structure."""
    errors = []

    if "id" in data:
        errors.append(f"Line {line_num}: Notification must not have 'id' field")

    if "method" not in data:
        errors.append(f"Line {line_num}: Notification missing 'method' field")
    else:
        method = data["method"]
        if notification_methods is not None and method not in notification_methods:
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


def validate_example(
    obj: dict[str, Any],
    line_num: int,
    request_methods: set[str] | None = None,
    notification_methods: set[str] | None = None,
) -> list[str]:
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
        errors.extend(validate_jsonrpc_request(data, line_num, request_methods))
    elif msg_type == "response":
        errors.extend(validate_jsonrpc_response(data, line_num))
    elif msg_type == "notification":
        errors.extend(validate_jsonrpc_notification(data, line_num, notification_methods))
    elif msg_type == "error":
        errors.extend(validate_jsonrpc_error(data, line_num))

    return errors


def main() -> int:
    """Main validation function."""
    # Find the examples file
    script_dir = Path(__file__).parent
    repo_root = script_dir.parent
    examples_file = repo_root / "shared" / "ipc" / "examples" / "IPC_V1_EXAMPLES.jsonl"
    contract_file = repo_root / "shared" / "contracts" / "sidecar.rpc.v1.json"

    if not examples_file.exists():
        print(f"ERROR: Examples file not found: {examples_file}", file=sys.stderr)
        return 1

    if not contract_file.exists():
        print(f"ERROR: Contract file not found: {contract_file}", file=sys.stderr)
        return 1

    try:
        contract_inventory = load_contract_inventory(contract_file)
    except Exception as e:
        print(f"ERROR: Contract parse error: {e}", file=sys.stderr)
        return 1
    request_methods = set(contract_inventory["methods"].keys())
    notification_methods = set(contract_inventory["notifications"].keys())
    required_methods = set(contract_inventory["required_methods"])
    optional_methods = set(contract_inventory["optional_methods"])
    print(
        "Contract defines "
        f"{len(request_methods)} methods ({len(required_methods)} required, {len(optional_methods)} optional)"
    )

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
            errors = validate_example(obj, line_num, request_methods, notification_methods)
            all_errors.extend(errors)

            # Update stats
            if "type" in obj and obj["type"] in stats:
                stats[obj["type"]] += 1

    all_errors.extend(validate_method_level_contract_shapes(examples_file, contract_file))
    coverage_errors, covered_count, total_methods = validate_contract_method_coverage(examples_file, contract_file)
    all_errors.extend(coverage_errors)
    all_errors.extend(validate_status_get_idle_fixture_variants(examples_file))
    duplicate_warnings, duplicate_errors = detect_duplicate_fixture_corpora(repo_root)
    for warning in duplicate_warnings:
        print(f"WARNING: {warning}")
    all_errors.extend(duplicate_errors)
    print(f"Fixture covers {covered_count}/{total_methods} methods")

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
