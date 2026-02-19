/**
 * Zustand store for application state management.
 *
 * This store serves as the single source of truth for UI state,
 * syncing with the Rust backend via Tauri commands and events.
 */

import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import type {
  AppState,
  AppError,
  AppConfig,
  AudioConfig,
  AudioDevice,
  AudioLevelEvent,
  Capabilities,
  ErrorEvent,
  HotkeyConfig,
  HotkeyStatus,
  InjectionConfig,
  ModelStatus,
  ModelStatusPayload,
  PresetInfo,
  Progress,
  RecordingStatusEvent,
  ReplacementRule,
  DiagnosticsReport,
  SidecarStatusEvent,
  SelfCheckResult,
  StateEventPayload,
  TranscriptErrorEvent,
  TranscriptEntry,
} from '../types';

// ============================================================================
// STORE STATE INTERFACE
// ============================================================================

export interface AppStoreState {
  // Application state
  appState: AppState;
  enabled: boolean;
  errorDetail?: string;

  // Model state
  modelStatus: ModelStatus | null;
  downloadProgress: Progress | null;

  // Audio devices
  devices: AudioDevice[];
  selectedDeviceUid: string | null;
  audioLevel: AudioLevelEvent | null;
  isMeterRunning: boolean;

  // Transcript history
  history: TranscriptEntry[];
  recordingStatus: RecordingStatusEvent | null;
  sidecarStatus: SidecarStatusEvent | null;
  lastTranscriptError: TranscriptErrorEvent | null;

  // Configuration (mirrors Rust config)
  config: AppConfig | null;

  // Capabilities
  capabilities: Capabilities | null;

  // Hotkey status
  hotkeyStatus: HotkeyStatus | null;

  // Available presets
  presets: PresetInfo[];

  // Self-check results
  selfCheckResult: SelfCheckResult | null;

  // UI state
  isInitialized: boolean;
  isLoading: boolean;
}

// ============================================================================
// STORE ACTIONS INTERFACE
// ============================================================================

export interface AppStoreActions {
  // Initialization
  initialize: () => Promise<void>;

  // Device actions
  refreshDevices: () => Promise<void>;
  selectDevice: (uid: string | null) => Promise<void>;
  startMicTest: () => Promise<void>;
  stopMicTest: () => Promise<void>;

  // Config actions
  loadConfig: () => Promise<void>;
  updateAudioConfig: (config: Partial<AudioConfig>) => Promise<void>;
  updateHotkeyConfig: (config: Partial<HotkeyConfig>) => Promise<void>;
  updateInjectionConfig: (config: Partial<InjectionConfig>) => Promise<void>;
  setReplacementRules: (rules: ReplacementRule[]) => Promise<void>;
  resetConfig: () => Promise<void>;

  // Model actions
  refreshModelStatus: () => Promise<void>;
  downloadModel: () => Promise<void>;
  purgeModelCache: () => Promise<void>;

  // History actions
  refreshHistory: () => Promise<void>;
  copyTranscript: (id: string) => Promise<void>;
  copyLastTranscript: () => Promise<void>;
  clearHistory: () => Promise<void>;

  // Hotkey actions
  refreshHotkeyStatus: () => Promise<void>;
  setHotkey: (primary: string, copyLast: string) => Promise<void>;

  // Preset actions
  loadPresets: () => Promise<void>;
  loadPreset: (presetId: string) => Promise<ReplacementRule[]>;

  // Capabilities actions
  refreshCapabilities: () => Promise<void>;

  // Self-check actions
  runSelfCheck: () => Promise<void>;

  // Diagnostics
  generateDiagnostics: () => Promise<DiagnosticsReport>;
  getRecentLogs: (count?: number) => Promise<string[]>;

  // Toggle enabled
  toggleEnabled: () => Promise<void>;
  setEnabled: (enabled: boolean) => Promise<void>;

  // Internal actions (called by event handlers)
  _setAppState: (event: StateEventPayload) => void;
  _setModelStatus: (status: ModelStatusPayload) => void;
  _setDownloadProgress: (progress: Progress | null) => void;
  _setAudioLevel: (level: AudioLevelEvent | null) => void;
  _addHistoryEntry: (entry: TranscriptEntry) => void;
  _setRecordingStatus: (status: RecordingStatusEvent) => void;
  _setSidecarStatus: (status: SidecarStatusEvent) => void;
  _setTranscriptError: (payload: TranscriptErrorEvent) => void;
  _setError: (payload: string | ErrorEvent | TranscriptErrorEvent) => void;
}

// ============================================================================
// COMBINED STORE TYPE
// ============================================================================

export type AppStore = AppStoreState & AppStoreActions;

// ============================================================================
// DEFAULT STATE
// ============================================================================

const defaultState: AppStoreState = {
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
};

function stateDetailFromPayload(payload: StateEventPayload): string | undefined {
  if ('detail' in payload && typeof payload.detail === 'string' && payload.detail.length > 0) {
    return payload.detail;
  }
  if (
    'error_detail' in payload
    && typeof payload.error_detail === 'string'
    && payload.error_detail.length > 0
  ) {
    return payload.error_detail;
  }
  return undefined;
}

function normalizeModelStatusPayload(
  payload: ModelStatusPayload,
  current: ModelStatus | null
): ModelStatus {
  const next: ModelStatus = {
    status: payload.status,
  };

  if ('seq' in payload && typeof payload.seq === 'number') {
    next.seq = payload.seq;
  }

  if ('model_id' in payload && typeof payload.model_id === 'string' && payload.model_id.length > 0) {
    next.model_id = payload.model_id;
  } else if (current?.model_id) {
    next.model_id = current.model_id;
  }

  if ('revision' in payload && typeof payload.revision === 'string' && payload.revision.length > 0) {
    next.revision = payload.revision;
  } else if (current?.revision) {
    next.revision = current.revision;
  }

  if ('cache_path' in payload && typeof payload.cache_path === 'string' && payload.cache_path.length > 0) {
    next.cache_path = payload.cache_path;
  } else if (current?.cache_path) {
    next.cache_path = current.cache_path;
  }

  if ('progress' in payload) {
    next.progress = payload.progress;
  } else if (current?.progress) {
    next.progress = current.progress;
  }

  if ('error' in payload && typeof payload.error === 'string' && payload.error.length > 0) {
    next.error = payload.error;
  } else if (payload.status !== 'error' && current?.error) {
    next.error = undefined;
  } else if (current?.error) {
    next.error = current.error;
  }

  return next;
}

function normalizeTranscriptEntry(entry: TranscriptEntry): TranscriptEntry {
  const finalText =
    typeof entry.final_text === 'string' && entry.final_text.length > 0
      ? entry.final_text
      : entry.text;
  const rawText =
    typeof entry.raw_text === 'string' && entry.raw_text.length > 0
      ? entry.raw_text
      : entry.text;

  return {
    ...entry,
    text: finalText,
    raw_text: rawText,
    final_text: finalText,
  };
}

function messageFromAppError(error: AppError): string {
  return typeof error.message === 'string' && error.message.length > 0
    ? error.message
    : 'Unknown error';
}

function normalizeErrorMessage(payload: string | ErrorEvent | TranscriptErrorEvent): string {
  if (typeof payload === 'string') {
    return payload;
  }
  if (typeof payload.message === 'string' && payload.message.length > 0) {
    return payload.message;
  }
  if (typeof payload.error === 'string' && payload.error.length > 0) {
    return payload.error;
  }
  if (payload.error && typeof payload.error === 'object') {
    return messageFromAppError(payload.error);
  }
  if (payload.app_error && typeof payload.app_error === 'object') {
    return messageFromAppError(payload.app_error);
  }
  return 'Unknown error';
}

function appStateFromRecordingPhase(phase: string): AppState | null {
  if (phase === 'recording' || phase === 'transcribing' || phase === 'idle') {
    return phase;
  }
  if (phase === 'loading_model' || phase === 'error') {
    return phase;
  }
  return null;
}

// ============================================================================
// STORE IMPLEMENTATION
// ============================================================================

export const useAppStore = create<AppStore>((set, get) => ({
  ...defaultState,

  // --------------------------------------------------------------------------
  // INITIALIZATION
  // --------------------------------------------------------------------------

  initialize: async () => {
    if (get().isInitialized) return;

    set({ isLoading: true });

    try {
      // Load initial state in parallel
      await Promise.all([
        get().loadConfig(),
        get().refreshDevices(),
        get().refreshCapabilities(),
        get().refreshModelStatus(),
        get().refreshHistory(),
        get().refreshHotkeyStatus(),
        get().loadPresets(),
      ]);

      // Get current app state
      const stateEvent = await invoke<StateEventPayload>('get_app_state');
      set({
        appState: stateEvent.state,
        enabled: stateEvent.enabled,
        errorDetail: stateDetailFromPayload(stateEvent),
      });

      set({ isInitialized: true });
    } catch (error) {
      console.error('Failed to initialize app store:', error);
      set({ errorDetail: String(error) });
    } finally {
      set({ isLoading: false });
    }
  },

  // --------------------------------------------------------------------------
  // DEVICE ACTIONS
  // --------------------------------------------------------------------------

  refreshDevices: async () => {
    try {
      const devices = await invoke<AudioDevice[]>('list_audio_devices');
      const config = get().config;
      set({
        devices,
        selectedDeviceUid: config?.audio.device_uid ?? null,
      });
    } catch (error) {
      console.error('Failed to refresh devices:', error);
    }
  },

  selectDevice: async (uid) => {
    try {
      await invoke('set_audio_device', { deviceUid: uid });
      set({ selectedDeviceUid: uid });

      // Update local config
      const config = get().config;
      if (config) {
        set({
          config: {
            ...config,
            audio: { ...config.audio, device_uid: uid ?? undefined },
          },
        });
      }
    } catch (error) {
      console.error('Failed to select device:', error);
      throw error;
    }
  },

  startMicTest: async () => {
    try {
      await invoke('start_mic_test');
      set({ isMeterRunning: true, audioLevel: null });
    } catch (error) {
      console.error('Failed to start mic test:', error);
      throw error;
    }
  },

  stopMicTest: async () => {
    try {
      await invoke('stop_mic_test');
      set({ isMeterRunning: false, audioLevel: null });
    } catch (error) {
      console.error('Failed to stop mic test:', error);
      throw error;
    }
  },

  // --------------------------------------------------------------------------
  // CONFIG ACTIONS
  // --------------------------------------------------------------------------

  loadConfig: async () => {
    try {
      const config = await invoke<AppConfig>('get_config');
      set({
        config,
        selectedDeviceUid: config.audio.device_uid ?? null,
      });
    } catch (error) {
      console.error('Failed to load config:', error);
    }
  },

  updateAudioConfig: async (audioConfig) => {
    const config = get().config;
    if (!config) return;

    const isDeviceChange = Object.prototype.hasOwnProperty.call(audioConfig, 'device_uid')
      && audioConfig.device_uid !== config.audio.device_uid;
    const shouldRestartMeter = isDeviceChange && get().isMeterRunning;

    const newConfig = {
      ...config,
      audio: { ...config.audio, ...audioConfig },
    };

    let meterStoppedForDeviceSwitch = false;
    let configPersisted = false;

    try {
      if (shouldRestartMeter) {
        await get().stopMicTest();
        meterStoppedForDeviceSwitch = true;
      }

      await invoke('update_config', { config: newConfig });
      configPersisted = true;
      set({
        config: newConfig,
        selectedDeviceUid: newConfig.audio.device_uid ?? null,
      });

      if (meterStoppedForDeviceSwitch) {
        await get().startMicTest();
      }
    } catch (error) {
      console.error('Failed to update audio config:', error);
      if (meterStoppedForDeviceSwitch && !configPersisted) {
        try {
          await get().startMicTest();
        } catch (restartError) {
          console.error('Failed to restart mic test after config update failure:', restartError);
        }
      }
      throw error;
    }
  },

  updateHotkeyConfig: async (hotkeyConfig) => {
    const config = get().config;
    if (!config) return;

    const newConfig = {
      ...config,
      hotkeys: { ...config.hotkeys, ...hotkeyConfig },
    };

    try {
      await invoke('update_config', { config: newConfig });
      set({ config: newConfig });
      await get().refreshHotkeyStatus();
    } catch (error) {
      console.error('Failed to update hotkey config:', error);
      throw error;
    }
  },

  updateInjectionConfig: async (injectionConfig) => {
    const config = get().config;
    if (!config) return;

    const newConfig = {
      ...config,
      injection: { ...config.injection, ...injectionConfig },
    };

    try {
      await invoke('update_config', { config: newConfig });
      set({ config: newConfig });
    } catch (error) {
      console.error('Failed to update injection config:', error);
      throw error;
    }
  },

  setReplacementRules: async (rules) => {
    try {
      await invoke('set_replacement_rules', { rules });
      const config = get().config;
      if (config) {
        set({ config: { ...config, replacements: rules } });
      }
    } catch (error) {
      console.error('Failed to set replacement rules:', error);
      throw error;
    }
  },

  resetConfig: async () => {
    try {
      await invoke('reset_config_to_defaults');
      await get().loadConfig();
    } catch (error) {
      console.error('Failed to reset config:', error);
      throw error;
    }
  },

  // --------------------------------------------------------------------------
  // MODEL ACTIONS
  // --------------------------------------------------------------------------

  refreshModelStatus: async () => {
    try {
      const status = await invoke<ModelStatus>('get_model_status');
      set({ modelStatus: status });
    } catch (error) {
      console.error('Failed to refresh model status:', error);
    }
  },

  downloadModel: async () => {
    try {
      set({ downloadProgress: { current: 0, total: undefined, unit: 'bytes' } });
      await invoke('download_model');
      await get().refreshModelStatus();
    } catch (error) {
      console.error('Failed to download model:', error);
      throw error;
    } finally {
      set({ downloadProgress: null });
    }
  },

  purgeModelCache: async () => {
    try {
      await invoke('purge_model_cache');
      await get().refreshModelStatus();
    } catch (error) {
      console.error('Failed to purge model cache:', error);
      throw error;
    }
  },

  // --------------------------------------------------------------------------
  // HISTORY ACTIONS
  // --------------------------------------------------------------------------

  refreshHistory: async () => {
    try {
      const history = await invoke<TranscriptEntry[]>('get_transcript_history');
      set({ history: history.map((entry) => normalizeTranscriptEntry(entry)) });
    } catch (error) {
      console.error('Failed to refresh history:', error);
    }
  },

  copyTranscript: async (id) => {
    try {
      await invoke('copy_transcript', { id });
    } catch (error) {
      console.error('Failed to copy transcript:', error);
      throw error;
    }
  },

  copyLastTranscript: async () => {
    try {
      await invoke('copy_last_transcript');
    } catch (error) {
      console.error('Failed to copy last transcript:', error);
      throw error;
    }
  },

  clearHistory: async () => {
    try {
      await invoke('clear_history');
      set({ history: [] });
    } catch (error) {
      console.error('Failed to clear history:', error);
      throw error;
    }
  },

  // --------------------------------------------------------------------------
  // HOTKEY ACTIONS
  // --------------------------------------------------------------------------

  refreshHotkeyStatus: async () => {
    try {
      const status = await invoke<HotkeyStatus>('get_hotkey_status');
      set({ hotkeyStatus: status });
    } catch (error) {
      console.error('Failed to refresh hotkey status:', error);
    }
  },

  setHotkey: async (primary, copyLast) => {
    try {
      await invoke('set_hotkey', { primary, copyLast });
      await get().refreshHotkeyStatus();
    } catch (error) {
      console.error('Failed to set hotkey:', error);
      throw error;
    }
  },

  // --------------------------------------------------------------------------
  // PRESET ACTIONS
  // --------------------------------------------------------------------------

  loadPresets: async () => {
    try {
      const presets = await invoke<PresetInfo[]>('get_available_presets');
      set({ presets });
    } catch (error) {
      console.error('Failed to load presets:', error);
    }
  },

  loadPreset: async (presetId) => {
    try {
      const rules = await invoke<ReplacementRule[]>('load_preset', { presetId });
      return rules;
    } catch (error) {
      console.error('Failed to load preset:', error);
      throw error;
    }
  },

  // --------------------------------------------------------------------------
  // CAPABILITIES ACTIONS
  // --------------------------------------------------------------------------

  refreshCapabilities: async () => {
    try {
      const capabilities = await invoke<Capabilities>('get_capabilities');
      set({ capabilities });
    } catch (error) {
      console.error('Failed to refresh capabilities:', error);
    }
  },

  // --------------------------------------------------------------------------
  // SELF-CHECK ACTIONS
  // --------------------------------------------------------------------------

  runSelfCheck: async () => {
    try {
      const result = await invoke<SelfCheckResult>('run_self_check');
      set({ selfCheckResult: result });
    } catch (error) {
      console.error('Failed to run self-check:', error);
      throw error;
    }
  },

  // --------------------------------------------------------------------------
  // DIAGNOSTICS ACTIONS
  // --------------------------------------------------------------------------

  generateDiagnostics: async () => {
    try {
      const report = await invoke<DiagnosticsReport>('generate_diagnostics');
      return report;
    } catch (error) {
      console.error('Failed to generate diagnostics:', error);
      throw error;
    }
  },

  getRecentLogs: async (count = 100) => {
    try {
      const logs = await invoke<string[]>('get_recent_logs', { count });
      return logs;
    } catch (error) {
      console.error('Failed to get recent logs:', error);
      throw error;
    }
  },

  // --------------------------------------------------------------------------
  // TOGGLE ENABLED
  // --------------------------------------------------------------------------

  toggleEnabled: async () => {
    try {
      const enabled = await invoke<boolean>('toggle_enabled');
      set({ enabled });
    } catch (error) {
      console.error('Failed to toggle enabled:', error);
      throw error;
    }
  },

  setEnabled: async (enabled) => {
    try {
      await invoke('set_enabled', { enabled });
      set({ enabled });
    } catch (error) {
      console.error('Failed to set enabled:', error);
      throw error;
    }
  },

  // --------------------------------------------------------------------------
  // INTERNAL ACTIONS (called by event handlers)
  // --------------------------------------------------------------------------

  _setAppState: (event) => {
    set({
      appState: event.state,
      enabled: event.enabled,
      errorDetail: stateDetailFromPayload(event),
    });
  },

  _setModelStatus: (status) => {
    set((state) => ({
      modelStatus: normalizeModelStatusPayload(status, state.modelStatus),
    }));
  },

  _setDownloadProgress: (progress) => {
    set({ downloadProgress: progress });
  },

  _setAudioLevel: (level) => {
    set({ audioLevel: level });
  },

  _addHistoryEntry: (entry) => {
    const fallbackHistoryLimit = 100;
    const normalizedEntry = normalizeTranscriptEntry(entry);
    set((state) => ({
      // Keep frontend list aligned with backend-configured history capacity.
      history: [normalizedEntry, ...state.history].slice(
        0,
        Math.max(1, state.config?.history.max_entries ?? fallbackHistoryLimit)
      ),
    }));
  },

  _setRecordingStatus: (status) => {
    set((state) => {
      const nextAppState = appStateFromRecordingPhase(status.phase);
      return {
        recordingStatus: status,
        appState: nextAppState ?? state.appState,
      };
    });
  },

  _setSidecarStatus: (status) => {
    set({ sidecarStatus: status });
  },

  _setTranscriptError: (payload) => {
    set({
      lastTranscriptError: payload,
      errorDetail: normalizeErrorMessage(payload),
    });
  },

  _setError: (payload) => {
    set({ errorDetail: normalizeErrorMessage(payload) });
  },
}));

// ============================================================================
// SELECTORS (for optimized re-renders)
// ============================================================================

export const selectAppState = (state: AppStore) => state.appState;
export const selectIsRecording = (state: AppStore) => state.appState === 'recording';
export const selectIsTranscribing = (state: AppStore) => state.appState === 'transcribing';
export const selectIsIdle = (state: AppStore) => state.appState === 'idle';
export const selectModelReady = (state: AppStore) => state.modelStatus?.status === 'ready';
export const selectDevices = (state: AppStore) => state.devices;
export const selectHistory = (state: AppStore) => state.history;
export const selectConfig = (state: AppStore) => state.config;
export const selectCapabilities = (state: AppStore) => state.capabilities;
