"""Audio meter for real-time level monitoring.

This module provides microphone level monitoring for:
- Mic testing in settings (source="meter")
- Visual feedback during recording (source="recording")

The meter is low-CPU and doesn't require ASR model load.
"""

from __future__ import annotations

import threading
import time
from collections import deque
from typing import Any, Optional

import numpy as np

from .audio import find_device_by_uid, get_default_device
from .notifications import calculate_audio_levels, emit_audio_level
from .protocol import Request, log

# === Constants ===

DEFAULT_INTERVAL_MS = 80  # Default emission interval
MIN_INTERVAL_MS = 30  # Minimum allowed interval
MAX_INTERVAL_MS = 250  # Maximum allowed interval
BUFFER_DURATION_MS = 100  # Duration of audio to analyze
METER_SAMPLE_RATE = 16000  # Sample rate for meter
METER_CHUNK_SIZE = 256  # Small chunks for low latency


def _clamp_interval(interval_ms: int) -> int:
    """Clamp interval to valid range."""
    return max(MIN_INTERVAL_MS, min(MAX_INTERVAL_MS, interval_ms))


class AudioMeter:
    """Real-time audio level meter.

    Provides non-blocking audio level monitoring that emits
    event.audio_level notifications at a configurable cadence.
    """

    def __init__(self):
        self._running = False
        self._stream: Any = None
        self._buffer: deque = deque(maxlen=int(BUFFER_DURATION_MS * METER_SAMPLE_RATE / 1000))
        self._lock = threading.Lock()
        self._thread: Optional[threading.Thread] = None
        self._interval_ms = DEFAULT_INTERVAL_MS

    @property
    def is_running(self) -> bool:
        """Check if meter is running."""
        return self._running

    def start(
        self,
        device_uid: Optional[str] = None,
        interval_ms: int = DEFAULT_INTERVAL_MS,
    ) -> None:
        """Start the audio meter.

        Args:
            device_uid: Device to monitor, or None for default
            interval_ms: Emission interval in ms (clamped to 30-250ms)

        Raises:
            RuntimeError: If meter already running
            ValueError: If device not found
            OSError: If failed to open audio device
        """
        with self._lock:
            if self._running:
                raise RuntimeError("Audio meter already running")

            self._interval_ms = _clamp_interval(interval_ms)
            self._buffer.clear()

            # Resolve device
            device_index = None
            if device_uid is not None:
                device = find_device_by_uid(device_uid)
                if device is None:
                    raise ValueError(f"Device not found: {device_uid}")
                device_index = self._get_device_index(device_uid)

            # Open audio stream
            try:
                import sounddevice as sd

                self._stream = sd.InputStream(
                    samplerate=METER_SAMPLE_RATE,
                    channels=1,
                    dtype=np.float32,
                    blocksize=METER_CHUNK_SIZE,
                    device=device_index,
                    callback=self._audio_callback,
                )
                self._stream.start()
                self._running = True

                log(f"Audio meter started: device={device_uid or 'default'}, interval={self._interval_ms}ms")

            except Exception as e:
                log(f"Failed to start audio meter: {e}")
                raise OSError(f"Failed to open audio device: {e}") from e

        # Start emission thread
        self._thread = threading.Thread(target=self._emit_loop, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        """Stop the audio meter."""
        with self._lock:
            if not self._running:
                return

            self._running = False

        # Close stream
        if self._stream is not None:
            self._stream.stop()
            self._stream.close()
            self._stream = None

        # Wait for thread
        if self._thread is not None:
            self._thread.join(timeout=1.0)
            self._thread = None

        log("Audio meter stopped")

    def _audio_callback(
        self,
        indata: np.ndarray,
        frames: int,
        time_info: Any,
        status: Any,
    ) -> None:
        """sounddevice callback - runs in PortAudio thread."""
        if status:
            log(f"Meter audio callback status: {status}")

        # Add samples to buffer
        with self._lock:
            if not self._running:
                return

            # Extract mono data
            mono = indata[:, 0] if indata.ndim > 1 else indata.flatten()
            self._buffer.extend(mono)

    def _emit_loop(self) -> None:
        """Background loop that emits level events."""
        while self._running:
            time.sleep(self._interval_ms / 1000)

            if not self._running:
                break

            # Get current levels
            with self._lock:
                if not self._buffer:
                    continue

                # Convert deque to array for analysis
                audio = np.array(list(self._buffer), dtype=np.float32)

            # Calculate and emit levels
            rms, peak = calculate_audio_levels(audio)
            emit_audio_level(rms=rms, peak=peak, source="meter")

    def _get_device_index(self, device_uid: str) -> Optional[int]:
        """Get sounddevice device index from our UID."""
        try:
            import sounddevice as sd
            from .audio import _generate_stable_uid

            devices = sd.query_devices()
            host_apis = sd.query_hostapis()

            if isinstance(devices, dict):
                devices = [devices]

            for idx, device in enumerate(devices):
                if device.get("max_input_channels", 0) <= 0:
                    continue

                host_api_index = device.get("hostapi", 0)
                host_api_name = "unknown"
                if isinstance(host_apis, list) and host_api_index < len(host_apis):
                    host_api_name = host_apis[host_api_index].get("name", "unknown")

                uid = _generate_stable_uid(device, host_api_name)
                if uid == device_uid:
                    return idx

            return None
        except Exception as e:
            log(f"Error getting device index for meter: {e}")
            return None


# === Global Meter Instance ===

_meter: Optional[AudioMeter] = None


def get_meter() -> AudioMeter:
    """Get the global AudioMeter instance."""
    global _meter
    if _meter is None:
        _meter = AudioMeter()
    return _meter


# === JSON-RPC Handlers ===


class MeterError(Exception):
    """Audio meter error."""

    def __init__(self, message: str, code: str = "E_METER"):
        self.message = message
        self.code = code
        super().__init__(message)


class MeterAlreadyRunningError(MeterError):
    """Raised when trying to start meter while already running."""

    def __init__(self, message: str = "Audio meter already running"):
        super().__init__(message, "E_METER_RUNNING")


class MeterNotRunningError(MeterError):
    """Raised when trying to stop meter while not running."""

    def __init__(self, message: str = "Audio meter not running"):
        super().__init__(message, "E_METER_NOT_RUNNING")


def handle_audio_meter_start(request: Request) -> dict[str, Any]:
    """Handle audio.meter_start request.

    Params:
        device_uid: Optional device UID to monitor
        interval_ms: Emission interval (30-250ms, default 80ms)

    Returns:
        running: True if meter started
        interval_ms: Actual interval being used (after clamping)

    Errors:
        E_METER_RUNNING: Meter already running
        E_DEVICE_NOT_FOUND: Device not found
        E_AUDIO_IO: Failed to open audio device
    """
    device_uid = request.params.get("device_uid")
    interval_ms = request.params.get("interval_ms", DEFAULT_INTERVAL_MS)

    meter = get_meter()

    try:
        meter.start(device_uid, interval_ms)
        return {
            "running": True,
            "interval_ms": meter._interval_ms,
        }
    except RuntimeError as e:
        raise MeterAlreadyRunningError(str(e))
    except ValueError as e:
        from .audio import DeviceNotFoundError

        raise DeviceNotFoundError(str(e), device_uid)
    except OSError as e:
        raise MeterError(str(e), "E_AUDIO_IO")


def handle_audio_meter_stop(request: Request) -> dict[str, Any]:
    """Handle audio.meter_stop request.

    Returns:
        stopped: True if meter was stopped
    """
    meter = get_meter()

    if not meter.is_running:
        # Idempotent - already stopped is fine
        return {"stopped": True}

    meter.stop()
    return {"stopped": True}


def handle_audio_meter_status(request: Request) -> dict[str, Any]:
    """Handle audio.meter_status request.

    Returns:
        running: Whether meter is running
        interval_ms: Current interval if running
    """
    meter = get_meter()

    result: dict[str, Any] = {"running": meter.is_running}
    if meter.is_running:
        result["interval_ms"] = meter._interval_ms

    return result
