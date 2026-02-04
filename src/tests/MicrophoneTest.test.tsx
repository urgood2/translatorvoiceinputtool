/**
 * Tests for MicrophoneTest component.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { MicrophoneTest } from '../components/Settings/MicrophoneTest';

describe('MicrophoneTest', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders test button in idle state', () => {
    render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={vi.fn()}
        onStopTest={vi.fn()}
        audioLevel={null}
      />
    );
    expect(screen.getByText('Start Test')).toBeDefined();
    expect(screen.getByText('Microphone Test')).toBeDefined();
  });

  it('shows idle hint when not running', () => {
    render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={vi.fn()}
        onStopTest={vi.fn()}
        audioLevel={null}
      />
    );
    expect(screen.getByText(/Click "Start Test"/)).toBeDefined();
  });

  it('calls onStartTest when start button clicked', async () => {
    const onStartTest = vi.fn().mockResolvedValue(undefined);
    render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={onStartTest}
        onStopTest={vi.fn()}
        audioLevel={null}
      />
    );

    await act(async () => {
      fireEvent.click(screen.getByText('Start Test'));
    });

    expect(onStartTest).toHaveBeenCalled();
  });

  it('shows stop button when running', () => {
    render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={vi.fn()}
        onStopTest={vi.fn()}
        audioLevel={{ rms: 0.3, peak: 0.4 }}
        isRunning={true}
      />
    );
    expect(screen.getByText('Stop Test')).toBeDefined();
  });

  it('calls onStopTest when stop button clicked', async () => {
    const onStopTest = vi.fn().mockResolvedValue(undefined);
    render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={vi.fn()}
        onStopTest={onStopTest}
        audioLevel={{ rms: 0.3, peak: 0.4 }}
        isRunning={true}
      />
    );

    await act(async () => {
      fireEvent.click(screen.getByText('Stop Test'));
    });

    expect(onStopTest).toHaveBeenCalled();
  });

  it('displays level percentage', () => {
    render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={vi.fn()}
        onStopTest={vi.fn()}
        audioLevel={{ rms: 0.5, peak: 0.6 }}
        isRunning={true}
      />
    );

    // The component uses animation, so we check for level display element
    expect(screen.getByText(/%$/)).toBeDefined();
  });

  it('shows no signal warning after timeout', async () => {
    render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={vi.fn()}
        onStopTest={vi.fn()}
        audioLevel={{ rms: 0, peak: 0 }}
        isRunning={true}
      />
    );

    // Advance timer past no-signal timeout (3 seconds)
    await act(async () => {
      vi.advanceTimersByTime(3500);
    });

    expect(screen.getByText('No audio detected')).toBeDefined();
  });

  it('shows check microphone hint with no signal', async () => {
    render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={vi.fn()}
        onStopTest={vi.fn()}
        audioLevel={{ rms: 0, peak: 0 }}
        isRunning={true}
      />
    );

    await act(async () => {
      vi.advanceTimersByTime(3500);
    });

    expect(screen.getByText(/Check your microphone connection/)).toBeDefined();
  });

  it('hides no signal warning when audio detected', async () => {
    const { rerender } = render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={vi.fn()}
        onStopTest={vi.fn()}
        audioLevel={{ rms: 0, peak: 0 }}
        isRunning={true}
      />
    );

    // Trigger no signal
    await act(async () => {
      vi.advanceTimersByTime(3500);
    });

    expect(screen.getByText('No audio detected')).toBeDefined();

    // Simulate audio detected
    rerender(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={vi.fn()}
        onStopTest={vi.fn()}
        audioLevel={{ rms: 0.5, peak: 0.6 }}
        isRunning={true}
      />
    );

    await act(async () => {
      vi.advanceTimersByTime(100);
    });

    expect(screen.queryByText('No audio detected')).toBeNull();
  });

  it('displays error when onStartTest fails', async () => {
    const onStartTest = vi.fn().mockRejectedValue(new Error('Permission denied'));
    render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={onStartTest}
        onStopTest={vi.fn()}
        audioLevel={null}
      />
    );

    await act(async () => {
      fireEvent.click(screen.getByText('Start Test'));
    });

    expect(screen.getByText('Permission denied')).toBeDefined();
  });

  it('shows legend markers', () => {
    render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={vi.fn()}
        onStopTest={vi.fn()}
        audioLevel={{ rms: 0.3, peak: 0.4 }}
        isRunning={true}
      />
    );

    expect(screen.getByText('Silent')).toBeDefined();
    expect(screen.getByText('Normal')).toBeDefined();
    expect(screen.getByText('Loud')).toBeDefined();
    expect(screen.getByText('Clipping')).toBeDefined();
  });

  it('disables button while starting', async () => {
    const onStartTest = vi.fn().mockImplementation(() => new Promise(() => {})); // Never resolves
    render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={onStartTest}
        onStopTest={vi.fn()}
        audioLevel={null}
      />
    );

    fireEvent.click(screen.getByText('Start Test'));

    // Button should show "Starting..." and be disabled
    expect(screen.getByText('Starting...')).toBeDefined();
    expect(screen.getByRole('button')).toHaveProperty('disabled', true);
  });

  it('does not show no signal warning when not running', () => {
    render(
      <MicrophoneTest
        deviceUid="device-1"
        onStartTest={vi.fn()}
        onStopTest={vi.fn()}
        audioLevel={{ rms: 0, peak: 0 }}
        isRunning={false}
      />
    );

    expect(screen.queryByText('No audio detected')).toBeNull();
  });
});
