/**
 * Tests for MicSetupStep onboarding component.
 *
 * Covers: device listing on mount, device selection, mic test start/stop,
 * audio level display, cleanup on unmount, error states.
 */

import { describe, test, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { MicSetupStep } from './MicSetupStep';
import { useAppStore } from '../../store/appStore';
import type { AudioDevice } from '../../types';

// ── Mock Tauri invoke ─────────────────────────────────────────────

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

// ── Fixtures ──────────────────────────────────────────────────────

const DEVICES: AudioDevice[] = [
  { uid: 'mic-1', name: 'Built-in Microphone', is_default: true, sample_rate: 48000, channels: 1 },
  { uid: 'mic-2', name: 'USB Headset', is_default: false, sample_rate: 44100, channels: 2 },
];

// ── Setup ─────────────────────────────────────────────────────────

const refreshDevicesSpy = vi.fn();
const selectDeviceSpy = vi.fn();
const startMicTestSpy = vi.fn().mockResolvedValue(undefined);
const stopMicTestSpy = vi.fn().mockResolvedValue(undefined);

beforeEach(() => {
  vi.clearAllMocks();
  useAppStore.setState({
    devices: DEVICES,
    selectedDeviceUid: 'mic-1',
    audioLevel: null,
    isMeterRunning: false,
    refreshDevices: refreshDevicesSpy,
    selectDevice: selectDeviceSpy,
    startMicTest: startMicTestSpy,
    stopMicTest: stopMicTestSpy,
  });
});

// ── Tests ─────────────────────────────────────────────────────────

describe('MicSetupStep', () => {
  test('renders heading and description', () => {
    render(<MicSetupStep onReady={vi.fn()} />);
    expect(screen.getByText('Microphone Setup')).toBeDefined();
    expect(screen.getByText(/Select your microphone/)).toBeDefined();
  });

  test('calls refreshDevices on mount', () => {
    render(<MicSetupStep onReady={vi.fn()} />);
    expect(refreshDevicesSpy).toHaveBeenCalled();
  });

  test('renders device selector with available devices', () => {
    render(<MicSetupStep onReady={vi.fn()} />);
    const select = screen.getByLabelText('Input Device') as HTMLSelectElement;
    expect(select).toBeDefined();
    expect(select.value).toBe('mic-1');

    // Both devices are listed
    expect(screen.getByText('Built-in Microphone (Default)')).toBeDefined();
    expect(screen.getByText('USB Headset')).toBeDefined();
  });

  test('shows device info for selected device', () => {
    render(<MicSetupStep onReady={vi.fn()} />);
    expect(screen.getByText('48kHz, 1 channel')).toBeDefined();
  });

  test('shows plural channels for stereo device', () => {
    useAppStore.setState({
      devices: DEVICES,
      selectedDeviceUid: 'mic-2',
    });
    render(<MicSetupStep onReady={vi.fn()} />);
    expect(screen.getByText('44.1kHz, 2 channels')).toBeDefined();
  });

  test('calls selectDevice when changing device', async () => {
    render(<MicSetupStep onReady={vi.fn()} />);
    const select = screen.getByLabelText('Input Device') as HTMLSelectElement;

    await act(async () => {
      fireEvent.change(select, { target: { value: 'mic-2' } });
    });

    expect(selectDeviceSpy).toHaveBeenCalledWith('mic-2');
  });

  test('shows "No devices found" when device list is empty', () => {
    useAppStore.setState({ devices: [], selectedDeviceUid: null });
    render(<MicSetupStep onReady={vi.fn()} />);
    expect(screen.getByText('No devices found')).toBeDefined();
  });

  test('renders mic test controls', () => {
    render(<MicSetupStep onReady={vi.fn()} />);
    expect(screen.getByText('Microphone Test')).toBeDefined();
    expect(screen.getByText('Start Test')).toBeDefined();
  });

  test('start test button calls startMicTest', async () => {
    render(<MicSetupStep onReady={vi.fn()} />);

    await act(async () => {
      fireEvent.click(screen.getByText('Start Test'));
    });

    expect(startMicTestSpy).toHaveBeenCalled();
  });

  test('stop test button calls stopMicTest when running', async () => {
    useAppStore.setState({ isMeterRunning: true });
    render(<MicSetupStep onReady={vi.fn()} />);

    await act(async () => {
      fireEvent.click(screen.getByText('Stop Test'));
    });

    expect(stopMicTestSpy).toHaveBeenCalled();
  });

  test('stops mic test on unmount', () => {
    useAppStore.setState({ isMeterRunning: true });
    const { unmount } = render(<MicSetupStep onReady={vi.fn()} />);

    unmount();

    expect(stopMicTestSpy).toHaveBeenCalled();
  });

  test('does not stop mic test on unmount when not running', () => {
    useAppStore.setState({ isMeterRunning: false });
    const { unmount } = render(<MicSetupStep onReady={vi.fn()} />);

    unmount();

    // refreshDevices is called on mount but stopMicTest should not be
    expect(stopMicTestSpy).not.toHaveBeenCalled();
  });

  test('shows idle hint when mic test is not running', () => {
    render(<MicSetupStep onReady={vi.fn()} />);
    expect(screen.getByText(/Click "Start Test"/)).toBeDefined();
  });

  test('defaults to system default device when no selection', () => {
    useAppStore.setState({
      devices: DEVICES,
      selectedDeviceUid: null,
    });
    render(<MicSetupStep onReady={vi.fn()} />);
    // Should show default device info since no explicit selection
    expect(screen.getByText('48kHz, 1 channel')).toBeDefined();
  });
});
