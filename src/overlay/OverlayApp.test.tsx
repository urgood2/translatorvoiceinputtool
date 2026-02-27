import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { act, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { OverlayApp } from './OverlayApp';

type EventCallback = (event: { payload: unknown }) => void;

describe('OverlayApp listener lifecycle', () => {
  let visibilityState: DocumentVisibilityState;
  let listeners: Map<string, Set<EventCallback>>;
  let unlistenSpies: Map<string, Array<ReturnType<typeof vi.fn>>>;

  const activeListenerCount = (eventName: string): number => listeners.get(eventName)?.size ?? 0;

  const totalUnlistenCalls = (eventName: string): number =>
    (unlistenSpies.get(eventName) ?? []).reduce((sum, spy) => sum + spy.mock.calls.length, 0);

  const emitEvent = (eventName: string, payload: unknown) => {
    const callbacks = listeners.get(eventName);
    if (!callbacks) {
      return;
    }
    for (const callback of callbacks) {
      callback({ payload });
    }
  };

  beforeEach(() => {
    visibilityState = 'visible';
    listeners = new Map();
    unlistenSpies = new Map();

    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      get: () => visibilityState,
    });

    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'get_app_state') {
        return { state: 'idle' };
      }
      return undefined;
    });

    vi.mocked(listen).mockImplementation(async (eventName: string, callback: EventCallback) => {
      if (!listeners.has(eventName)) {
        listeners.set(eventName, new Set());
      }
      listeners.get(eventName)?.add(callback);

      const unlisten = vi.fn(() => {
        listeners.get(eventName)?.delete(callback);
      });
      if (!unlistenSpies.has(eventName)) {
        unlistenSpies.set(eventName, []);
      }
      unlistenSpies.get(eventName)?.push(unlisten);
      return unlisten;
    });
  });

  it('detaches activity listeners while document is hidden and reattaches when visible', async () => {
    render(<OverlayApp />);

    await waitFor(() => {
      expect(activeListenerCount('overlay:toggle')).toBe(1);
      expect(activeListenerCount('recording:status')).toBe(1);
      expect(activeListenerCount('state:changed')).toBe(1);
      expect(activeListenerCount('sidecar:status')).toBe(1);
      expect(activeListenerCount('audio:level')).toBe(1);
    });

    visibilityState = 'hidden';
    act(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });

    await waitFor(() => {
      expect(activeListenerCount('recording:status')).toBe(0);
      expect(activeListenerCount('state:changed')).toBe(0);
      expect(activeListenerCount('sidecar:status')).toBe(0);
      expect(activeListenerCount('audio:level')).toBe(0);
      expect(totalUnlistenCalls('recording:status')).toBeGreaterThan(0);
      expect(totalUnlistenCalls('state:changed')).toBeGreaterThan(0);
      expect(totalUnlistenCalls('sidecar:status')).toBeGreaterThan(0);
      expect(totalUnlistenCalls('audio:level')).toBeGreaterThan(0);
    });

    act(() => {
      emitEvent('recording:status', { phase: 'recording', audio_ms: 20, started_at: undefined });
    });
    expect(screen.queryByText('Recording')).toBeNull();

    visibilityState = 'visible';
    act(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });

    await waitFor(() => {
      expect(activeListenerCount('recording:status')).toBe(1);
      expect(activeListenerCount('state:changed')).toBe(1);
      expect(activeListenerCount('sidecar:status')).toBe(1);
      expect(activeListenerCount('audio:level')).toBe(1);
    });

    act(() => {
      emitEvent('recording:status', { phase: 'recording', audio_ms: 20, started_at: undefined });
    });
    expect(screen.getByText('Recording')).toBeInTheDocument();
  });

  it('detaches activity listeners when overlay is disabled and reattaches when re-enabled', async () => {
    render(<OverlayApp />);

    await waitFor(() => {
      expect(activeListenerCount('overlay:toggle')).toBe(1);
      expect(activeListenerCount('recording:status')).toBe(1);
      expect(activeListenerCount('state:changed')).toBe(1);
      expect(activeListenerCount('sidecar:status')).toBe(1);
      expect(activeListenerCount('audio:level')).toBe(1);
    });

    act(() => {
      emitEvent('overlay:toggle', { enabled: false });
    });

    await waitFor(() => {
      expect(activeListenerCount('recording:status')).toBe(0);
      expect(activeListenerCount('state:changed')).toBe(0);
      expect(activeListenerCount('sidecar:status')).toBe(0);
      expect(activeListenerCount('audio:level')).toBe(0);
      expect(activeListenerCount('overlay:toggle')).toBe(1);
    });

    act(() => {
      emitEvent('recording:status', { phase: 'recording', audio_ms: 20, started_at: undefined });
    });
    expect(screen.queryByText('Recording')).toBeNull();

    act(() => {
      emitEvent('overlay:toggle', { enabled: true });
    });

    await waitFor(() => {
      expect(activeListenerCount('recording:status')).toBe(1);
      expect(activeListenerCount('state:changed')).toBe(1);
      expect(activeListenerCount('sidecar:status')).toBe(1);
      expect(activeListenerCount('audio:level')).toBe(1);
    });

    act(() => {
      emitEvent('recording:status', { phase: 'recording', audio_ms: 20, started_at: undefined });
    });
    expect(screen.getByText('Recording')).toBeInTheDocument();
  });

  it('refreshes app state on re-enable so overlay does not show stale phase', async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({ state: 'idle' })
      .mockResolvedValueOnce({ state: 'recording' });

    render(<OverlayApp />);

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith('get_app_state');
      expect(activeListenerCount('overlay:toggle')).toBe(1);
    });

    act(() => {
      emitEvent('overlay:toggle', { enabled: false });
    });

    await waitFor(() => {
      expect(activeListenerCount('recording:status')).toBe(0);
    });

    act(() => {
      emitEvent('overlay:toggle', { enabled: true });
    });

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledTimes(2);
    });
    expect(screen.getByText('Recording')).toBeInTheDocument();
  });
});
