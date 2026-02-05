"""Fuzz tests for the JSON-RPC protocol parser using Hypothesis."""

import json

import pytest

try:
    from hypothesis import given, settings, strategies as st
except ImportError:
    pytest.skip("hypothesis not installed", allow_module_level=True)

from openvoicy_sidecar.protocol import (
    Request,
    Response,
    Notification,
    parse_line,
    ParseError,
    InvalidRequestError,
)


# Strategy for generating valid JSON-RPC request objects
json_rpc_request = st.fixed_dictionaries(
    {
        "jsonrpc": st.just("2.0"),
        "method": st.text(min_size=1, max_size=100),
        "id": st.one_of(st.integers(), st.text(max_size=50), st.none()),
    },
    optional={"params": st.dictionaries(st.text(max_size=20), st.text(max_size=100))},
)

# Strategy for generating arbitrary JSON values
json_value = st.recursive(
    st.one_of(
        st.none(),
        st.booleans(),
        st.integers(),
        st.floats(allow_nan=False, allow_infinity=False),
        st.text(max_size=100),
    ),
    lambda children: st.one_of(
        st.lists(children, max_size=5),
        st.dictionaries(st.text(max_size=20), children, max_size=5),
    ),
    max_leaves=10,
)

# Strategy for arbitrary bytes (to test malformed input)
arbitrary_bytes = st.binary(max_size=1024)


@settings(max_examples=1000, deadline=None)
@given(st.text(max_size=10000))
def test_parse_arbitrary_text_doesnt_crash(text: str) -> None:
    """The parser should never crash on arbitrary text input."""
    try:
        result = parse_line(text)
        # If parsing succeeds, result should be a valid type
        assert result is None or isinstance(result, (Request, Response, Notification, dict))
    except (json.JSONDecodeError, ValueError, ParseError, InvalidRequestError):
        # Expected for invalid JSON or JSON-RPC structure
        pass
    except Exception as e:
        # Unexpected exceptions should be reported
        pytest.fail(f"Unexpected exception: {type(e).__name__}: {e}")


@settings(max_examples=500, deadline=None)
@given(json_rpc_request)
def test_parse_valid_request_structure(request_dict: dict) -> None:
    """Valid JSON-RPC request structures should parse correctly."""
    json_str = json.dumps(request_dict)
    try:
        result = parse_line(json_str)
        # Should either parse successfully or reject gracefully
        assert result is None or isinstance(result, (Request, dict))
    except (json.JSONDecodeError, ValueError, KeyError, ParseError, InvalidRequestError):
        # These are acceptable rejection modes
        pass


@settings(max_examples=500, deadline=None)
@given(json_value)
def test_parse_arbitrary_json_doesnt_crash(value) -> None:
    """The parser should handle arbitrary valid JSON without crashing."""
    try:
        json_str = json.dumps(value)
        result = parse_line(json_str)
        # Non-object JSON should be rejected gracefully
        if not isinstance(value, dict):
            assert result is None or isinstance(result, dict)
    except (json.JSONDecodeError, ValueError, TypeError, ParseError, InvalidRequestError):
        # Acceptable rejection modes
        pass
    except Exception as e:
        pytest.fail(f"Unexpected exception: {type(e).__name__}: {e}")


@settings(max_examples=200, deadline=None)
@given(st.integers(min_value=0, max_value=10000))
def test_request_from_dict_with_varying_ids(req_id: int) -> None:
    """Request.from_dict should handle various ID values."""
    data = {"method": "test.method", "id": req_id, "params": {}}
    request = Request.from_dict(data)
    assert request.id == req_id
    assert request.method == "test.method"


@settings(max_examples=200, deadline=None)
@given(st.text(min_size=1, max_size=200))
def test_request_from_dict_with_varying_methods(method: str) -> None:
    """Request.from_dict should handle various method names."""
    data = {"method": method, "id": 1, "params": {}}
    request = Request.from_dict(data)
    assert request.method == method


@settings(max_examples=200, deadline=None)
@given(st.dictionaries(st.text(max_size=20), st.text(max_size=100), max_size=10))
def test_response_round_trip(params: dict) -> None:
    """Response should serialize and deserialize correctly."""
    response = Response(id=1, result=params)
    json_str = response.to_json()
    parsed = json.loads(json_str)
    assert parsed["jsonrpc"] == "2.0"
    assert parsed["id"] == 1
    assert parsed["result"] == params


@settings(max_examples=200, deadline=None)
@given(st.text(min_size=1, max_size=100), st.dictionaries(st.text(max_size=20), st.text(max_size=100), max_size=5))
def test_notification_round_trip(method: str, params: dict) -> None:
    """Notification should serialize correctly."""
    notification = Notification(method=method, params=params)
    json_str = notification.to_json()
    parsed = json.loads(json_str)
    assert parsed["jsonrpc"] == "2.0"
    assert parsed["method"] == method
    assert parsed["params"] == params
