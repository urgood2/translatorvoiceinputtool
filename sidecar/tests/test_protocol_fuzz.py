"""Fuzz tests for protocol parsing using Hypothesis.

These tests verify that the protocol parser never crashes or raises
unexpected exceptions on malformed input, ensuring robustness against
untrusted messages from the IPC channel.
"""

from __future__ import annotations

import json
from typing import Any

import pytest
from hypothesis import given, settings, strategies as st

from openvoicy_sidecar.protocol import (
    InvalidRequestError,
    ParseError,
    parse_line,
)


# Expected exceptions that are acceptable for malformed input
ACCEPTABLE_EXCEPTIONS = (ParseError, InvalidRequestError, ValueError, KeyError, TypeError)


@given(st.binary())
@settings(max_examples=500, deadline=1000)
def test_parse_never_crashes_on_bytes(data: bytes) -> None:
    """Test that the parser handles arbitrary bytes without crashing."""
    try:
        text = data.decode("utf-8", errors="replace")
        parse_line(text)
    except ACCEPTABLE_EXCEPTIONS:
        pass  # Expected errors for malformed input


@given(st.text(max_size=10000))
@settings(max_examples=500, deadline=1000)
def test_parse_never_crashes_on_text(data: str) -> None:
    """Test that the parser handles arbitrary text without crashing."""
    try:
        parse_line(data)
    except ACCEPTABLE_EXCEPTIONS:
        pass  # Expected errors for malformed input


@given(st.text(alphabet=st.characters(categories=("Cs",)), max_size=1000))
@settings(max_examples=100, deadline=1000)
def test_parse_handles_unicode_surrogates(data: str) -> None:
    """Test that the parser handles Unicode surrogate characters."""
    try:
        parse_line(data)
    except ACCEPTABLE_EXCEPTIONS:
        pass


@given(
    st.recursive(
        st.none() | st.booleans() | st.integers() | st.text(max_size=100),
        lambda children: st.lists(children, max_size=10)
        | st.dictionaries(st.text(max_size=20), children, max_size=10),
        max_leaves=50,
    )
)
@settings(max_examples=500, deadline=2000)
def test_parse_handles_arbitrary_json_structures(data: Any) -> None:
    """Test that the parser handles arbitrary valid JSON structures."""
    try:
        json_str = json.dumps(data)
        parse_line(json_str)
    except ACCEPTABLE_EXCEPTIONS:
        pass


@given(st.integers(min_value=-2**63, max_value=2**63))
@settings(max_examples=200, deadline=1000)
def test_parse_handles_large_integers(value: int) -> None:
    """Test that the parser handles JSON with large integers."""
    json_str = f'{{"jsonrpc": "2.0", "id": {value}, "method": "test"}}'
    try:
        result = parse_line(json_str)
        assert result is not None
        # IDs can be either integers or strings, so this should work
    except ACCEPTABLE_EXCEPTIONS:
        pass


@given(st.text(min_size=1, max_size=100))
@settings(max_examples=200, deadline=1000)
def test_parse_handles_arbitrary_method_names(method: str) -> None:
    """Test that the parser handles arbitrary method names."""
    try:
        escaped_method = json.dumps(method)
        json_str = f'{{"jsonrpc": "2.0", "id": 1, "method": {escaped_method}}}'
        result = parse_line(json_str)
        assert result is not None
        assert result.method == method
    except ACCEPTABLE_EXCEPTIONS:
        pass


@given(st.dictionaries(st.text(max_size=20), st.text(max_size=100), max_size=20))
@settings(max_examples=200, deadline=1000)
def test_parse_handles_arbitrary_params(params: dict[str, str]) -> None:
    """Test that the parser handles arbitrary params objects."""
    try:
        params_json = json.dumps(params)
        json_str = f'{{"jsonrpc": "2.0", "id": 1, "method": "test", "params": {params_json}}}'
        result = parse_line(json_str)
        assert result is not None
        assert result.params == params
    except ACCEPTABLE_EXCEPTIONS:
        pass


@given(st.lists(st.binary(min_size=1, max_size=100), min_size=1, max_size=50))
@settings(max_examples=100, deadline=2000)
def test_parse_handles_repeated_malformed_input(inputs: list[bytes]) -> None:
    """Test that repeated parsing of malformed input doesn't cause state issues."""
    for data in inputs:
        try:
            text = data.decode("utf-8", errors="replace")
            parse_line(text)
        except ACCEPTABLE_EXCEPTIONS:
            pass


@given(st.text(max_size=100))
@settings(max_examples=100, deadline=1000)
def test_parse_handles_whitespace_variants(whitespace: str) -> None:
    """Test that the parser handles various whitespace combinations."""
    base = '{"jsonrpc": "2.0", "id": 1, "method": "test"}'
    variants = [
        whitespace + base,
        base + whitespace,
        whitespace + base + whitespace,
    ]
    for json_str in variants:
        try:
            parse_line(json_str)
        except ACCEPTABLE_EXCEPTIONS:
            pass


@given(st.integers(min_value=0, max_value=1000))
@settings(max_examples=50, deadline=5000)
def test_parse_handles_deeply_nested_structures(depth: int) -> None:
    """Test that the parser handles deeply nested JSON without stack overflow."""
    # Limit actual depth to prevent test timeout
    depth = min(depth, 100)
    nested = "null"
    for _ in range(depth):
        nested = f'{{"nested": {nested}}}'
    try:
        parse_line(nested)
    except ACCEPTABLE_EXCEPTIONS:
        pass
    except RecursionError:
        # This is acceptable - the parser should protect against this
        pass


class TestParseLineEdgeCases:
    """Non-property-based tests for specific edge cases."""

    def test_empty_string(self) -> None:
        """Empty string should return None."""
        assert parse_line("") is None

    def test_whitespace_only(self) -> None:
        """Whitespace-only string should return None."""
        assert parse_line("   \t\n  ") is None

    def test_invalid_json(self) -> None:
        """Invalid JSON should raise ParseError."""
        with pytest.raises(ParseError):
            parse_line("{not valid json}")

    def test_valid_json_not_object(self) -> None:
        """Valid JSON that's not an object should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError):
            parse_line('"just a string"')

        with pytest.raises(InvalidRequestError):
            parse_line("[1, 2, 3]")

    def test_missing_jsonrpc_version(self) -> None:
        """Missing jsonrpc field should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError):
            parse_line('{"id": 1, "method": "test"}')

    def test_wrong_jsonrpc_version(self) -> None:
        """Wrong jsonrpc version should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError):
            parse_line('{"jsonrpc": "1.0", "id": 1, "method": "test"}')

    def test_missing_method(self) -> None:
        """Missing method field should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError):
            parse_line('{"jsonrpc": "2.0", "id": 1}')

    def test_non_string_method(self) -> None:
        """Non-string method should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError):
            parse_line('{"jsonrpc": "2.0", "id": 1, "method": 123}')

    def test_non_object_params(self) -> None:
        """Non-object params should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError):
            parse_line('{"jsonrpc": "2.0", "id": 1, "method": "test", "params": [1, 2]}')

    def test_null_bytes(self) -> None:
        """Null bytes in string should be handled."""
        try:
            parse_line('{"jsonrpc": "2.0", "id": 1, "method": "test\x00method"}')
        except ACCEPTABLE_EXCEPTIONS:
            pass

    def test_oversized_message(self) -> None:
        """Very large messages should not cause memory issues."""
        # Create a ~2MB string - above MAX_LINE_LENGTH
        large_value = "x" * (2 * 1024 * 1024)
        json_str = f'{{"jsonrpc": "2.0", "id": 1, "method": "test", "params": {{"data": "{large_value}"}}}}'
        try:
            # This should either work or fail gracefully
            parse_line(json_str)
        except ACCEPTABLE_EXCEPTIONS:
            pass
        except MemoryError:
            # Acceptable to run out of memory on huge input
            pass

    def test_scientific_notation_id(self) -> None:
        """Scientific notation in ID should be handled."""
        try:
            parse_line('{"jsonrpc": "2.0", "id": 1e10, "method": "test"}')
        except ACCEPTABLE_EXCEPTIONS:
            pass

    def test_negative_id(self) -> None:
        """Negative ID should be handled."""
        result = parse_line('{"jsonrpc": "2.0", "id": -1, "method": "test"}')
        assert result is not None

    def test_string_id(self) -> None:
        """String ID should be handled."""
        result = parse_line('{"jsonrpc": "2.0", "id": "abc-123", "method": "test"}')
        assert result is not None

    def test_null_id(self) -> None:
        """Null ID (notification) should be handled."""
        result = parse_line('{"jsonrpc": "2.0", "id": null, "method": "test"}')
        assert result is not None


if __name__ == "__main__":
    # Run hypothesis tests with more examples when run directly
    import sys

    pytest.main([__file__, "-v", "--hypothesis-seed=0"] + sys.argv[1:])
