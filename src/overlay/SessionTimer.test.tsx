import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SessionTimer } from './SessionTimer';

describe('Overlay SessionTimer', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  it('displays 00:00 when idle with no audio', () => {
    render(<SessionTimer phase="idle" />);
    expect(screen.getByText('00:00')).toBeTruthy();
  });

  it('displays audioMs when idle', () => {
    render(<SessionTimer phase="idle" audioMs={65000} />);
    expect(screen.getByText('01:05')).toBeTruthy();
  });

  it('displays audioMs during transcribing', () => {
    render(<SessionTimer phase="transcribing" audioMs={125000} />);
    expect(screen.getByText('02:05')).toBeTruthy();
  });

  it('counts up from startedAtMs during recording', () => {
    const now = Date.now();
    vi.setSystemTime(now);

    render(<SessionTimer phase="recording" startedAtMs={now - 3000} />);
    expect(screen.getByText('00:03')).toBeTruthy();
  });

  it('updates at ~2Hz interval during recording', () => {
    const now = 1700000000000;
    vi.setSystemTime(now);

    render(<SessionTimer phase="recording" startedAtMs={now} />);
    expect(screen.getByText('00:00')).toBeTruthy();

    // advanceTimersByTime also advances the fake clock, so Date.now() moves
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(screen.getByText('00:05')).toBeTruthy();
  });

  it('stops counting when phase changes from recording to transcribing', () => {
    const now = Date.now();
    vi.setSystemTime(now);

    const { rerender } = render(
      <SessionTimer phase="recording" startedAtMs={now} audioMs={0} />,
    );
    act(() => {
      vi.setSystemTime(now + 3000);
      vi.advanceTimersByTime(500);
    });
    expect(screen.getByText('00:03')).toBeTruthy();

    rerender(
      <SessionTimer phase="transcribing" audioMs={3000} />,
    );
    // Should show the frozen audioMs value
    expect(screen.getByText('00:03')).toBeTruthy();
  });

  it('uses tabular-nums for consistent character width', () => {
    const { container } = render(<SessionTimer phase="idle" />);
    const span = container.querySelector('span');
    expect(span?.style.fontVariantNumeric).toBe('tabular-nums');
  });

  it('formats minutes and seconds with zero padding', () => {
    render(<SessionTimer phase="idle" audioMs={5000} />);
    expect(screen.getByText('00:05')).toBeTruthy();
  });

  it('handles large durations correctly', () => {
    // 10 minutes 30 seconds
    render(<SessionTimer phase="idle" audioMs={630000} />);
    expect(screen.getByText('10:30')).toBeTruthy();
  });

  it('clamps negative elapsed to zero', () => {
    render(<SessionTimer phase="idle" audioMs={-1000} />);
    expect(screen.getByText('00:00')).toBeTruthy();
  });
});
