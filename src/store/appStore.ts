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
  UiConfig,
  ModelStatus,
  ModelState,
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
  stateTimestamp?: string;
  errorRecoveryActions: string[];

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
  sidecarRecoveryNeeded: boolean;
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
  startRecording: () => Promise<void>;
  stopRecording: () => Promise<void>;
  cancelRecording: () => Promise<void>;

  // Config actions
  loadConfig: () => Promise<void>;
  updateAudioConfig: (config: Partial<AudioConfig>) => Promise<void>;
  updateHotkeyConfig: (config: Partial<HotkeyConfig>) => Promise<void>;
  updateInjectionConfig: (config: Partial<InjectionConfig>) => Promise<void>;
  updateUiConfig: (config: Partial<UiConfig>) => Promise<void>;
  setReplacementRules: (rules: ReplacementRule[]) => Promise<void>;
  resetConfig: () => Promise<void>;

  // Model actions
  refreshModelStatus: () => Promise<void>;
  downloadModel: () => Promise<void>;
  purgeModelCache: (modelId?: string) => Promise<void>;

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
  restartSidecar: () => Promise<void>;

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
  stateTimestamp: undefined,
  errorRecoveryActions: [],
  modelStatus: null,
  downloadProgress: null,
  devices: [],
  selectedDeviceUid: null,
  audioLevel: null,
  isMeterRunning: false,
  history: [],
  recordingStatus: null,
  sidecarStatus: null,
  sidecarRecoveryNeeded: false,
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
  const normalizeModelState = (value: unknown): ModelState => {
    if (typeof value !== 'string') {
      return 'unknown';
    }

    const normalized = value.toLowerCase();
    if (normalized === 'installing') return 'loading';
    if (normalized === 'available') return 'missing';
    if (
      normalized === 'missing'
      || normalized === 'downloading'
      || normalized === 'loading'
      || normalized === 'verifying'
      || normalized === 'ready'
      || normalized === 'error'
      || normalized === 'unknown'
    ) {
      return normalized;
    }

    return 'unknown';
  };

  const next: ModelStatus = {
    status: normalizeModelState(payload.status),
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

function deriveRecoveryActions(
  payload: string | ErrorEvent | TranscriptErrorEvent,
): string[] {
  if (typeof payload === 'string') return [];
  const appError =
    (payload.app_error && typeof payload.app_error === 'object' ? payload.app_error : null)
    ?? (payload.error && typeof payload.error === 'object' ? payload.error : null);
  const recoverable = appError?.recoverable ?? ('recoverable' in payload && payload.recoverable);
  if (!recoverable) return [];
  const code = appError?.code;
  if (code === 'E_MIC_PERMISSION') return ['Check microphone permissions in system settings'];
  if (code === 'E_DEVICE_NOT_FOUND') return ['Reconnect the audio device or select a different one'];
  if (code === 'E_NETWORK') return ['Check your internet connection and retry'];
  return ['Retry the operation'];
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

  startRecording: async () => {
    try {
      await invoke('start_recording');
    } catch (error) {
      console.error('Failed to start recording:', error);
      set({
        appState: 'error',
        errorDetail: error instanceof Error ? error.message : String(error),
      });
      throw error;
    }
  },

  stopRecording: async () => {
    try {
      await invoke('stop_recording');
    } catch (error) {
      console.error('Failed to stop recording:', error);
      set({
        appState: 'error',
        errorDetail: error instanceof Error ? error.message : String(error),
      });
      throw error;
    }
  },

  cancelRecording: async () => {
    try {
      await invoke('cancel_recording');
    } catch (error) {
      console.error('Failed to cancel recording:', error);
      set({
        appState: 'error',
        errorDetail: error instanceof Error ? error.message : String(error),
      });
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

  updateUiConfig: async (uiConfig) => {
    const config = get().config;
    if (!config) return;

    const newConfig = {
      ...config,
      ui: { ...config.ui, ...uiConfig },
    };

    try {
      await invoke('update_config', { config: newConfig });
      set({ config: newConfig });
    } catch (error) {
      console.error('Failed to update UI config:', error);
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

  purgeModelCache: async (modelId?: string) => {
    try {
      const trimmedModelId = typeof modelId === 'string' ? modelId.trim() : '';
      if (trimmedModelId.length > 0) {
        await invoke('purge_model_cache', { modelId: trimmedModelId });
      } else {
        await invoke('purge_model_cache');
      }
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

  restartSidecar: async () => {
    try {
      await invoke('restart_sidecar');
    } catch (error) {
      console.error('Failed to restart sidecar:', error);
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
      stateTimestamp: 'timestamp' in event ? event.timestamp : undefined,
    });
  },

  _setModelStatus: (status) => {
    set((state) => {
      // Ignore model:status events for non-configured models (e.g. purge of model B
      // should not overwrite UI status when configured model A is active).
      const configuredModelId = state.config?.model?.model_id;
      if (
        configuredModelId
        && 'model_id' in status
        && typeof status.model_id === 'string'
        && status.model_id.length > 0
        && status.model_id !== configuredModelId
      ) {
        return state;
      }

      const nextStatus = normalizeModelStatusPayload(status, state.modelStatus);
      const isInstalling =
        nextStatus.status === 'loading'
        || nextStatus.status === 'downloading'
        || nextStatus.status === 'verifying';

      return {
        modelStatus: nextStatus,
        downloadProgress: isInstalling ? (nextStatus.progress ?? state.downloadProgress) : null,
      };
    });
  },

  _setDownloadProgress: (progress) => {
    set((state) => {
      const currentStatus = state.modelStatus;
      const isInstalling = currentStatus
        ? (
            currentStatus.status === 'loading'
            || currentStatus.status === 'downloading'
            || currentStatus.status === 'verifying'
          )
        : false;

      if (!currentStatus || !isInstalling) {
        return { downloadProgress: progress };
      }

      return {
        downloadProgress: progress,
        modelStatus: {
          ...currentStatus,
          progress: progress ?? undefined,
        },
      };
    });
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
    set({
      sidecarStatus: status,
      sidecarRecoveryNeeded: status.state === 'failed' || status.state === 'restarting',
    });
  },

  _setTranscriptError: (payload) => {
    set({
      lastTranscriptError: payload,
      errorDetail: normalizeErrorMessage(payload),
      errorRecoveryActions: deriveRecoveryActions(payload),
    });
  },

  _setError: (payload) => {
    set({
      errorDetail: normalizeErrorMessage(payload),
      errorRecoveryActions: deriveRecoveryActions(payload),
    });
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
export const selectStateTimestamp = (state: AppStore) => state.stateTimestamp;
export const selectErrorRecoveryActions = (state: AppStore) => state.errorRecoveryActions;
export const selectSidecarRecoveryNeeded = (state: AppStore) => state.sidecarRecoveryNeeded;
export const selectConfig = (state: AppStore) => state.config;
export const selectCapabilities = (state: AppStore) => state.capabilities;
export const selectReplacementBadgeCount = (state: AppStore) => {
  const replacements = state.config?.replacements ?? [];
  const enabledRules = replacements.filter((rule) => rule.enabled).length;
  const derivedEnabledPresetIds = new Set(
    replacements
      .map((rule) => rule.origin)
      .filter((origin): origin is `preset:${string}` => typeof origin === 'string' && origin.startsWith('preset:'))
      .map((origin) => origin.slice('preset:'.length))
  );
  const configuredPresetCount = state.config?.presets.enabled_presets.length ?? 0;
  const enabledPresets = Math.max(configuredPresetCount, derivedEnabledPresetIds.size);

  return enabledRules + enabledPresets;
};
