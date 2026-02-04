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
// INTERNAL ACTION TESTS
// ============================================================================

describe('Internal Actions', () => {
  test('_setAppState updates state from event', () => {
    useAppStore.getState()._setAppState({
      state: 'recording',
      enabled: true,
      error_detail: undefined,
    });

    expect(useAppStore.getState().appState).toBe('recording');
    expect(useAppStore.getState().enabled).toBe(true);
  });

  test('_setAppState sets error detail', () => {
    useAppStore.getState()._setAppState({
      state: 'error',
      enabled: true,
      error_detail: 'Something went wrong',
    });

    expect(useAppStore.getState().appState).toBe('error');
    expect(useAppStore.getState().errorDetail).toBe('Something went wrong');
  });

  test('_setModelStatus updates model status', () => {
    const status = createMockModelStatus();
    useAppStore.getState()._setModelStatus(status);

    expect(useAppStore.getState().modelStatus).toEqual(status);
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

  test('_addHistoryEntry limits to 100 entries', () => {
    // Create 100 existing entries
    const existingHistory = Array.from({ length: 100 }, (_, i) =>
      createMockTranscript({ id: `entry-${i}` })
    );
    useAppStore.setState({ history: existingHistory });

    // Add one more
    const newEntry = createMockTranscript({ id: 'newest' });
    useAppStore.getState()._addHistoryEntry(newEntry);

    const history = useAppStore.getState().history;
    expect(history.length).toBe(100);
    expect(history[0].id).toBe('newest');
    expect(history[99].id).toBe('entry-98'); // entry-99 was dropped
  });

  test('_setError updates error detail', () => {
    useAppStore.getState()._setError('Test error');

    expect(useAppStore.getState().errorDetail).toBe('Test error');
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
