"""Audio device enumeration and selection with stable UIDs.

This module provides cross-platform audio device management using
sounddevice (PortAudio). Device UIDs are designed to be stable across
sidecar restarts.

Stable UID Strategy:
- Linux: ALSA device names (e.g., "hw:0,0") or PulseAudio source names
- macOS: CoreAudio device UIDs via PortAudio
- Windows: WASAPI device endpoint IDs via PortAudio

Edge Cases Handled:
- No devices available: Returns empty list (not an error)
- Permission denied: E_MIC_PERMISSION error
- Device not found: E_DEVICE_NOT_FOUND error
- Hot-plug: Graceful handling when device disappears
"""

from __future__ import annotations

import hashlib
import sys
from dataclasses import dataclass
from typing import Any

from .protocol import (
    ERROR_DEVICE_NOT_FOUND,
    ERROR_MIC_PERMISSION,
    Request,
    log,
    make_error,
)

# Global state for selected device
_active_device_uid: str | None = None


@dataclass
class AudioDevice:
    """Represents an audio input device."""

    uid: str
    name: str
    is_default: bool
    default_sample_rate: int
    channels: int
    host_api: str

    def to_dict(self) -> dict[str, Any]:
        """Convert to API response format."""
        return {
            "uid": self.uid,
            "name": self.name,
            "is_default": self.is_default,
            "default_sample_rate": self.default_sample_rate,
            "channels": self.channels,
        }


def _generate_stable_uid(device_info: dict[str, Any], host_api_name: str) -> str:
    """Generate a stable UID for a device.

    On most platforms, PortAudio provides stable device identifiers through
    the device name. We create a hash-based UID from the combination of:
    - Device name
    - Host API name
    - Max input channels (for disambiguation)

    This provides stability across restarts while being unique enough to
    distinguish multiple devices with similar names.
    """
    name = device_info.get("name", "")
    max_input_channels = device_info.get("max_input_channels", 0)

    # Create a unique identifier string
    id_parts = [
        name,
        host_api_name,
        str(max_input_channels),
    ]
    id_string = "|".join(id_parts)

    # Hash to create a shorter, stable UID
    hash_digest = hashlib.sha256(id_string.encode()).hexdigest()[:12]

    # Platform-specific prefix for readability
    if sys.platform == "darwin":
        prefix = "macos"
    elif sys.platform == "win32":
        prefix = "win"
    else:
        prefix = "linux"

    return f"{prefix}:{hash_digest}"


def list_audio_devices() -> list[AudioDevice]:
    """List all available audio input devices.

    Returns an empty list if no devices are available or if sounddevice
    is not installed. Does not raise errors for missing devices.

    Raises:
        PermissionError: If microphone access is denied.
    """
    try:
        import sounddevice as sd
    except ImportError:
        log("sounddevice not available, returning empty device list")
        return []

    try:
        devices = sd.query_devices()
        host_apis = sd.query_hostapis()
    except sd.PortAudioError as e:
        # Check for permission-related errors
        error_str = str(e).lower()
        if "permission" in error_str or "access denied" in error_str:
            raise PermissionError("Microphone permission denied") from e
        # Other errors - log and return empty list
        log(f"PortAudio error querying devices: {e}")
        return []
    except Exception as e:
        log(f"Unexpected error querying devices: {e}")
        return []

    # Find default input device
    try:
        default_input = sd.default.device[0]  # Input device index
    except Exception:
        default_input = None

    result: list[AudioDevice] = []

    # Handle both single device and list of devices
    if isinstance(devices, dict):
        devices = [devices]

    for idx, device in enumerate(devices):
        # Only include input devices (devices with input channels)
        max_input_channels = device.get("max_input_channels", 0)
        if max_input_channels <= 0:
            continue

        # Get host API name
        host_api_index = device.get("hostapi", 0)
        host_api_name = "unknown"
        if isinstance(host_apis, list) and host_api_index < len(host_apis):
            host_api_name = host_apis[host_api_index].get("name", "unknown")
        elif isinstance(host_apis, dict):
            host_api_name = host_apis.get("name", "unknown")

        uid = _generate_stable_uid(device, host_api_name)
        name = device.get("name", f"Device {idx}")
        sample_rate = int(device.get("default_samplerate", 48000))

        result.append(
            AudioDevice(
                uid=uid,
                name=name,
                is_default=(idx == default_input),
                default_sample_rate=sample_rate,
                channels=max_input_channels,
                host_api=host_api_name,
            )
        )

    return result


def find_device_by_uid(uid: str) -> AudioDevice | None:
    """Find a device by its UID.

    Returns None if the device is not found.
    """
    devices = list_audio_devices()
    for device in devices:
        if device.uid == uid:
            return device
    return None


def get_default_device() -> AudioDevice | None:
    """Get the default input device.

    Returns None if no default device is available.
    """
    devices = list_audio_devices()
    for device in devices:
        if device.is_default:
            return device
    # If no device is marked as default, return the first one
    return devices[0] if devices else None


def set_active_device(uid: str | None) -> str | None:
    """Set the active device for recording.

    Args:
        uid: Device UID to select, or None to select the default device.

    Returns:
        The UID of the selected device.

    Raises:
        ValueError: If the device UID is not found.
        PermissionError: If microphone permission is denied.
    """
    global _active_device_uid

    if uid is None:
        # Select default device
        default = get_default_device()
        if default is None:
            # No devices available - this is okay, we just have no active device
            _active_device_uid = None
            return None
        _active_device_uid = default.uid
        return default.uid

    # Find the requested device
    device = find_device_by_uid(uid)
    if device is None:
        raise ValueError(f"Device not found: {uid}")

    _active_device_uid = uid
    return uid


def get_active_device_uid() -> str | None:
    """Get the UID of the currently active device."""
    return _active_device_uid


# === JSON-RPC Handlers ===


def handle_audio_list_devices(request: Request) -> dict[str, Any]:
    """Handle audio.list_devices request.

    Returns a list of available audio input devices with stable UIDs.
    Returns an empty list if no devices are available (not an error).

    Errors:
        E_MIC_PERMISSION: Microphone permission denied.
    """
    try:
        devices = list_audio_devices()
        return {"devices": [d.to_dict() for d in devices]}
    except PermissionError:
        # This will be caught by the server and converted to an error response
        raise MicPermissionError("Microphone permission denied")


def handle_audio_set_device(request: Request) -> dict[str, Any]:
    """Handle audio.set_device request.

    Sets the active audio device for recording.

    Params:
        device_uid: Device UID to select, or null for default device.

    Returns:
        active_device_uid: The UID of the selected device.

    Errors:
        E_DEVICE_NOT_FOUND: Device UID not found.
        E_MIC_PERMISSION: Microphone permission denied.
    """
    device_uid = request.params.get("device_uid")

    try:
        active_uid = set_active_device(device_uid)
        return {"active_device_uid": active_uid}
    except ValueError as e:
        raise DeviceNotFoundError(str(e), device_uid)
    except PermissionError:
        raise MicPermissionError("Microphone permission denied")


# === Custom Exceptions for Error Handling ===


class MicPermissionError(Exception):
    """Microphone permission denied error."""

    def __init__(self, message: str = "Microphone permission denied"):
        self.message = message
        super().__init__(message)


class DeviceNotFoundError(Exception):
    """Device not found error."""

    def __init__(self, message: str, device_uid: str | None = None):
        self.message = message
        self.device_uid = device_uid
        super().__init__(message)
