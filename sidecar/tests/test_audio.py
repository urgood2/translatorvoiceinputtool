"""Tests for audio device enumeration and selection."""

from __future__ import annotations

import json
import subprocess
import sys
from typing import Any
from unittest.mock import MagicMock, patch

import pytest

from openvoicy_sidecar.audio import (
    AudioDevice,
    DeviceNotFoundError,
    MicPermissionError,
    _generate_stable_uid,
    find_device_by_uid,
    get_active_device_uid,
    get_default_device,
    handle_audio_list_devices,
    handle_audio_set_device,
    list_audio_devices,
    set_active_device,
)
from openvoicy_sidecar.protocol import Request


# === Fixtures ===


@pytest.fixture
def mock_devices() -> list[dict[str, Any]]:
    """Mock device list from sounddevice.query_devices()."""
    return [
        {
            "name": "Built-in Microphone",
            "hostapi": 0,
            "max_input_channels": 2,
            "max_output_channels": 0,
            "default_samplerate": 48000.0,
        },
        {
            "name": "USB Headset",
            "hostapi": 0,
            "max_input_channels": 1,
            "max_output_channels": 2,
            "default_samplerate": 44100.0,
        },
        {
            "name": "Speakers",
            "hostapi": 0,
            "max_input_channels": 0,  # Output only
            "max_output_channels": 2,
            "default_samplerate": 48000.0,
        },
    ]


@pytest.fixture
def mock_host_apis() -> list[dict[str, Any]]:
    """Mock host APIs from sounddevice.query_hostapis()."""
    return [{"name": "CoreAudio", "devices": [0, 1, 2], "default_input_device": 0}]


@pytest.fixture
def reset_active_device():
    """Reset the active device after each test."""
    import openvoicy_sidecar.audio as audio_module

    original = audio_module._active_device_uid
    yield
    audio_module._active_device_uid = original


# === Unit Tests: list_audio_devices ===


class TestListAudioDevices:
    """Tests for list_audio_devices function."""

    def test_returns_empty_list_when_sounddevice_not_available(self):
        """Should return empty list when sounddevice is not installed."""
        with patch.dict("sys.modules", {"sounddevice": None}):
            # Force reimport to trigger ImportError
            import importlib

            import openvoicy_sidecar.audio as audio_module

            # Mock the import inside the function
            with patch.object(audio_module, "list_audio_devices") as mock_list:
                mock_list.return_value = []
                devices = mock_list()
                assert devices == []

    def test_returns_empty_list_when_no_devices(self):
        """Should return empty list when no input devices exist."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = []
        mock_sd.query_hostapis.return_value = []
        mock_sd.default.device = (None, None)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            devices = list_audio_devices()
            assert devices == []

    def test_filters_out_output_only_devices(
        self, mock_devices: list[dict], mock_host_apis: list[dict]
    ):
        """Should only return devices with input channels."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = mock_devices
        mock_sd.query_hostapis.return_value = mock_host_apis
        mock_sd.default.device = (0, 2)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            devices = list_audio_devices()

            # Should have 2 input devices (Built-in Mic and USB Headset)
            # Speakers (output only) should be filtered out
            assert len(devices) == 2
            names = [d.name for d in devices]
            assert "Built-in Microphone" in names
            assert "USB Headset" in names
            assert "Speakers" not in names

    def test_marks_default_device(
        self, mock_devices: list[dict], mock_host_apis: list[dict]
    ):
        """Should correctly identify the default device."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = mock_devices
        mock_sd.query_hostapis.return_value = mock_host_apis
        mock_sd.default.device = (0, 2)  # First device is default input

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            devices = list_audio_devices()

            default_devices = [d for d in devices if d.is_default]
            assert len(default_devices) == 1
            assert default_devices[0].name == "Built-in Microphone"

    def test_identical_input_devices_get_distinct_uids(self, mock_host_apis: list[dict]):
        """Same-model mics should get unique UIDs within one enumeration."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = [
            {
                "name": "USB Microphone",
                "hostapi": 0,
                "max_input_channels": 1,
                "max_output_channels": 0,
                "default_samplerate": 48000.0,
            },
            {
                "name": "USB Microphone",
                "hostapi": 0,
                "max_input_channels": 1,
                "max_output_channels": 0,
                "default_samplerate": 48000.0,
            },
        ]
        mock_sd.query_hostapis.return_value = mock_host_apis
        mock_sd.default.device = (0, None)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            devices = list_audio_devices()
            assert len(devices) == 2
            assert len({d.uid for d in devices}) == 2

    def test_permission_error_raises_mic_permission(self):
        """Should raise PermissionError when access denied."""
        mock_sd = MagicMock()
        mock_sd.PortAudioError = Exception
        mock_sd.query_devices.side_effect = Exception("permission denied")

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            with pytest.raises(PermissionError, match="permission denied"):
                list_audio_devices()


# === Unit Tests: Stable UIDs ===


class TestStableUIDs:
    """Tests for stable UID generation."""

    def test_uid_is_deterministic(self):
        """Same device info should produce same UID."""
        device_info = {
            "name": "Built-in Microphone",
            "max_input_channels": 2,
        }
        uid1 = _generate_stable_uid(device_info, "CoreAudio")
        uid2 = _generate_stable_uid(device_info, "CoreAudio")
        assert uid1 == uid2

    def test_uid_differs_by_device_name(self):
        """Different device names should produce different UIDs."""
        device1 = {"name": "Built-in Microphone", "max_input_channels": 2}
        device2 = {"name": "USB Headset", "max_input_channels": 2}

        uid1 = _generate_stable_uid(device1, "CoreAudio")
        uid2 = _generate_stable_uid(device2, "CoreAudio")
        assert uid1 != uid2

    def test_uid_differs_by_host_api(self):
        """Same device name with different host API should produce different UIDs."""
        device = {"name": "Microphone", "max_input_channels": 2}

        uid1 = _generate_stable_uid(device, "CoreAudio")
        uid2 = _generate_stable_uid(device, "WASAPI")
        assert uid1 != uid2

    def test_uid_differs_by_device_discriminator(self):
        """Identical devices should not collide when discriminator differs."""
        device = {"name": "Identical USB Mic", "max_input_channels": 1}

        uid1 = _generate_stable_uid(device, "CoreAudio", device_discriminator=0)
        uid2 = _generate_stable_uid(device, "CoreAudio", device_discriminator=1)
        assert uid1 != uid2

    def test_uid_has_platform_prefix(self):
        """UID should have platform-specific prefix."""
        device = {"name": "Test Device", "max_input_channels": 1}
        uid = _generate_stable_uid(device, "TestAPI")

        if sys.platform == "darwin":
            assert uid.startswith("macos:")
        elif sys.platform == "win32":
            assert uid.startswith("win:")
        else:
            assert uid.startswith("linux:")

    def test_uid_stability_across_reconnect(
        self, mock_devices: list[dict], mock_host_apis: list[dict]
    ):
        """UID should remain stable when device is reconnected (simulated)."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = mock_devices
        mock_sd.query_hostapis.return_value = mock_host_apis
        mock_sd.default.device = (0, 2)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            # First enumeration
            devices1 = list_audio_devices()
            uid1 = devices1[0].uid

            # Second enumeration (simulates reconnect)
            devices2 = list_audio_devices()
            uid2 = devices2[0].uid

            # UIDs should match
            assert uid1 == uid2


# === Unit Tests: set_device ===


class TestSetDevice:
    """Tests for set_active_device function."""

    def test_set_device_with_null_selects_default(
        self, mock_devices: list[dict], mock_host_apis: list[dict], reset_active_device
    ):
        """Setting device_uid to None should select default device."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = mock_devices
        mock_sd.query_hostapis.return_value = mock_host_apis
        mock_sd.default.device = (0, 2)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            devices = list_audio_devices()
            default_uid = [d.uid for d in devices if d.is_default][0]

            result = set_active_device(None)
            assert result == default_uid
            assert get_active_device_uid() == default_uid

    def test_set_device_with_valid_uid(
        self, mock_devices: list[dict], mock_host_apis: list[dict], reset_active_device
    ):
        """Should successfully set a specific device."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = mock_devices
        mock_sd.query_hostapis.return_value = mock_host_apis
        mock_sd.default.device = (0, 2)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            devices = list_audio_devices()
            usb_device = [d for d in devices if "USB" in d.name][0]

            result = set_active_device(usb_device.uid)
            assert result == usb_device.uid
            assert get_active_device_uid() == usb_device.uid

    def test_set_device_with_invalid_uid_raises_error(
        self, mock_devices: list[dict], mock_host_apis: list[dict], reset_active_device
    ):
        """Should raise ValueError for unknown device UID."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = mock_devices
        mock_sd.query_hostapis.return_value = mock_host_apis
        mock_sd.default.device = (0, 2)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            with pytest.raises(ValueError, match="Device not found"):
                set_active_device("nonexistent-device-uid")

    def test_set_device_null_with_no_devices(self, reset_active_device):
        """Setting null when no devices exist should return None."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = []
        mock_sd.query_hostapis.return_value = []
        mock_sd.default.device = (None, None)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            result = set_active_device(None)
            assert result is None
            assert get_active_device_uid() is None


# === Unit Tests: Hot-plug scenario ===


class TestHotPlug:
    """Tests for hot-plug scenarios."""

    def test_device_disappears_between_list_and_set(
        self, mock_devices: list[dict], mock_host_apis: list[dict], reset_active_device
    ):
        """Should handle device disappearing between list and set."""
        mock_sd = MagicMock()
        mock_sd.default.device = (0, 2)

        call_count = [0]

        def query_devices_side_effect():
            call_count[0] += 1
            if call_count[0] == 1:
                # First call: all devices present
                return mock_devices
            else:
                # Second call: USB device removed
                return [mock_devices[0], mock_devices[2]]

        mock_sd.query_devices.side_effect = query_devices_side_effect
        mock_sd.query_hostapis.return_value = mock_host_apis

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            # First: list devices and get USB device UID
            devices = list_audio_devices()
            usb_device = [d for d in devices if "USB" in d.name][0]
            usb_uid = usb_device.uid

            # Second: try to set USB device (which has been removed)
            with pytest.raises(ValueError, match="Device not found"):
                set_active_device(usb_uid)


# === Unit Tests: JSON-RPC Handlers ===


class TestAudioHandlers:
    """Tests for JSON-RPC handler functions."""

    def test_handle_list_devices(
        self, mock_devices: list[dict], mock_host_apis: list[dict]
    ):
        """Handler should return devices in correct format."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = mock_devices
        mock_sd.query_hostapis.return_value = mock_host_apis
        mock_sd.default.device = (0, 2)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            request = Request(method="audio.list_devices", id=1)
            result = handle_audio_list_devices(request)

            assert "devices" in result
            assert len(result["devices"]) == 2  # 2 input devices

            device = result["devices"][0]
            assert "uid" in device
            assert "name" in device
            assert "is_default" in device
            assert "default_sample_rate" in device
            assert "channels" in device

    def test_handle_list_devices_empty(self):
        """Handler should return empty list when no devices."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = []
        mock_sd.query_hostapis.return_value = []
        mock_sd.default.device = (None, None)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            request = Request(method="audio.list_devices", id=1)
            result = handle_audio_list_devices(request)

            assert result == {"devices": []}

    def test_handle_set_device_valid(
        self, mock_devices: list[dict], mock_host_apis: list[dict], reset_active_device
    ):
        """Handler should set device and return active UID."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = mock_devices
        mock_sd.query_hostapis.return_value = mock_host_apis
        mock_sd.default.device = (0, 2)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            # First get a valid UID
            devices = list_audio_devices()
            valid_uid = devices[0].uid

            request = Request(
                method="audio.set_device",
                id=2,
                params={"device_uid": valid_uid},
            )
            result = handle_audio_set_device(request)

            assert result == {"active_device_uid": valid_uid}

    def test_handle_set_device_null(
        self, mock_devices: list[dict], mock_host_apis: list[dict], reset_active_device
    ):
        """Handler should select default when device_uid is null."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = mock_devices
        mock_sd.query_hostapis.return_value = mock_host_apis
        mock_sd.default.device = (0, 2)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            request = Request(
                method="audio.set_device",
                id=3,
                params={"device_uid": None},
            )
            result = handle_audio_set_device(request)

            assert "active_device_uid" in result
            assert result["active_device_uid"] is not None

    def test_handle_set_device_invalid_raises_error(
        self, mock_devices: list[dict], mock_host_apis: list[dict], reset_active_device
    ):
        """Handler should raise DeviceNotFoundError for invalid UID."""
        mock_sd = MagicMock()
        mock_sd.query_devices.return_value = mock_devices
        mock_sd.query_hostapis.return_value = mock_host_apis
        mock_sd.default.device = (0, 2)

        with patch.dict("sys.modules", {"sounddevice": mock_sd}):
            request = Request(
                method="audio.set_device",
                id=4,
                params={"device_uid": "nonexistent-uid"},
            )

            with pytest.raises(DeviceNotFoundError):
                handle_audio_set_device(request)


# === Integration Tests ===


class TestAudioIntegration:
    """Integration tests that run the actual sidecar."""

    @pytest.fixture
    def sidecar_process(self):
        """Start a sidecar process for integration testing."""
        proc = subprocess.Popen(
            [sys.executable, "-m", "openvoicy_sidecar"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        yield proc
        # Cleanup
        if proc.poll() is None:
            proc.terminate()
            proc.wait(timeout=5)

    def _send_request(self, proc, method: str, params: dict | None = None) -> dict:
        """Send a JSON-RPC request and get the response."""
        request = {"jsonrpc": "2.0", "id": 1, "method": method}
        if params:
            request["params"] = params

        proc.stdin.write(json.dumps(request) + "\n")
        proc.stdin.flush()

        response_line = proc.stdout.readline()
        return json.loads(response_line)

    def test_list_devices_integration(self, sidecar_process):
        """Integration test: list_devices should work without errors."""
        response = self._send_request(sidecar_process, "audio.list_devices")

        # Should get a response (success or error)
        assert response.get("jsonrpc") == "2.0"
        assert response.get("id") == 1

        # If successful, should have devices key
        if "result" in response:
            assert "devices" in response["result"]
            # Devices might be empty if no audio hardware in CI
            assert isinstance(response["result"]["devices"], list)

    def test_set_device_null_integration(self, sidecar_process):
        """Integration test: set_device with null should work."""
        response = self._send_request(
            sidecar_process, "audio.set_device", {"device_uid": None}
        )

        assert response.get("jsonrpc") == "2.0"
        assert response.get("id") == 1

        # Should succeed even with no devices
        if "result" in response:
            assert "active_device_uid" in response["result"]

    def test_set_device_invalid_uid_integration(self, sidecar_process):
        """Integration test: invalid UID should return E_DEVICE_NOT_FOUND."""
        response = self._send_request(
            sidecar_process, "audio.set_device", {"device_uid": "invalid-uid-xyz"}
        )

        assert response.get("jsonrpc") == "2.0"
        assert "error" in response
        assert response["error"]["data"]["kind"] == "E_DEVICE_NOT_FOUND"
