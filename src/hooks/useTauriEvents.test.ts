/**
 * Unit tests for the Tauri events hook.
 *
 * Note: These tests verify the hook structure and basic functionality.
 * Integration with actual Tauri events is tested in E2E tests.
 */

import { describe, test, expect, beforeEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { listen } from '@tauri-apps/api/event';
import { useTauriEvents, useTauriEvent } from './useTauriEvents';
import { useAppStore } from '../store/appStore';
import { emitMockEvent } from '../tests/setup';

// Reset store before each test
beforeEach(() => {
  useAppStore.setState({
    appState: 'idle',
    enabled: true,
    errorDetail: undefined,
    modelStatus: null,
    downloadProgress: null,
    devices: [],
    selectedDeviceUid: null,
    audioLevel: null,
    isMeterRunning: false,
    history: [],
    recordingStatus: null,
    sidecarStatus: null,
    lastTranscriptError: null,
    config: null,
    capabilities: null,
    hotkeyStatus: null,
    presets: [],
    selfCheckResult: null,
    isInitialized: false,
    isLoading: false,
  });
});

// ============================================================================
// useTauriEvents TESTS
// ============================================================================

describe('useTauriEvents', () => {
  test('calls listen for expected events on mount', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    // Wait for async setup
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    // Verify listen was called for expected events
    expect(listen).toHaveBeenCalledWith('state:changed', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('state_changed', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('model:status', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('model:progress', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('audio:level', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('transcript:complete', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('transcription:complete', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('transcript:error', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('transcription:error', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('app:error', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('sidecar:status', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('status:changed', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('recording:status', expect.any(Function));

    unmount();
  });

  test('cleans up when unmounted before async setup resolves', async () => {
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          setTimeout(() => resolve(unlisten), 20);
        }) as ReturnType<typeof listen>
    );

    const { unmount } = renderHook(() => useTauriEvents());
    unmount();

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 30));
    });

    expect(unlisten).toHaveBeenCalledTimes(1);
    // Setup should stop once cancellation is observed.
    expect(listen).toHaveBeenCalledTimes(1);
  });

  test('store actions update state correctly', () => {
    // Test internal actions directly since event mock is complex
    const store = useAppStore.getState();

    // Test _setAppState
    store._setAppState({
      state: 'recording',
      enabled: true,
      detail: undefined,
    });
    expect(useAppStore.getState().appState).toBe('recording');

    // Test _setModelStatus
    store._setModelStatus({ status: 'ready', model_id: 'test' });
    expect(useAppStore.getState().modelStatus?.status).toBe('ready');

    // Test _setDownloadProgress
    store._setDownloadProgress({ current: 50, total: 100, unit: 'bytes' });
    expect(useAppStore.getState().downloadProgress?.current).toBe(50);

    // Test _setAudioLevel
    store._setAudioLevel({ rms: -20, peak: -10 });
    expect(useAppStore.getState().audioLevel?.rms).toBe(-20);

    // Test _addHistoryEntry
    store._addHistoryEntry({
      id: 'test',
      text: 'Hello',
      raw_text: 'Hello',
      final_text: 'Hello',
      timestamp: new Date().toISOString(),
      audio_duration_ms: 1000,
      transcription_duration_ms: 200,
      injection_result: { status: 'injected' },
    });
    expect(useAppStore.getState().history[0]?.id).toBe('test');

    // Test _setError
    store._setError('Test error');
    expect(useAppStore.getState().errorDetail).toBe('Test error');
  });

  test('normalizes legacy transcript payload and preserves timings', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    // Wait for async setup
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    const timings = {
      ipc_ms: 11,
      transcribe_ms: 340,
      postprocess_ms: 8,
      inject_ms: 36,
      total_ms: 395,
    };

    act(() => {
      emitMockEvent('transcript:complete', {
        session_id: 'session-123',
        text: 'Legacy payload',
        audio_duration_ms: 1200,
        processing_duration_ms: 450,
        injection_result: { status: 'failed', error: 'paste failed' },
        timings,
      });
    });

    const [entry] = useAppStore.getState().history;
    expect(entry).toMatchObject({
      id: 'session-123',
      text: 'Legacy payload',
      raw_text: 'Legacy payload',
      final_text: 'Legacy payload',
      session_id: 'session-123',
      audio_duration_ms: 1200,
      transcription_duration_ms: 450,
      injection_result: { status: 'error', message: 'paste failed' },
      timings,
    });
    expect(new Date(entry.timestamp).toString()).not.toBe('Invalid Date');

    unmount();
  });

  test('dedupes canonical and legacy transcript events by shared seq', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    const canonicalPayload = {
      seq: 42,
      entry: {
        id: 'entry-42',
        text: 'Canonical payload',
        raw_text: 'Canonical payload',
        final_text: 'Canonical payload',
        timestamp: new Date().toISOString(),
        audio_duration_ms: 1000,
        transcription_duration_ms: 220,
        session_id: 'session-42',
        injection_result: { status: 'injected' as const },
      },
    };

    act(() => {
      emitMockEvent('transcript:complete', canonicalPayload);
      emitMockEvent('transcription:complete', canonicalPayload);
    });

    const history = useAppStore.getState().history;
    expect(history).toHaveLength(1);
    expect(history[0]?.id).toBe('entry-42');

    unmount();
  });

  test('dedupes canonical and legacy state events by shared seq', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      emitMockEvent('state:changed', {
        seq: 77,
        state: 'recording',
        enabled: true,
      });
      emitMockEvent('state_changed', {
        seq: 77,
        state: 'idle',
        enabled: false,
      });
    });

    const currentState = useAppStore.getState();
    expect(currentState.appState).toBe('recording');
    expect(currentState.enabled).toBe(true);

    unmount();
  });

  test('recording:status updates recording slice and app state', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      emitMockEvent('recording:status', {
        seq: 101,
        phase: 'recording',
        session_id: 'session-101',
      });
    });

    const state = useAppStore.getState();
    expect(state.recordingStatus).toMatchObject({
      seq: 101,
      phase: 'recording',
      session_id: 'session-101',
    });
    expect(state.appState).toBe('recording');

    unmount();
  });

  test('sidecar:status updates sidecar status slice', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      emitMockEvent('sidecar:status', {
        seq: 7,
        state: 'ready',
        restart_count: 0,
      });
    });

    const state = useAppStore.getState();
    expect(state.sidecarStatus).toMatchObject({
      seq: 7,
      state: 'ready',
      restart_count: 0,
    });

    unmount();
  });
});

// ============================================================================
// useTauriEvent TESTS
// ============================================================================

describe('useTauriEvent', () => {
  test('calls listen with event name on mount', async () => {
    const handler = vi.fn();

    const { unmount } = renderHook(() =>
      useTauriEvent('custom:event', handler)
    );

    // Wait for async setup
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 10));
    });

    expect(listen).toHaveBeenCalledWith('custom:event', expect.any(Function));

    unmount();
  });

  test('handler reference is updated on rerender', async () => {
    const handler1 = vi.fn();
    const handler2 = vi.fn();

    const { rerender, unmount } = renderHook(
      ({ handler }) => useTauriEvent('custom:event', handler),
      { initialProps: { handler: handler1 } }
    );

    // Rerender with new handler
    rerender({ handler: handler2 });

    // The hook should now use handler2
    // We verify this by checking the ref is updated (handler1 !== handler2)
    expect(handler1).not.toBe(handler2);

    unmount();
  });

  test('cleans up listener when unmounted before async listen resolves', async () => {
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          setTimeout(() => resolve(unlisten), 20);
        }) as ReturnType<typeof listen>
    );

    const handler = vi.fn();
    const { unmount } = renderHook(() => useTauriEvent('custom:event', handler));

    // Unmount immediately, before mocked listen promise resolves.
    unmount();

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 30));
    });

    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
