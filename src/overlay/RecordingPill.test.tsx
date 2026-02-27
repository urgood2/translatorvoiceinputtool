import { render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { RecordingPill } from './RecordingPill';

type MatchMediaConfig = {
  matches: boolean;
};

const originalMatchMedia = window.matchMedia;

function mockMatchMedia({ matches }: MatchMediaConfig): void {
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    writable: true,
    value: (query: string) => ({
      matches,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

describe('Overlay RecordingPill', () => {
  afterEach(() => {
    Object.defineProperty(window, 'matchMedia', {
      configurable: true,
      writable: true,
      value: originalMatchMedia,
    });
  });

  it('is hidden while idle', () => {
    render(
      <RecordingPill
        phase="idle"
        sidecarState="ready"
        timer={<span>00:00</span>}
        waveform={<span>wf</span>}
      />,
    );

    expect(screen.queryByText('Recording')).toBeNull();
    expect(screen.queryByText('Transcribing')).toBeNull();
  });

  it('renders a pulsing red dot while recording', () => {
    mockMatchMedia({ matches: false });
    render(
      <RecordingPill
        phase="recording"
        sidecarState="ready"
        timer={<span>00:00</span>}
        waveform={<span>wf</span>}
      />,
    );

    expect(screen.getByText('Recording')).toBeInTheDocument();
    const dot = screen.getByTestId('recording-dot');
    expect(dot).toBeInTheDocument();
    expect(dot).toHaveStyle({ animation: 'overlay-recording-pulse 1.05s ease-in-out infinite' });
  });

  it('renders a spinner while transcribing', () => {
    mockMatchMedia({ matches: false });
    render(
      <RecordingPill
        phase="transcribing"
        sidecarState="ready"
        timer={<span>00:00</span>}
        waveform={<span>wf</span>}
      />,
    );

    expect(screen.getByText('Transcribing')).toBeInTheDocument();
    const spinner = screen.getByTestId('transcribing-spinner');
    expect(spinner).toBeInTheDocument();
    expect(spinner).toHaveStyle({ animation: 'overlay-transcribing-spin 0.95s linear infinite' });
  });

  it('disables animation when reduced motion is preferred', () => {
    mockMatchMedia({ matches: true });
    render(
      <RecordingPill
        phase="recording"
        sidecarState="ready"
        timer={<span>00:00</span>}
        waveform={<span>wf</span>}
      />,
    );

    expect(screen.getByTestId('recording-dot')).toHaveStyle({ animation: 'none' });
  });

  it('rerenders from idle to recording without hook-order errors', () => {
    mockMatchMedia({ matches: false });
    const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const baseProps = {
      sidecarState: 'ready' as const,
      timer: <span>00:00</span>,
      waveform: <span>wf</span>,
    };

    try {
      const { rerender } = render(
        <RecordingPill
          phase="idle"
          {...baseProps}
        />,
      );

      expect(screen.queryByText('Recording')).toBeNull();
      expect(() =>
        rerender(
          <RecordingPill
            phase="recording"
            {...baseProps}
          />,
        ),
      ).not.toThrow();
      expect(screen.getByText('Recording')).toBeInTheDocument();
      const hookOrderErrorLogged = consoleErrorSpy.mock.calls.some((call) =>
        call.some(
          (arg) =>
            typeof arg === 'string'
            && arg.includes('Rendered more hooks than during the previous render'),
        ),
      );
      expect(hookOrderErrorLogged).toBe(false);
    } finally {
      consoleErrorSpy.mockRestore();
    }
  });
});
