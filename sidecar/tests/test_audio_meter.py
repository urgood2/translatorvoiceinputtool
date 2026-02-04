"""Tests for audio meter functionality."""

from __future__ import annotations

import sys
import time
from unittest.mock import MagicMock, patch

import numpy as np
import pytest

# Mock sounddevice before importing the module
mock_sd = MagicMock()
sys.modules["sounddevice"] = mock_sd

from openvoicy_sidecar.audio_meter import (
    AudioMeter,
    DEFAULT_INTERVAL_MS,
    MAX_INTERVAL_MS,
    MIN_INTERVAL_MS,
    MeterAlreadyRunningError,
    MeterError,
    _clamp_interval,
    get_meter,
    handle_audio_meter_start,
    handle_audio_meter_status,
    handle_audio_meter_stop,
)
from openvoicy_sidecar.protocol import Request


# === Fixtures ===


@pytest.fixture(autouse=True)
def reset_meter():
    """Reset the global meter instance and mock before each test."""
    import openvoicy_sidecar.audio_meter as meter_module

    if meter_module._meter is not None and meter_module._meter.is_running:
        meter_module._meter.stop()
    meter_module._meter = None

    # Reset mock
    mock_sd.reset_mock()
    mock_stream = MagicMock()
    mock_sd.InputStream.return_value = mock_stream

    yield

    if meter_module._meter is not None and meter_module._meter.is_running:
        meter_module._meter.stop()
    meter_module._meter = None


# === Unit Tests: Interval Clamping ===


class TestIntervalClamping:
    """Tests for interval clamping."""

    def test_clamp_below_minimum(self):
        """Should clamp to minimum."""
        assert _clamp_interval(10) == MIN_INTERVAL_MS
        assert _clamp_interval(0) == MIN_INTERVAL_MS
        assert _clamp_interval(-100) == MIN_INTERVAL_MS

    def test_clamp_above_maximum(self):
        """Should clamp to maximum."""
        assert _clamp_interval(500) == MAX_INTERVAL_MS
        assert _clamp_interval(1000) == MAX_INTERVAL_MS

    def test_clamp_within_range(self):
        """Should not change values in range."""
        assert _clamp_interval(80) == 80
        assert _clamp_interval(30) == 30
        assert _clamp_interval(250) == 250


# === Unit Tests: AudioMeter ===


class TestAudioMeter:
    """Tests for AudioMeter class."""

    def test_initial_state(self):
        """Should start not running."""
        meter = AudioMeter()
        assert not meter.is_running

    def test_start_creates_stream(self):
        """Should create audio stream on start."""
        meter = AudioMeter()

        meter.start()

        mock_sd.InputStream.assert_called_once()
        mock_sd.InputStream.return_value.start.assert_called_once()
        assert meter.is_running

        meter.stop()

    def test_start_twice_raises(self):
        """Should raise if started twice."""
        meter = AudioMeter()
        meter.start()

        with pytest.raises(RuntimeError, match="already running"):
            meter.start()

        meter.stop()

    def test_stop_idempotent(self):
        """Should not raise if already stopped."""
        meter = AudioMeter()
        # Stopping when not running should be safe
        meter.stop()  # No exception
        meter.stop()  # No exception

    def test_interval_clamping_on_start(self):
        """Should clamp interval on start."""
        meter = AudioMeter()

        meter.start(interval_ms=10)  # Below minimum

        assert meter._interval_ms == MIN_INTERVAL_MS
        meter.stop()

    def test_default_interval(self):
        """Should use default interval."""
        meter = AudioMeter()

        meter.start()

        assert meter._interval_ms == DEFAULT_INTERVAL_MS
        meter.stop()


# === Unit Tests: Handlers ===


class TestHandlers:
    """Tests for JSON-RPC handlers."""

    def test_meter_start_handler(self):
        """Should start meter via handler."""
        request = Request(method="audio.meter_start", id=1, params={})

        result = handle_audio_meter_start(request)

        assert result["running"] is True
        assert result["interval_ms"] == DEFAULT_INTERVAL_MS

        # Clean up
        get_meter().stop()

    def test_meter_start_with_interval(self):
        """Should start meter with custom interval."""
        request = Request(
            method="audio.meter_start",
            id=1,
            params={"interval_ms": 100},
        )

        result = handle_audio_meter_start(request)

        assert result["interval_ms"] == 100

        get_meter().stop()

    def test_meter_start_interval_clamped(self):
        """Should clamp interval in handler."""
        request = Request(
            method="audio.meter_start",
            id=1,
            params={"interval_ms": 5},  # Below minimum
        )

        result = handle_audio_meter_start(request)

        assert result["interval_ms"] == MIN_INTERVAL_MS

        get_meter().stop()

    def test_meter_stop_handler(self):
        """Should stop meter via handler."""
        # Start the meter first
        meter = get_meter()
        meter.start()

        request = Request(method="audio.meter_stop", id=1, params={})
        result = handle_audio_meter_stop(request)

        assert result["stopped"] is True
        assert not meter.is_running

    def test_meter_stop_idempotent(self):
        """Should be idempotent."""
        request = Request(method="audio.meter_stop", id=1, params={})

        # Stopping when not running should be fine
        result = handle_audio_meter_stop(request)
        assert result["stopped"] is True

    def test_meter_status_not_running(self):
        """Should report not running."""
        request = Request(method="audio.meter_status", id=1, params={})

        result = handle_audio_meter_status(request)

        assert result["running"] is False
        assert "interval_ms" not in result

    def test_meter_status_running(self):
        """Should report running with interval."""
        meter = get_meter()
        meter.start(interval_ms=100)

        request = Request(method="audio.meter_status", id=1, params={})
        result = handle_audio_meter_status(request)

        assert result["running"] is True
        assert result["interval_ms"] == 100

        meter.stop()


# === Integration Tests ===


class TestMeterIntegration:
    """Integration tests for audio meter."""

    def test_level_emission(self):
        """Should emit audio levels at specified cadence."""
        meter = AudioMeter()
        emitted_levels = []

        # Mock emission function
        with patch("openvoicy_sidecar.audio_meter.emit_audio_level") as mock_emit:
            def capture_emit(rms, peak, source):
                emitted_levels.append({"rms": rms, "peak": peak, "source": source})

            mock_emit.side_effect = capture_emit

            meter.start(interval_ms=50)

            # Manually populate buffer to simulate audio callback
            test_audio = np.random.randn(1600).astype(np.float32) * 0.1
            meter._buffer.extend(test_audio)

            # Wait for some emissions
            time.sleep(0.2)

            meter.stop()

        # Should have emitted several levels
        assert len(emitted_levels) >= 2
        for level in emitted_levels:
            assert level["source"] == "meter"
            assert 0 <= level["rms"] <= 1
            assert 0 <= level["peak"] <= 1

    def test_buffer_populated_by_callback(self):
        """Should populate buffer via audio callback."""
        meter = AudioMeter()
        meter.start()

        # Simulate audio callback
        test_audio = np.random.randn(256).astype(np.float32) * 0.1
        meter._audio_callback(
            test_audio.reshape(-1, 1),
            256,
            None,
            None,
        )

        # Buffer should have data
        assert len(meter._buffer) == 256

        meter.stop()
