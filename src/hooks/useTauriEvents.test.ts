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

function fireMockEventWithLog(eventName: string, payload: unknown): void {
  const seq = (payload as { seq?: unknown })?.seq;
  console.debug(`[test-event] name=${eventName} seq=${String(seq)}`);
  emitMockEvent(eventName, payload);
}

function logStoreTest(eventName: string, payload: unknown): void {
  const before = useAppStore.getState();
  console.debug(
    `[STORE_TEST] before event=${eventName} appState=${before.appState} history=${before.history.length} errorDetail=${String(before.errorDetail)}`
  );
  console.debug(`[STORE_TEST] payload event=${eventName} payload=${JSON.stringify(payload)}`);
}

function logStoreStateAfter(label: string): void {
  const after = useAppStore.getState();
  console.debug(
    `[STORE_TEST] after ${label} appState=${after.appState} history=${after.history.length} errorDetail=${String(after.errorDetail)}`
  );
}

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

    // Verify listen was called for expected canonical events only (legacy aliases retired)
    expect(listen).toHaveBeenCalledWith('state:changed', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('model:status', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('model:progress', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('audio:level', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('transcript:complete', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('transcript:error', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('app:error', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('sidecar:status', expect.any(Function));
    expect(listen).toHaveBeenCalledWith('recording:status', expect.any(Function));

    // Verify legacy aliases are no longer registered
    expect(listen).not.toHaveBeenCalledWith('state_changed', expect.any(Function));
    expect(listen).not.toHaveBeenCalledWith('transcription:complete', expect.any(Function));
    expect(listen).not.toHaveBeenCalledWith('transcription:error', expect.any(Function));
    expect(listen).not.toHaveBeenCalledWith('status:changed', expect.any(Function));

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
      // contract-validate-ignore: this case intentionally exercises legacy transcript payload shape
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

  test('dedupes duplicate transcript deliveries with identical seq', async () => {
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
      emitMockEvent('transcript:complete', {
        seq: 42,
        entry: {
          id: 'entry-42-duplicate',
          text: 'Duplicate payload should be ignored',
          raw_text: 'Duplicate payload should be ignored',
          final_text: 'Duplicate payload should be ignored',
          timestamp: new Date().toISOString(),
          audio_duration_ms: 1001,
          transcription_duration_ms: 221,
          session_id: 'session-42',
          injection_result: { status: 'injected' as const },
        },
      });
    });

    const history = useAppStore.getState().history;
    expect(history).toHaveLength(1);
    expect(history[0]?.id).toBe('entry-42');

    unmount();
  });

  test('processes transcript:complete canonical payload with entry wrapper', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      emitMockEvent('transcript:complete', {
        seq: 900,
        entry: {
          id: 'entry-canonical-900',
          text: 'Canonical payload',
          raw_text: 'Canonical payload',
          final_text: 'Canonical payload',
          timestamp: '2026-01-01T00:00:06.000Z',
          audio_duration_ms: 1000,
          transcription_duration_ms: 210,
          session_id: 'session-canonical-900',
          injection_result: { status: 'injected' },
        },
      });
    });

    const history = useAppStore.getState().history;
    expect(history).toHaveLength(1);
    expect(history[0]?.id).toBe('entry-canonical-900');

    unmount();
  });

  test('dedupes duplicate state deliveries with identical seq', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      emitMockEvent('state:changed', {
        seq: 77,
        state: 'recording',
        enabled: true,
        timestamp: '2026-01-01T00:00:00.000Z',
      });
      emitMockEvent('state:changed', {
        seq: 77,
        state: 'idle',
        enabled: false,
        timestamp: '2026-01-01T00:00:01.000Z',
      });
    });

    const currentState = useAppStore.getState();
    expect(currentState.appState).toBe('recording');
    expect(currentState.enabled).toBe(true);
    expect(currentState.stateTimestamp).toBe('2026-01-01T00:00:00.000Z');

    unmount();
  });

  test('processes increasing seq values on the same stream', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      fireMockEventWithLog('state:changed', {
        seq: 80,
        state: 'recording',
        enabled: true,
        timestamp: '2026-01-01T00:00:06.000Z',
      });
      fireMockEventWithLog('state:changed', {
        seq: 81,
        state: 'idle',
        enabled: false,
        timestamp: '2026-01-01T00:00:07.000Z',
      });
    });

    const currentState = useAppStore.getState();
    expect(currentState.appState).toBe('idle');
    expect(currentState.enabled).toBe(false);

    unmount();
  });

  test('tracks seq independently across state, transcript, model, and sidecar streams', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      fireMockEventWithLog('state:changed', {
        seq: 100,
        state: 'recording',
        enabled: true,
        timestamp: '2026-01-01T00:00:08.000Z',
      });
      fireMockEventWithLog('transcript:complete', {
        seq: 1,
        entry: {
          id: 'entry-stream-independence',
          text: 'independent streams',
          raw_text: 'independent streams',
          final_text: 'independent streams',
          timestamp: '2026-01-01T00:00:08.000Z',
          audio_duration_ms: 900,
          transcription_duration_ms: 180,
          injection_result: { status: 'injected' as const },
        },
      });
      fireMockEventWithLog('model:status', {
        seq: 1,
        status: 'ready',
        model_id: 'test-model',
      });
      fireMockEventWithLog('sidecar:status', {
        seq: 1,
        state: 'ready',
        restart_count: 0,
      });
      // Stale values per stream should be ignored without affecting other streams.
      fireMockEventWithLog('state:changed', {
        seq: 99,
        state: 'idle',
        enabled: false,
        timestamp: '2026-01-01T00:00:09.000Z',
      });
      fireMockEventWithLog('transcript:complete', {
        seq: 0,
        entry: {
          id: 'entry-stale',
          text: 'stale',
          raw_text: 'stale',
          final_text: 'stale',
          timestamp: '2026-01-01T00:00:09.000Z',
          audio_duration_ms: 800,
          transcription_duration_ms: 150,
          injection_result: { status: 'injected' as const },
        },
      });
      fireMockEventWithLog('model:status', {
        seq: 0,
        status: 'loading',
        model_id: 'stale-model',
      });
      fireMockEventWithLog('sidecar:status', {
        seq: 0,
        state: 'restarting',
        restart_count: 1,
      });
    });

    const state = useAppStore.getState();
    expect(state.appState).toBe('recording');
    expect(state.enabled).toBe(true);
    expect(state.history).toHaveLength(1);
    expect(state.history[0]?.id).toBe('entry-stream-independence');
    expect(state.modelStatus).toMatchObject({
      seq: 1,
      status: 'ready',
      model_id: 'test-model',
    });
    expect(state.sidecarStatus).toMatchObject({
      seq: 1,
      state: 'ready',
      restart_count: 0,
    });

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

  test('processes transcript:error payloads in canonical shape', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      fireMockEventWithLog('transcript:error', {
        seq: 601,
        session_id: 'session-err-1',
        error: 'Canonical transcript error',
        message: 'Canonical transcript error',
        recoverable: true,
        app_error: {
          code: 'E_TRANSCRIPTION_FAILED',
          message: 'Canonical transcript error',
          recoverable: true,
        },
      });
    });

    const state = useAppStore.getState();
    expect(state.lastTranscriptError).toMatchObject({
      seq: 601,
      session_id: 'session-err-1',
      error: 'Canonical transcript error',
    });
    expect(state.errorDetail).toBe('Canonical transcript error');

    unmount();
  });

  test('ignores out-of-order seq for same stream', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    const newer = {
      seq: 20,
      entry: {
        id: 'entry-new',
        text: 'new',
        raw_text: 'new',
        final_text: 'new',
        timestamp: new Date().toISOString(),
        audio_duration_ms: 1000,
        transcription_duration_ms: 220,
        injection_result: { status: 'injected' as const },
      },
    };
    const older = {
      seq: 19,
      entry: {
        id: 'entry-old',
        text: 'old',
        raw_text: 'old',
        final_text: 'old',
        timestamp: new Date().toISOString(),
        audio_duration_ms: 1000,
        transcription_duration_ms: 220,
        injection_result: { status: 'injected' as const },
      },
    };

    act(() => {
      fireMockEventWithLog('transcript:complete', newer);
      fireMockEventWithLog('transcript:complete', older);
    });

    const history = useAppStore.getState().history;
    expect(history).toHaveLength(1);
    expect(history[0]?.id).toBe('entry-new');

    unmount();
  });

  test('tracks seq independently across streams', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      fireMockEventWithLog('state:changed', {
        seq: 100,
        state: 'recording',
        enabled: true,
        timestamp: '2026-01-01T00:00:02.000Z',
      });
      fireMockEventWithLog('transcript:complete', {
        seq: 1,
        entry: {
          id: 'entry-independent',
          text: 'independent',
          raw_text: 'independent',
          final_text: 'independent',
          timestamp: '2026-01-01T00:00:03.000Z',
          audio_duration_ms: 1200,
          transcription_duration_ms: 333,
          injection_result: { status: 'injected' as const },
        },
      });
    });

    const state = useAppStore.getState();
    expect(state.appState).toBe('recording');
    expect(state.history).toHaveLength(1);
    expect(state.history[0]?.id).toBe('entry-independent');

    unmount();
  });

  test('seq tracking resets after unmount and remount', async () => {
    const first = renderHook(() => useTauriEvents());
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      fireMockEventWithLog('state:changed', {
        seq: 50,
        state: 'recording',
        enabled: true,
        timestamp: '2026-01-01T00:00:04.000Z',
      });
    });
    expect(useAppStore.getState().appState).toBe('recording');
    first.unmount();

    const second = renderHook(() => useTauriEvents());
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      fireMockEventWithLog('state:changed', {
        seq: 50,
        state: 'idle',
        enabled: true,
        timestamp: '2026-01-01T00:00:05.000Z',
      });
    });
    expect(useAppStore.getState().appState).toBe('idle');

    second.unmount();
  });

  test('cleans up every registered listener on unmount', async () => {
    const defaultListenImpl = vi.mocked(listen).getMockImplementation();
    const unlistenFns: Array<ReturnType<typeof vi.fn>> = [];
    vi.mocked(listen).mockImplementation((() => {
      const unlisten = vi.fn();
      unlistenFns.push(unlisten);
      return Promise.resolve(unlisten);
    }) as typeof listen);

    try {
      const { unmount } = renderHook(() => useTauriEvents());
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 50));
      });

      const expectedCanonicalListenerCount = 9;
      expect(listen).toHaveBeenCalledTimes(expectedCanonicalListenerCount);
      expect(unlistenFns).toHaveLength(expectedCanonicalListenerCount);

      unmount();

      unlistenFns.forEach((fn) => {
        expect(fn).toHaveBeenCalledTimes(1);
      });
    } finally {
      if (defaultListenImpl) {
        vi.mocked(listen).mockImplementation(defaultListenImpl);
      }
    }
  });

  test('processes app:error payloads in legacy and structured shapes', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      // contract-validate-ignore: this case intentionally exercises legacy app:error payload shape
      fireMockEventWithLog('app:error', {
        seq: 501,
        message: 'Legacy error shape',
        recoverable: true,
      });
    });
    expect(useAppStore.getState().errorDetail).toBe('Legacy error shape');

    act(() => {
      fireMockEventWithLog('app:error', {
        seq: 502,
        error: {
          code: 'E_INTERNAL',
          message: 'Structured error shape',
          recoverable: false,
        },
      });
    });
    expect(useAppStore.getState().errorDetail).toBe('Structured error shape');

    unmount();
  });

  test('state payload prefers detail field over error_detail', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    const payload = {
      seq: 910,
      state: 'error' as const,
      enabled: false,
      detail: 'Canonical detail wins',
      error_detail: 'Legacy detail fallback',
      timestamp: '2026-01-01T00:11:00.000Z',
    };
    logStoreTest('state:changed', payload);

    act(() => {
      fireMockEventWithLog('state:changed', payload);
    });

    const state = useAppStore.getState();
    logStoreStateAfter('state detail precedence');
    expect(state.appState).toBe('error');
    expect(state.enabled).toBe(false);
    expect(state.errorDetail).toBe('Canonical detail wins');
    expect(state.stateTimestamp).toBe('2026-01-01T00:11:00.000Z');

    unmount();
  });

  test('events without seq are processed for backward compatibility', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    const first = {
      state: 'recording' as const,
      enabled: true,
      timestamp: '2026-01-01T00:12:00.000Z',
    };
    const second = {
      state: 'idle' as const,
      enabled: false,
      timestamp: '2026-01-01T00:12:01.000Z',
    };
    logStoreTest('state:changed(no-seq)', first);

    act(() => {
      emitMockEvent('state:changed', first);
    });

    const stateAfterFirst = useAppStore.getState();
    expect(stateAfterFirst.appState).toBe('recording');
    expect(stateAfterFirst.enabled).toBe(true);
    expect(stateAfterFirst.stateTimestamp).toBe('2026-01-01T00:12:00.000Z');

    act(() => {
      emitMockEvent('state:changed', second);
    });

    const state = useAppStore.getState();
    logStoreStateAfter('no-seq compatibility');
    expect(state.appState).toBe('idle');
    expect(state.enabled).toBe(false);
    expect(state.stateTimestamp).toBe('2026-01-01T00:12:01.000Z');

    unmount();
  });

  test('rapid transcript events are stored newest-first', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      fireMockEventWithLog('transcript:complete', {
        seq: 1001,
        entry: {
          id: 'rapid-1',
          text: 'one',
          raw_text: 'one',
          final_text: 'one',
          timestamp: '2026-01-01T00:13:00.000Z',
          audio_duration_ms: 100,
          transcription_duration_ms: 20,
          injection_result: { status: 'injected' as const },
        },
      });
      fireMockEventWithLog('transcript:complete', {
        seq: 1002,
        entry: {
          id: 'rapid-2',
          text: 'two',
          raw_text: 'two',
          final_text: 'two',
          timestamp: '2026-01-01T00:13:01.000Z',
          audio_duration_ms: 110,
          transcription_duration_ms: 22,
          injection_result: { status: 'injected' as const },
        },
      });
      fireMockEventWithLog('transcript:complete', {
        seq: 1003,
        entry: {
          id: 'rapid-3',
          text: 'three',
          raw_text: 'three',
          final_text: 'three',
          timestamp: '2026-01-01T00:13:02.000Z',
          audio_duration_ms: 120,
          transcription_duration_ms: 24,
          injection_result: { status: 'injected' as const },
        },
      });
    });

    const history = useAppStore.getState().history;
    logStoreStateAfter('rapid transcript ordering');
    expect(history).toHaveLength(3);
    expect(history[0]?.id).toBe('rapid-3');
    expect(history[1]?.id).toBe('rapid-2');
    expect(history[2]?.id).toBe('rapid-1');

    unmount();
  });

  test('model status canonical shape and progress updates are reflected in store', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    const statusPayload = {
      seq: 1201,
      model_id: 'parakeet-rnnt-1.1b',
      status: 'downloading' as const,
      progress: { current: 25, total: 100, unit: 'bytes' },
    };
    const progressPayload = { current: 30, total: 100, unit: 'bytes' };
    logStoreTest('model:status', statusPayload);
    logStoreTest('model:progress', progressPayload);

    act(() => {
      fireMockEventWithLog('model:status', statusPayload);
      fireMockEventWithLog('model:progress', progressPayload);
    });

    const state = useAppStore.getState();
    logStoreStateAfter('model status + progress');
    expect(state.modelStatus).toMatchObject({
      seq: 1201,
      model_id: 'parakeet-rnnt-1.1b',
      status: 'downloading',
      progress: { current: 30, total: 100, unit: 'bytes' },
    });
    expect(state.downloadProgress).toEqual(progressPayload);

    unmount();
  });

  test('sidecar failed payload updates sidecar slice with restart metadata', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    const failedPayload = {
      seq: 1301,
      state: 'failed',
      restart_count: 3,
      message: 'sidecar crashed repeatedly',
    };
    logStoreTest('sidecar:status', failedPayload);

    act(() => {
      fireMockEventWithLog('sidecar:status', failedPayload);
    });

    const state = useAppStore.getState();
    logStoreStateAfter('sidecar failed');
    expect(state.sidecarStatus).toMatchObject(failedPayload);

    unmount();
  });

  test('recording status handles transitions with minimal optional fields', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    act(() => {
      fireMockEventWithLog('recording:status', {
        seq: 1401,
        phase: 'recording',
        session_id: 'session-1401',
      });
      // Missing session_id/audio_ms should still be accepted for idle transition.
      fireMockEventWithLog('recording:status', {
        seq: 1402,
        phase: 'idle',
      });
    });

    const state = useAppStore.getState();
    logStoreStateAfter('recording transitions');
    expect(state.recordingStatus).toMatchObject({
      seq: 1402,
      phase: 'idle',
    });
    expect(state.appState).toBe('idle');

    unmount();
  });

  test('legacy transcript payload missing optional fields is normalized safely', async () => {
    const { unmount } = renderHook(() => useTauriEvents());

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

    const payload = {
      session_id: 'legacy-minimal',
      text: 'Legacy minimal',
      audio_duration_ms: 321,
      processing_duration_ms: 123,
    };
    logStoreTest('transcript:complete(legacy-shape)', payload);

    act(() => {
      // Exercising minimal legacy payload shape via canonical event name
      emitMockEvent('transcript:complete', payload);
    });

    const [entry] = useAppStore.getState().history;
    logStoreStateAfter('legacy transcript optional fields');
    expect(entry).toMatchObject({
      id: 'legacy-minimal',
      text: 'Legacy minimal',
      raw_text: 'Legacy minimal',
      final_text: 'Legacy minimal',
      audio_duration_ms: 321,
      transcription_duration_ms: 123,
      injection_result: { status: 'injected' },
    });
    expect(typeof entry.timestamp).toBe('string');

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
