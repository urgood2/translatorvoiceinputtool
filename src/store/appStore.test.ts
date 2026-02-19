/**
 * Unit tests for the Zustand app store.
 */

import { describe, test, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import {
  useAppStore,
  selectAppState,
  selectIsRecording,
  selectIsTranscribing,
  selectIsIdle,
  selectModelReady,
  selectDevices,
  selectHistory,
  selectConfig,
} from './appStore';
import {
  setMockInvokeHandler,
  createMockDevice,
  createMockTranscript,
  createMockModelStatus,
  createMockConfig,
} from '../tests/setup';
import type { DiagnosticsReport } from '../types';

// ============================================================================
// TEST SETUP
// ============================================================================

// Initial state for resetting between tests
const getInitialState = () => ({
  appState: 'idle' as const,
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

beforeEach(() => {
  // Reset store to initial state
  useAppStore.setState(getInitialState());
});

// ============================================================================
// SELECTOR TESTS
// ============================================================================

describe('Selectors', () => {
  test('selectAppState returns current app state', () => {
    useAppStore.setState({ appState: 'recording' });
    expect(selectAppState(useAppStore.getState())).toBe('recording');
  });

  test('selectIsRecording returns true when recording', () => {
    useAppStore.setState({ appState: 'recording' });
    expect(selectIsRecording(useAppStore.getState())).toBe(true);

    useAppStore.setState({ appState: 'idle' });
    expect(selectIsRecording(useAppStore.getState())).toBe(false);
  });

  test('selectIsTranscribing returns true when transcribing', () => {
    useAppStore.setState({ appState: 'transcribing' });
    expect(selectIsTranscribing(useAppStore.getState())).toBe(true);
  });

  test('selectIsIdle returns true when idle', () => {
    useAppStore.setState({ appState: 'idle' });
    expect(selectIsIdle(useAppStore.getState())).toBe(true);

    useAppStore.setState({ appState: 'recording' });
    expect(selectIsIdle(useAppStore.getState())).toBe(false);
  });

  test('selectModelReady returns true when model is ready', () => {
    useAppStore.setState({ modelStatus: { status: 'ready', model_id: 'test' } });
    expect(selectModelReady(useAppStore.getState())).toBe(true);

    useAppStore.setState({ modelStatus: { status: 'downloading', model_id: 'test' } });
    expect(selectModelReady(useAppStore.getState())).toBe(false);

    useAppStore.setState({ modelStatus: null });
    expect(selectModelReady(useAppStore.getState())).toBe(false);
  });

  test('selectDevices returns device list', () => {
    const devices = [createMockDevice({ uid: '1' }), createMockDevice({ uid: '2' })];
    useAppStore.setState({ devices });
    expect(selectDevices(useAppStore.getState())).toEqual(devices);
  });

  test('selectHistory returns transcript history', () => {
    const history = [createMockTranscript({ id: '1' }), createMockTranscript({ id: '2' })];
    useAppStore.setState({ history });
    expect(selectHistory(useAppStore.getState())).toEqual(history);
  });

  test('selectConfig returns config', () => {
    const config = createMockConfig();
    useAppStore.setState({ config });
    expect(selectConfig(useAppStore.getState())).toEqual(config);
  });
});

// ============================================================================
// DEVICE ACTION TESTS
// ============================================================================

describe('Device Actions', () => {
  test('refreshDevices updates device list', async () => {
    const mockDevices = [
      createMockDevice({ uid: 'device-1', name: 'Mic 1' }),
      createMockDevice({ uid: 'device-2', name: 'Mic 2', is_default: true }),
    ];

    setMockInvokeHandler((cmd) => {
      if (cmd === 'list_audio_devices') return mockDevices;
      return undefined;
    });

    await useAppStore.getState().refreshDevices();

    expect(useAppStore.getState().devices).toEqual(mockDevices);
  });

  test('selectDevice calls Tauri and updates state', async () => {
    setMockInvokeHandler((cmd, args) => {
      if (cmd === 'set_audio_device') {
        expect(args).toEqual({ deviceUid: 'new-device' });
        return undefined;
      }
      return undefined;
    });

    await useAppStore.getState().selectDevice('new-device');

    expect(invoke).toHaveBeenCalledWith('set_audio_device', { deviceUid: 'new-device' });
    expect(useAppStore.getState().selectedDeviceUid).toBe('new-device');
  });

  test('selectDevice updates config when present', async () => {
    const config = createMockConfig();
    useAppStore.setState({ config });

    setMockInvokeHandler(() => undefined);

    await useAppStore.getState().selectDevice('new-device');

    expect(useAppStore.getState().config?.audio.device_uid).toBe('new-device');
  });

  test('selectDevice with null clears device', async () => {
    useAppStore.setState({ selectedDeviceUid: 'old-device' });

    setMockInvokeHandler(() => undefined);

    await useAppStore.getState().selectDevice(null);

    expect(useAppStore.getState().selectedDeviceUid).toBeNull();
  });

  test('startMicTest enables meter and resets audio level', async () => {
    useAppStore.setState({
      isMeterRunning: false,
      audioLevel: { rms: -12, peak: -6, source: 'meter' },
    });

    setMockInvokeHandler((cmd) => {
      if (cmd === 'start_mic_test') return undefined;
      return undefined;
    });

    await useAppStore.getState().startMicTest();

    expect(invoke).toHaveBeenCalledWith('start_mic_test');
    expect(useAppStore.getState().isMeterRunning).toBe(true);
    expect(useAppStore.getState().audioLevel).toBeNull();
  });

  test('stopMicTest disables meter and resets audio level', async () => {
    useAppStore.setState({
      isMeterRunning: true,
      audioLevel: { rms: -15, peak: -8, source: 'meter' },
    });

    setMockInvokeHandler((cmd) => {
      if (cmd === 'stop_mic_test') return undefined;
      return undefined;
    });

    await useAppStore.getState().stopMicTest();

    expect(invoke).toHaveBeenCalledWith('stop_mic_test');
    expect(useAppStore.getState().isMeterRunning).toBe(false);
    expect(useAppStore.getState().audioLevel).toBeNull();
  });
});

// ============================================================================
// CONFIG ACTION TESTS
// ============================================================================

describe('Config Actions', () => {
  test('loadConfig fetches and stores config', async () => {
    const mockConfig = createMockConfig();
    mockConfig.audio.device_uid = 'saved-device';

    setMockInvokeHandler((cmd) => {
      if (cmd === 'get_config') return mockConfig;
      return undefined;
    });

    await useAppStore.getState().loadConfig();

    expect(useAppStore.getState().config).toEqual(mockConfig);
    expect(useAppStore.getState().selectedDeviceUid).toBe('saved-device');
  });

  test('updateAudioConfig merges audio settings', async () => {
    const config = createMockConfig();
    useAppStore.setState({ config });

    setMockInvokeHandler(() => undefined);

    await useAppStore.getState().updateAudioConfig({ sample_rate: 48000 });

    expect(useAppStore.getState().config?.audio.sample_rate).toBe(48000);
    expect(invoke).toHaveBeenCalledWith('update_config', expect.anything());
  });

  test('updateAudioConfig restarts mic test when device changes while meter is running', async () => {
    const config = createMockConfig();
    config.audio.device_uid = 'old-device';
    useAppStore.setState({
      config,
      selectedDeviceUid: 'old-device',
      isMeterRunning: true,
      audioLevel: { rms: -10, peak: -4, source: 'meter' },
    });

    setMockInvokeHandler((cmd) => {
      if (cmd === 'stop_mic_test') return undefined;
      if (cmd === 'update_config') return undefined;
      if (cmd === 'start_mic_test') return undefined;
      return undefined;
    });

    await useAppStore.getState().updateAudioConfig({ device_uid: 'new-device' });

    expect(invoke).toHaveBeenNthCalledWith(1, 'stop_mic_test');
    expect(invoke).toHaveBeenNthCalledWith(2, 'update_config', expect.anything());
    expect(invoke).toHaveBeenNthCalledWith(3, 'start_mic_test');
    expect(useAppStore.getState().config?.audio.device_uid).toBe('new-device');
    expect(useAppStore.getState().selectedDeviceUid).toBe('new-device');
    expect(useAppStore.getState().isMeterRunning).toBe(true);
  });

  test('updateAudioConfig does nothing without config', async () => {
    await useAppStore.getState().updateAudioConfig({ sample_rate: 48000 });

    // Should not throw or call invoke
    expect(useAppStore.getState().config).toBeNull();
  });

  test('resetConfig reloads config', async () => {
    const freshConfig = createMockConfig();

    setMockInvokeHandler((cmd) => {
      if (cmd === 'reset_config_to_defaults') return undefined;
      if (cmd === 'get_config') return freshConfig;
      return undefined;
    });

    await useAppStore.getState().resetConfig();

    expect(invoke).toHaveBeenCalledWith('reset_config_to_defaults');
    expect(useAppStore.getState().config).toEqual(freshConfig);
  });

  test('updateHotkeyConfig merges config and refreshes hotkey status', async () => {
    const config = createMockConfig();
    const hotkeyStatus = { registered: true, error: null };
    useAppStore.setState({ config });

    setMockInvokeHandler((cmd) => {
      if (cmd === 'update_config') return undefined;
      if (cmd === 'get_hotkey_status') return hotkeyStatus;
      return undefined;
    });

    await useAppStore.getState().updateHotkeyConfig({ primary: 'Alt+Space' });

    expect(useAppStore.getState().config?.hotkeys.primary).toBe('Alt+Space');
    expect(useAppStore.getState().hotkeyStatus).toEqual(hotkeyStatus);
  });

  test('updateInjectionConfig merges injection settings', async () => {
    const config = createMockConfig();
    useAppStore.setState({ config });

    setMockInvokeHandler((cmd) => {
      if (cmd === 'update_config') return undefined;
      return undefined;
    });

    await useAppStore.getState().updateInjectionConfig({ auto_paste: false });

    expect(useAppStore.getState().config?.injection.auto_paste).toBe(false);
    expect(invoke).toHaveBeenCalledWith('update_config', expect.anything());
  });

  test('setReplacementRules updates config replacements', async () => {
    const config = createMockConfig();
    useAppStore.setState({ config });
    const rules = [
      {
        id: 'rule-1',
        enabled: true,
        kind: 'literal',
        pattern: 'btw',
        replacement: 'by the way',
        word_boundary: true,
        case_sensitive: false,
      },
    ];

    setMockInvokeHandler((cmd) => {
      if (cmd === 'set_replacement_rules') return undefined;
      return undefined;
    });

    await useAppStore.getState().setReplacementRules(rules);

    expect(invoke).toHaveBeenCalledWith('set_replacement_rules', { rules });
    expect(useAppStore.getState().config?.replacements).toEqual(rules);
  });
});

// ============================================================================
// MODEL ACTION TESTS
// ============================================================================

describe('Model Actions', () => {
  test('refreshModelStatus updates model status', async () => {
    const mockStatus = createMockModelStatus();

    setMockInvokeHandler((cmd) => {
      if (cmd === 'get_model_status') return mockStatus;
      return undefined;
    });

    await useAppStore.getState().refreshModelStatus();

    expect(useAppStore.getState().modelStatus).toEqual(mockStatus);
  });

  test('downloadModel sets progress and refreshes status', async () => {
    const finalStatus = createMockModelStatus({ status: 'ready' });

    setMockInvokeHandler((cmd) => {
      if (cmd === 'download_model') return undefined;
      if (cmd === 'get_model_status') return finalStatus;
      return undefined;
    });

    await useAppStore.getState().downloadModel();

    expect(useAppStore.getState().downloadProgress).toBeNull();
    expect(useAppStore.getState().modelStatus).toEqual(finalStatus);
  });

  test('purgeModelCache refreshes status after purge', async () => {
    const status = createMockModelStatus({ status: 'not_downloaded' });

    setMockInvokeHandler((cmd) => {
      if (cmd === 'purge_model_cache') return undefined;
      if (cmd === 'get_model_status') return status;
      return undefined;
    });

    await useAppStore.getState().purgeModelCache();

    expect(invoke).toHaveBeenCalledWith('purge_model_cache');
    expect(useAppStore.getState().modelStatus).toEqual(status);
  });
});

// ============================================================================
// HISTORY ACTION TESTS
// ============================================================================

describe('History Actions', () => {
  test('refreshHistory fetches transcript history', async () => {
    const mockHistory = [
      createMockTranscript({ id: '1', text: 'First' }),
      createMockTranscript({ id: '2', text: 'Second' }),
    ];

    setMockInvokeHandler((cmd) => {
      if (cmd === 'get_transcript_history') return mockHistory;
      return undefined;
    });

    await useAppStore.getState().refreshHistory();

    expect(useAppStore.getState().history).toEqual(mockHistory);
  });

  test('copyTranscript invokes copy command', async () => {
    setMockInvokeHandler(() => undefined);

    await useAppStore.getState().copyTranscript('transcript-123');

    expect(invoke).toHaveBeenCalledWith('copy_transcript', { id: 'transcript-123' });
  });

  test('copyLastTranscript invokes copy last command', async () => {
    setMockInvokeHandler(() => undefined);

    await useAppStore.getState().copyLastTranscript();

    expect(invoke).toHaveBeenCalledWith('copy_last_transcript');
  });

  test('clearHistory clears history locally and remotely', async () => {
    useAppStore.setState({
      history: [createMockTranscript({ id: '1' })],
    });

    setMockInvokeHandler(() => undefined);

    await useAppStore.getState().clearHistory();

    expect(invoke).toHaveBeenCalledWith('clear_history');
    expect(useAppStore.getState().history).toEqual([]);
  });
});

// ============================================================================
// ENABLED TOGGLE TESTS
// ============================================================================

describe('Enabled Toggle', () => {
  test('toggleEnabled flips enabled state', async () => {
    setMockInvokeHandler((cmd) => {
      if (cmd === 'toggle_enabled') return false;
      return undefined;
    });

    await useAppStore.getState().toggleEnabled();

    expect(useAppStore.getState().enabled).toBe(false);
  });

  test('setEnabled sets specific value', async () => {
    useAppStore.setState({ enabled: true });

    setMockInvokeHandler(() => undefined);

    await useAppStore.getState().setEnabled(false);

    expect(invoke).toHaveBeenCalledWith('set_enabled', { enabled: false });
    expect(useAppStore.getState().enabled).toBe(false);
  });
});

// ============================================================================
// ASYNC ACTION COVERAGE
// ============================================================================

describe('Async Action Coverage', () => {
  test('refreshHotkeyStatus fetches and stores hotkey status', async () => {
    const status = { registered: false, error: 'not registered' };
    setMockInvokeHandler((cmd) => {
      if (cmd === 'get_hotkey_status') return status;
      return undefined;
    });

    await useAppStore.getState().refreshHotkeyStatus();
    expect(useAppStore.getState().hotkeyStatus).toEqual(status);
  });

  test('setHotkey invokes backend and refreshes hotkey status', async () => {
    const status = { registered: true, error: null };
    setMockInvokeHandler((cmd, args) => {
      if (cmd === 'set_hotkey') {
        expect(args).toEqual({ primary: 'Ctrl+Alt+Space', copyLast: 'Ctrl+Alt+C' });
        return undefined;
      }
      if (cmd === 'get_hotkey_status') return status;
      return undefined;
    });

    await useAppStore.getState().setHotkey('Ctrl+Alt+Space', 'Ctrl+Alt+C');

    expect(useAppStore.getState().hotkeyStatus).toEqual(status);
  });

  test('loadPresets fetches available presets', async () => {
    const presets = [{ id: 'default', name: 'Default' }];
    setMockInvokeHandler((cmd) => {
      if (cmd === 'get_available_presets') return presets;
      return undefined;
    });

    await useAppStore.getState().loadPresets();
    expect(useAppStore.getState().presets).toEqual(presets);
  });

  test('loadPreset returns preset rules', async () => {
    const rules = [{ id: 'r1', enabled: true, kind: 'literal', pattern: 'x', replacement: 'y', word_boundary: false, case_sensitive: false }];
    setMockInvokeHandler((cmd, args) => {
      if (cmd === 'load_preset') {
        expect(args).toEqual({ presetId: 'default' });
        return rules;
      }
      return undefined;
    });

    const result = await useAppStore.getState().loadPreset('default');
    expect(result).toEqual(rules);
  });

  test('refreshCapabilities fetches and stores capabilities', async () => {
    const capabilities = { has_microphone: true };
    setMockInvokeHandler((cmd) => {
      if (cmd === 'get_capabilities') return capabilities;
      return undefined;
    });

    await useAppStore.getState().refreshCapabilities();
    expect(useAppStore.getState().capabilities).toEqual(capabilities);
  });

  test('runSelfCheck stores backend self-check result', async () => {
    const result = { ok: true };
    setMockInvokeHandler((cmd) => {
      if (cmd === 'run_self_check') return result;
      return undefined;
    });

    await useAppStore.getState().runSelfCheck();
    expect(useAppStore.getState().selfCheckResult).toEqual(result);
  });

  test('generateDiagnostics returns diagnostics report object', async () => {
    const report: DiagnosticsReport = {
      version: '0.1.0',
      platform: 'linux',
      capabilities: {
        display_server: { type: 'x11' },
        hotkey_press_available: true,
        hotkey_release_available: true,
        keystroke_injection_available: true,
        clipboard_available: true,
        hotkey_mode: { configured: 'hold', effective: 'hold' },
        injection_method: { configured: 'clipboard_paste', effective: 'clipboard_paste' },
        permissions: { microphone: 'granted' },
      },
      config: createMockConfig(),
      self_check: {
        hotkey: { status: 'ok', message: 'ok' },
        injection: { status: 'ok', message: 'ok' },
        microphone: { status: 'ok', message: 'ok' },
        sidecar: { status: 'ok', message: 'ok' },
        model: { status: 'ok', message: 'ok' },
      },
      recent_logs: [
        {
          timestamp: '2026-02-18T00:00:00Z',
          level: 'INFO',
          target: 'app::test',
          message: 'diagnostics test log',
        },
      ],
    };

    setMockInvokeHandler((cmd) => {
      if (cmd === 'generate_diagnostics') return report;
      return undefined;
    });

    const diagnostics = await useAppStore.getState().generateDiagnostics();
    expect(diagnostics).toEqual(report);
  });

  test('getRecentLogs uses default count and returns logs', async () => {
    const logs = ['line1', 'line2'];
    setMockInvokeHandler((cmd, args) => {
      if (cmd === 'get_recent_logs') {
        expect(args).toEqual({ count: 100 });
        return logs;
      }
      return undefined;
    });

    const result = await useAppStore.getState().getRecentLogs();
    expect(result).toEqual(logs);
  });
});

// ============================================================================
// INTERNAL ACTION TESTS
// ============================================================================

describe('Internal Actions', () => {
  test('_setAppState updates state from event', () => {
    useAppStore.getState()._setAppState({
      state: 'recording',
      enabled: true,
      detail: undefined,
    });

    expect(useAppStore.getState().appState).toBe('recording');
    expect(useAppStore.getState().enabled).toBe(true);
  });

  test('_setAppState sets error detail', () => {
    useAppStore.getState()._setAppState({
      state: 'error',
      enabled: true,
      detail: 'Something went wrong',
    });

    expect(useAppStore.getState().appState).toBe('error');
    expect(useAppStore.getState().errorDetail).toBe('Something went wrong');
  });

  test('_setModelStatus updates model status', () => {
    const status = createMockModelStatus();
    useAppStore.getState()._setModelStatus(status);

    expect(useAppStore.getState().modelStatus).toEqual({
      status: status.status,
      model_id: status.model_id,
    });
  });

  test('_setModelStatus accepts legacy payload without model_id', () => {
    useAppStore.setState({ modelStatus: { status: 'ready', model_id: 'existing-model' } });
    useAppStore.getState()._setModelStatus({ status: 'loading' });

    expect(useAppStore.getState().modelStatus).toEqual({
      status: 'loading',
      model_id: 'existing-model',
    });
  });

  test('_setDownloadProgress updates progress', () => {
    useAppStore.getState()._setDownloadProgress({
      current: 50,
      total: 100,
      unit: 'bytes',
    });

    expect(useAppStore.getState().downloadProgress).toEqual({
      current: 50,
      total: 100,
      unit: 'bytes',
    });
  });

  test('_setAudioLevel updates audio level', () => {
    useAppStore.getState()._setAudioLevel({
      rms: -20,
      peak: -10,
    });

    expect(useAppStore.getState().audioLevel).toEqual({
      rms: -20,
      peak: -10,
    });
  });

  test('_addHistoryEntry prepends entry', () => {
    useAppStore.setState({
      history: [createMockTranscript({ id: 'old' })],
    });

    const newEntry = createMockTranscript({ id: 'new', text: 'New entry' });
    useAppStore.getState()._addHistoryEntry(newEntry);

    const history = useAppStore.getState().history;
    expect(history[0].id).toBe('new');
    expect(history[1].id).toBe('old');
  });

  test('_addHistoryEntry limits to configured max entries', () => {
    const config = {
      ...createMockConfig(),
      history: {
        persistence_mode: 'memory' as const,
        max_entries: 20,
        encrypt_at_rest: true,
      },
    };
    useAppStore.setState({ config });

    // Create 20 existing entries
    const existingHistory = Array.from({ length: 20 }, (_, i) =>
      createMockTranscript({ id: `entry-${i}` })
    );
    useAppStore.setState({ history: existingHistory });

    // Add one more
    const newEntry = createMockTranscript({ id: 'newest' });
    useAppStore.getState()._addHistoryEntry(newEntry);

    const history = useAppStore.getState().history;
    expect(history.length).toBe(20);
    expect(history[0].id).toBe('newest');
    expect(history[19].id).toBe('entry-18'); // entry-19 was dropped
  });

  test('_addHistoryEntry falls back to 100 entries when config is unavailable', () => {
    // Create 100 existing entries
    const existingHistory = Array.from({ length: 100 }, (_, i) =>
      createMockTranscript({ id: `entry-${i}` })
    );
    useAppStore.setState({ history: existingHistory, config: null });

    // Add one more
    const newEntry = createMockTranscript({ id: 'newest' });
    useAppStore.getState()._addHistoryEntry(newEntry);

    const history = useAppStore.getState().history;
    expect(history.length).toBe(100);
    expect(history[0].id).toBe('newest');
    expect(history[99].id).toBe('entry-98');
  });

  test('_setError updates error detail', () => {
    useAppStore.getState()._setError('Test error');

    expect(useAppStore.getState().errorDetail).toBe('Test error');
  });

  test('_setError accepts structured app:error payload', () => {
    useAppStore.getState()._setError({
      error: {
        code: 'E_INTERNAL',
        message: 'Structured app error',
        recoverable: false,
      },
    });

    expect(useAppStore.getState().errorDetail).toBe('Structured app error');
  });

  test('_setRecordingStatus updates recording state and app state', () => {
    useAppStore.getState()._setRecordingStatus({
      phase: 'recording',
      session_id: 'session-1',
    });

    expect(useAppStore.getState().recordingStatus).toEqual({
      phase: 'recording',
      session_id: 'session-1',
    });
    expect(useAppStore.getState().appState).toBe('recording');
  });

  test('_setSidecarStatus updates sidecar status slice', () => {
    useAppStore.getState()._setSidecarStatus({
      state: 'ready',
      restart_count: 0,
    });

    expect(useAppStore.getState().sidecarStatus).toEqual({
      state: 'ready',
      restart_count: 0,
    });
  });

  test('_setTranscriptError stores payload and error detail', () => {
    useAppStore.getState()._setTranscriptError({
      session_id: 'session-1',
      error: {
        code: 'E_TRANSCRIPTION_FAILED',
        message: 'Transcript failed',
        recoverable: true,
      },
    });

    expect(useAppStore.getState().lastTranscriptError).toEqual({
      session_id: 'session-1',
      error: {
        code: 'E_TRANSCRIPTION_FAILED',
        message: 'Transcript failed',
        recoverable: true,
      },
    });
    expect(useAppStore.getState().errorDetail).toBe('Transcript failed');
  });
});

// ============================================================================
// INITIALIZATION TESTS
// ============================================================================

describe('Initialization', () => {
  test('initialize loads all data in parallel', async () => {
    const mockConfig = createMockConfig();
    const mockDevices = [createMockDevice()];
    const mockCapabilities = { has_microphone: true };
    const mockModelStatus = createMockModelStatus();
    const mockHistory = [createMockTranscript()];
    const mockHotkeyStatus = { registered: true, error: null };
    const mockPresets = [{ id: 'default', name: 'Default' }];
    const mockStateEvent = { state: 'idle', enabled: true };

    setMockInvokeHandler((cmd) => {
      switch (cmd) {
        case 'get_config':
          return mockConfig;
        case 'list_audio_devices':
          return mockDevices;
        case 'get_capabilities':
          return mockCapabilities;
        case 'get_model_status':
          return mockModelStatus;
        case 'get_transcript_history':
          return mockHistory;
        case 'get_hotkey_status':
          return mockHotkeyStatus;
        case 'get_available_presets':
          return mockPresets;
        case 'get_app_state':
          return mockStateEvent;
        default:
          return undefined;
      }
    });

    await useAppStore.getState().initialize();

    expect(useAppStore.getState().isInitialized).toBe(true);
    expect(useAppStore.getState().isLoading).toBe(false);
    expect(useAppStore.getState().config).toEqual(mockConfig);
    expect(useAppStore.getState().devices).toEqual(mockDevices);
  });

  test('initialize only runs once', async () => {
    useAppStore.setState({ isInitialized: true });

    setMockInvokeHandler(() => {
      throw new Error('Should not be called');
    });

    await useAppStore.getState().initialize();

    // Should not throw because it skips when already initialized
    expect(useAppStore.getState().isInitialized).toBe(true);
  });

  test('initialize handles errors gracefully', async () => {
    setMockInvokeHandler(() => {
      throw new Error('Network error');
    });

    await useAppStore.getState().initialize();

    expect(useAppStore.getState().isLoading).toBe(false);
    expect(useAppStore.getState().errorDetail).toBe('Error: Network error');
  });
});
