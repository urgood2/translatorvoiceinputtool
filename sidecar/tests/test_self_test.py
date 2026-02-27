"""Tests for sidecar self-test validators and command selection."""

from __future__ import annotations

import sys
from unittest.mock import MagicMock, patch

import pytest

from openvoicy_sidecar.self_test import (
    DEFAULT_RPC_TIMEOUT_SECONDS,
    INITIAL_PING_BACKOFF_SECONDS,
    SelfTestError,
    SidecarRpcProcess,
    _call_initial_ping_with_retry,
    build_sidecar_command,
    main,
    rpc_timeout_seconds,
    validate_clean_exit_code,
    validate_replacements_get_rules_result,
    validate_status_get_result,
    validate_system_info_result,
)


class TestSystemInfoValidation:
    """Tests for system.info response validation."""

    def test_accepts_valid_shape(self):
        validate_system_info_result(
            {
                "capabilities": ["asr", "replacements", "meter"],
                "runtime": {
                    "python_version": "3.13.0",
                    "platform": "linux",
                    "cuda_available": False,
                },
            }
        )

    def test_rejects_non_array_capabilities(self):
        with pytest.raises(SelfTestError, match="capabilities"):
            validate_system_info_result(
                {
                    "capabilities": {"asr": True},
                    "runtime": {
                        "python_version": "3.13.0",
                        "platform": "linux",
                        "cuda_available": False,
                    },
                }
            )


class TestStatusGetValidation:
    """Tests for status.get response validation."""

    def test_accepts_valid_shape(self):
        validate_status_get_result(
            {
                "state": "idle",
                "model": {
                    "model_id": "parakeet-tdt-0.6b-v3",
                    "status": "ready",
                },
            }
        )

    def test_rejects_invalid_state(self):
        with pytest.raises(SelfTestError, match="state"):
            validate_status_get_result({"state": "unknown"})

    def test_accepts_protocol_model_statuses(self):
        for status in ("missing", "downloading", "verifying", "ready", "error"):
            validate_status_get_result(
                {
                    "state": "loading_model" if status in {"downloading", "verifying"} else "idle",
                    "model": {
                        "model_id": "parakeet-tdt-0.6b-v3",
                        "status": status,
                    },
                }
            )

    def test_rejects_legacy_loading_model_status(self):
        with pytest.raises(SelfTestError, match="model.status"):
            validate_status_get_result(
                {
                    "state": "loading_model",
                    "model": {
                        "model_id": "parakeet-tdt-0.6b-v3",
                        "status": "loading",
                    },
                }
            )


class TestReplacementsValidation:
    """Tests for replacements.get_rules response validation."""

    def test_rejects_non_array_rules(self):
        with pytest.raises(SelfTestError, match="rules"):
            validate_replacements_get_rules_result({"rules": {"id": "x"}})


class TestBuildCommand:
    """Tests for sidecar command selection."""

    def test_builds_dev_mode_command(self, monkeypatch):
        monkeypatch.delattr(sys, "frozen", raising=False)
        command, env = build_sidecar_command()

        assert command[:3] == [sys.executable, "-m", "openvoicy_sidecar"]
        assert "PYTHONPATH" in env
        assert "sidecar/src" in env["PYTHONPATH"]

    def test_builds_frozen_mode_command(self, monkeypatch):
        monkeypatch.setattr(sys, "frozen", True, raising=False)
        monkeypatch.setattr(sys, "executable", "/tmp/openvoicy-sidecar")

        command, _ = build_sidecar_command()
        assert command == ["/tmp/openvoicy-sidecar"]

    def test_honors_openvoicy_sidecar_command_override(self, monkeypatch):
        monkeypatch.delattr(sys, "frozen", raising=False)
        monkeypatch.setenv("OPENVOICY_SIDECAR_COMMAND", "/tmp/openvoicy-sidecar --mode smoke")

        command, _ = build_sidecar_command()
        assert command == ["/tmp/openvoicy-sidecar", "--mode", "smoke"]


class TestTimeoutConfiguration:
    def test_uses_default_when_env_not_set(self, monkeypatch):
        monkeypatch.delenv("OPENVOICY_SELF_TEST_TIMEOUT_S", raising=False)
        assert rpc_timeout_seconds() == DEFAULT_RPC_TIMEOUT_SECONDS

    def test_accepts_env_override(self, monkeypatch):
        monkeypatch.setenv("OPENVOICY_SELF_TEST_TIMEOUT_S", "22.5")
        assert rpc_timeout_seconds() == pytest.approx(22.5)

    def test_invalid_env_falls_back_to_default(self, monkeypatch):
        monkeypatch.setenv("OPENVOICY_SELF_TEST_TIMEOUT_S", "not-a-number")
        assert rpc_timeout_seconds() == DEFAULT_RPC_TIMEOUT_SECONDS

    def test_non_positive_env_falls_back_to_default(self, monkeypatch):
        monkeypatch.setenv("OPENVOICY_SELF_TEST_TIMEOUT_S", "0")
        assert rpc_timeout_seconds() == DEFAULT_RPC_TIMEOUT_SECONDS


class TestInitialPingRetry:
    def test_retries_timeout_then_succeeds(self):
        sidecar = MagicMock()
        sidecar.call.side_effect = [
            SelfTestError("Timed out waiting for response to system.ping"),
            {"version": "0.1.0", "protocol": "v1"},
        ]

        with patch("openvoicy_sidecar.self_test.time.sleep") as mock_sleep:
            result = _call_initial_ping_with_retry(sidecar)

        assert result["protocol"] == "v1"
        assert sidecar.call.call_count == 2
        sidecar.call.assert_any_call("system.ping")
        mock_sleep.assert_called_once_with(INITIAL_PING_BACKOFF_SECONDS)

    def test_fails_fast_when_sidecar_exits(self):
        sidecar = MagicMock()
        sidecar.call.side_effect = SelfTestError("Sidecar exited before request system.ping")

        with patch("openvoicy_sidecar.self_test.time.sleep") as mock_sleep:
            with pytest.raises(SelfTestError, match="exited before request system.ping"):
                _call_initial_ping_with_retry(sidecar)

        assert sidecar.call.call_count == 1
        mock_sleep.assert_not_called()

    def test_fails_fast_for_non_timeout_error(self):
        sidecar = MagicMock()
        sidecar.call.side_effect = SelfTestError("system.ping returned error: {'code': -32603}")

        with patch("openvoicy_sidecar.self_test.time.sleep") as mock_sleep:
            with pytest.raises(SelfTestError, match="returned error"):
                _call_initial_ping_with_retry(sidecar)

        assert sidecar.call.call_count == 1
        mock_sleep.assert_not_called()


class TestShutdownExitCode:
    """Regression (29fu): shutdown must return exit code for clean-exit verification."""

    def test_shutdown_returns_zero_on_clean_exit(self):
        proc = SidecarRpcProcess(["true"], {})
        mock_proc = MagicMock()
        mock_proc.poll.return_value = None
        mock_proc.stdin = MagicMock()
        mock_proc.wait.return_value = 0
        mock_proc.returncode = 0
        proc._proc = mock_proc

        exit_code = proc.shutdown()
        assert exit_code == 0

    def test_shutdown_returns_nonzero_on_crash(self):
        proc = SidecarRpcProcess(["false"], {})
        mock_proc = MagicMock()
        mock_proc.poll.return_value = None
        mock_proc.stdin = MagicMock()
        mock_proc.wait.return_value = 1
        mock_proc.returncode = 1
        proc._proc = mock_proc

        exit_code = proc.shutdown()
        assert exit_code == 1
        # Positive non-zero exit code indicates crash; self-test should fail
        assert exit_code > 0

    def test_shutdown_returns_signal_terminated_exit_code(self):
        proc = SidecarRpcProcess(["true"], {})
        mock_proc = MagicMock()
        mock_proc.poll.return_value = None
        mock_proc.stdin = MagicMock()
        mock_proc.wait.return_value = -15
        mock_proc.returncode = -15
        proc._proc = mock_proc

        exit_code = proc.shutdown()
        assert exit_code == -15

    def test_shutdown_returns_none_when_no_process(self):
        proc = SidecarRpcProcess(["true"], {})
        assert proc.shutdown() is None

    def test_shutdown_returns_existing_code_for_already_exited_process(self):
        proc = SidecarRpcProcess(["true"], {})
        mock_proc = MagicMock()
        mock_proc.poll.return_value = 42
        mock_proc.returncode = 42
        proc._proc = mock_proc

        exit_code = proc.shutdown()
        assert exit_code == 42


class TestCleanExitValidation:
    def test_accepts_zero_exit_code(self):
        validate_clean_exit_code(0)

    def test_rejects_positive_nonzero_exit_code(self):
        with pytest.raises(SelfTestError, match="did not exit cleanly"):
            validate_clean_exit_code(1)

    def test_rejects_signal_terminated_exit_code(self):
        with pytest.raises(SelfTestError, match="did not exit cleanly"):
            validate_clean_exit_code(-15)

    def test_rejects_missing_exit_code(self):
        with pytest.raises(SelfTestError, match="not running at shutdown"):
            validate_clean_exit_code(None)


class TestMainLogging:
    def test_main_logs_explicit_pass_marker_on_success(self):
        with (
            patch("openvoicy_sidecar.self_test.run_self_test"),
            patch("openvoicy_sidecar.self_test._log") as mock_log,
        ):
            exit_code = main()

        assert exit_code == 0
        mock_log.assert_any_call("PASS: All checks passed")
