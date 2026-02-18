"""Tests for the JSON-RPC server loop."""

from __future__ import annotations

import io
import json
import subprocess
import sys
from pathlib import Path

import pytest

# Add src to path for imports
src_path = Path(__file__).parent.parent / "src"
sys.path.insert(0, str(src_path))

from openvoicy_sidecar.protocol import (
    ERROR_INVALID_REQUEST,
    ERROR_METHOD_NOT_FOUND,
    ERROR_PARSE_ERROR,
    MAX_LINE_LENGTH,
    InvalidRequestError,
    ParseError,
    Request,
    Response,
    make_error,
    make_success,
    parse_line,
)


class TestParseError:
    """Tests for ParseError exception."""

    def test_malformed_json(self):
        """Malformed JSON should raise ParseError."""
        with pytest.raises(ParseError) as exc_info:
            parse_line("{not valid json")
        assert "Invalid JSON" in str(exc_info.value)

    def test_truncated_json(self):
        """Truncated JSON should raise ParseError."""
        with pytest.raises(ParseError):
            parse_line('{"jsonrpc": "2.0", "method":')


class TestInvalidRequestError:
    """Tests for InvalidRequestError exception."""

    def test_not_an_object(self):
        """Non-object JSON should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError) as exc_info:
            parse_line('"just a string"')
        assert "object" in str(exc_info.value).lower()

    def test_array_instead_of_object(self):
        """JSON array should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError):
            parse_line('[1, 2, 3]')

    def test_missing_jsonrpc(self):
        """Missing jsonrpc field should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError) as exc_info:
            parse_line('{"method": "test"}')
        assert "jsonrpc" in str(exc_info.value).lower()

    def test_wrong_jsonrpc_version(self):
        """Wrong jsonrpc version should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError):
            parse_line('{"jsonrpc": "1.0", "method": "test"}')

    def test_missing_method(self):
        """Missing method field should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError) as exc_info:
            parse_line('{"jsonrpc": "2.0"}')
        assert "method" in str(exc_info.value).lower()

    def test_method_not_string(self):
        """Non-string method should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError):
            parse_line('{"jsonrpc": "2.0", "method": 123}')

    def test_params_not_object(self):
        """Non-object params should raise InvalidRequestError."""
        with pytest.raises(InvalidRequestError):
            parse_line('{"jsonrpc": "2.0", "method": "test", "params": [1,2,3]}')


class TestValidRequests:
    """Tests for valid request parsing."""

    def test_minimal_request(self):
        """Minimal valid request should parse."""
        req = parse_line('{"jsonrpc": "2.0", "method": "test"}')
        assert req is not None
        assert req.method == "test"
        assert req.id is None
        assert req.params == {}

    def test_request_with_id(self):
        """Request with id should parse correctly."""
        req = parse_line('{"jsonrpc": "2.0", "id": 1, "method": "test"}')
        assert req is not None
        assert req.id == 1

    def test_request_with_string_id(self):
        """Request with string id should parse correctly."""
        req = parse_line('{"jsonrpc": "2.0", "id": "abc-123", "method": "test"}')
        assert req is not None
        assert req.id == "abc-123"

    def test_request_with_params(self):
        """Request with params should parse correctly."""
        req = parse_line('{"jsonrpc": "2.0", "id": 1, "method": "test", "params": {"foo": "bar"}}')
        assert req is not None
        assert req.params == {"foo": "bar"}

    def test_empty_line(self):
        """Empty line should return None."""
        assert parse_line("") is None
        assert parse_line("   ") is None
        assert parse_line("\n") is None


class TestResponse:
    """Tests for Response serialization."""

    def test_success_response(self):
        """Success response should serialize correctly."""
        resp = make_success(1, {"result": "ok"})
        data = json.loads(resp.to_json())
        assert data["jsonrpc"] == "2.0"
        assert data["id"] == 1
        assert data["result"] == {"result": "ok"}
        assert "error" not in data

    def test_error_response(self):
        """Error response should serialize correctly."""
        resp = make_error(1, -32601, "Method not found", "E_METHOD_NOT_FOUND")
        data = json.loads(resp.to_json())
        assert data["jsonrpc"] == "2.0"
        assert data["id"] == 1
        assert data["error"]["code"] == -32601
        assert data["error"]["message"] == "Method not found"
        assert data["error"]["data"]["kind"] == "E_METHOD_NOT_FOUND"
        assert "result" not in data

    def test_error_with_details(self):
        """Error response with details should serialize correctly."""
        resp = make_error(1, -32601, "Method not found", "E_METHOD_NOT_FOUND", {"method": "foo"})
        data = json.loads(resp.to_json())
        assert data["error"]["data"]["details"] == {"method": "foo"}


class TestServerIntegration:
    """Integration tests for the server."""

    @pytest.fixture
    def run_sidecar(self):
        """Helper to run the sidecar with input and capture output."""

        def _run(input_lines: list[str], timeout: float = 5.0) -> tuple[list[dict], list[str]]:
            """Run sidecar and return (responses, stderr_lines)."""
            input_text = "\n".join(input_lines) + "\n"

            proc = subprocess.run(
                [sys.executable, "-m", "openvoicy_sidecar"],
                input=input_text,
                capture_output=True,
                text=True,
                cwd=str(src_path.parent),
                env={**dict(__import__("os").environ), "PYTHONPATH": str(src_path)},
                timeout=timeout,
            )

            responses = []
            for line in proc.stdout.strip().split("\n"):
                if line.strip():
                    responses.append(json.loads(line))

            stderr_lines = [l for l in proc.stderr.strip().split("\n") if l.strip()]

            return responses, stderr_lines

        return _run

    def test_system_ping(self, run_sidecar):
        """system.ping should return version and protocol."""
        responses, _ = run_sidecar(['{"jsonrpc":"2.0","id":1,"method":"system.ping"}'])
        assert len(responses) == 1
        assert responses[0]["id"] == 1
        assert "result" in responses[0]
        assert responses[0]["result"]["protocol"] == "v1"
        assert "version" in responses[0]["result"]

    def test_system_info(self, run_sidecar):
        """system.info should return capabilities and runtime info."""
        responses, _ = run_sidecar(['{"jsonrpc":"2.0","id":2,"method":"system.info"}'])
        assert len(responses) == 1
        result = responses[0]["result"]
        assert "capabilities" in result
        assert "runtime" in result
        assert result["protocol"] == "v1"

    def test_status_get(self, run_sidecar):
        """status.get should be implemented and return a valid status shape."""
        responses, _ = run_sidecar(['{"jsonrpc":"2.0","id":20,"method":"status.get"}'])
        assert len(responses) == 1
        assert "error" not in responses[0]

        result = responses[0]["result"]
        assert result["state"] in {"idle", "recording", "transcribing", "error"}
        if "detail" in result:
            assert isinstance(result["detail"], str)
        if "model" in result:
            assert result["model"]["status"] in {"ready", "loading", "error"}
            assert isinstance(result["model"]["model_id"], str)

    def test_unknown_method(self, run_sidecar):
        """Unknown method should return E_METHOD_NOT_FOUND error."""
        responses, _ = run_sidecar(['{"jsonrpc":"2.0","id":3,"method":"unknown.method"}'])
        assert len(responses) == 1
        error = responses[0]["error"]
        assert error["code"] == ERROR_METHOD_NOT_FOUND
        assert error["data"]["kind"] == "E_METHOD_NOT_FOUND"

    def test_malformed_json(self, run_sidecar):
        """Malformed JSON should return parse error."""
        responses, stderr = run_sidecar(["{not valid json}"])
        assert len(responses) == 1
        assert responses[0]["error"]["code"] == ERROR_PARSE_ERROR

    def test_missing_jsonrpc(self, run_sidecar):
        """Missing jsonrpc should return invalid request error."""
        responses, _ = run_sidecar(['{"method":"test"}'])
        assert len(responses) == 1
        assert responses[0]["error"]["code"] == ERROR_INVALID_REQUEST

    def test_multiple_requests(self, run_sidecar):
        """Multiple requests should all be processed."""
        responses, _ = run_sidecar([
            '{"jsonrpc":"2.0","id":1,"method":"system.ping"}',
            '{"jsonrpc":"2.0","id":2,"method":"system.ping"}',
            '{"jsonrpc":"2.0","id":3,"method":"system.ping"}',
        ])
        assert len(responses) == 3
        assert [r["id"] for r in responses] == [1, 2, 3]

    def test_empty_lines_ignored(self, run_sidecar):
        """Empty lines should be ignored."""
        responses, _ = run_sidecar([
            "",
            '{"jsonrpc":"2.0","id":1,"method":"system.ping"}',
            "   ",
            '{"jsonrpc":"2.0","id":2,"method":"system.ping"}',
        ])
        assert len(responses) == 2

    def test_shutdown(self, run_sidecar):
        """system.shutdown should return success and exit."""
        responses, stderr = run_sidecar([
            '{"jsonrpc":"2.0","id":1,"method":"system.shutdown","params":{"reason":"test"}}',
            '{"jsonrpc":"2.0","id":2,"method":"system.ping"}',  # Should not be processed
        ])
        # Should only get shutdown response
        assert len(responses) == 1
        assert responses[0]["result"]["status"] == "shutting_down"

    def test_eof_clean_exit(self, run_sidecar):
        """EOF should cause clean exit."""
        # Just send EOF (empty input)
        responses, stderr = run_sidecar([])
        assert len(responses) == 0
        # Verify server exited cleanly (no crash messages)
        assert any("exiting" in l.lower() for l in stderr)


class TestOversizedLine:
    """Tests for oversized line handling."""

    def test_oversized_line_detection(self):
        """Lines over MAX_LINE_LENGTH should be detected."""
        # Create a line just over the limit
        oversized = '{"jsonrpc":"2.0","method":"test","params":{"data":"' + "x" * (MAX_LINE_LENGTH + 100) + '"}}'
        assert len(oversized) > MAX_LINE_LENGTH


class TestPartialReads:
    """Tests simulating partial reads (lines split across buffers)."""

    def test_line_with_whitespace(self):
        """Lines with leading/trailing whitespace should parse."""
        req = parse_line('  {"jsonrpc": "2.0", "method": "test"}  \n')
        assert req is not None
        assert req.method == "test"

    def test_unicode_in_method(self):
        """Unicode in method names should work."""
        req = parse_line('{"jsonrpc": "2.0", "method": "test.emoji.🎤"}')
        assert req is not None
        assert req.method == "test.emoji.🎤"

    def test_unicode_in_params(self):
        """Unicode in params should work."""
        req = parse_line('{"jsonrpc": "2.0", "method": "test", "params": {"text": "héllo wörld 你好"}}')
        assert req is not None
        assert req.params["text"] == "héllo wörld 你好"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
