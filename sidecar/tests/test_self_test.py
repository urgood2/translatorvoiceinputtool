"""Tests for sidecar self-test validators and command selection."""

from __future__ import annotations

import sys
from unittest.mock import MagicMock, patch

import pytest

from openvoicy_sidecar.self_test import (
    SelfTestError,
    SidecarRpcProcess,
    build_sidecar_command,
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

    def test_shutdown_accepts_signal_terminated_exit(self):
        """SIGTERM (-15) after explicit shutdown is acceptable, not a crash."""
        proc = SidecarRpcProcess(["true"], {})
        mock_proc = MagicMock()
        mock_proc.poll.return_value = None
        mock_proc.stdin = MagicMock()
        mock_proc.wait.return_value = -15
        mock_proc.returncode = -15
        proc._proc = mock_proc

        exit_code = proc.shutdown()
        assert exit_code == -15
        # Negative exit codes (signal termination) are acceptable
        assert exit_code <= 0

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
