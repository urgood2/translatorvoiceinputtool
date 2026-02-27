import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it } from 'vitest';

import App from '../App';
import { useAppStore } from '../store/appStore';
import { setMockInvokeHandler } from './setup';

type AppConfigLike = Record<string, any>;

function resetAppStoreState() {
  useAppStore.setState({
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
  });
}

function makeConfig(overrides?: Partial<AppConfigLike>): AppConfigLike {
  return {
    schema_version: 1,
    audio: {
      device_uid: null,
      audio_cues_enabled: true,
      trim_silence: true,
      vad_enabled: false,
      vad_silence_ms: 1200,
      vad_min_speech_ms: 250,
    },
    hotkeys: {
      primary: 'Ctrl+Shift+Space',
      copy_last: 'Ctrl+Shift+C',
      mode: 'hold',
    },
    injection: {
      paste_delay_ms: 40,
      restore_clipboard: true,
      suffix: ' ',
      focus_guard_enabled: true,
    },
    model: {
      model_id: 'nvidia/parakeet-tdt-0.6b-v2',
      device: 'cuda',
      preferred_device: 'gpu',
      language: 'de',
    },
    replacements: [],
    ui: {
      show_on_startup: false,
      window_width: 800,
      window_height: 600,
      theme: 'dark',
      onboarding_completed: true,
      overlay_enabled: true,
      locale: 'de-DE',
      reduce_motion: true,
    },
    history: {
      persistence_mode: 'memory',
      max_entries: 100,
      encrypt_at_rest: true,
    },
    presets: {
      enabled_presets: [],
    },
    ...overrides,
  };
}

function installAppHandler(config: AppConfigLike, history: any[] = []) {
  setMockInvokeHandler((cmd) => {
    switch (cmd) {
      case 'get_config':
        return config;
      case 'list_audio_devices':
        return [];
      case 'get_capabilities':
        return {
          display_server: { type: 'x11' },
          hotkey_press_available: true,
          hotkey_release_available: true,
          keystroke_injection_available: true,
          clipboard_available: true,
          hotkey_mode: { configured: 'hold', effective: 'hold' },
          injection_method: { configured: 'clipboard_paste', effective: 'clipboard_paste' },
          permissions: { microphone: 'granted' },
        };
      case 'get_model_status':
        return { status: 'ready', model_id: 'nvidia/parakeet-tdt-0.6b-v2' };
      case 'get_transcript_history':
        return history;
      case 'get_hotkey_status':
        return {
          primary: 'Ctrl+Shift+Space',
          copy_last: 'Ctrl+Shift+C',
          mode: 'hold',
          registered: true,
        };
      case 'copy_transcript':
        return undefined;
      case 'get_available_presets':
        return [
          {
            id: 'medical',
            name: 'Medical Dictation',
            description: 'Common clinical terms',
            rule_count: 1,
          },
        ];
      case 'load_preset':
        return [
          {
            id: 'med-1',
            enabled: true,
            kind: 'literal',
            pattern: 'BP',
            replacement: 'blood pressure',
            word_boundary: true,
            case_sensitive: false,
            origin: 'preset',
          },
        ];
      case 'get_app_state':
        return { state: 'idle', enabled: true, detail: undefined };
      case 'run_self_check':
        return {
          hotkey: { status: 'ok', message: 'Registered and working' },
          injection: { status: 'ok', message: 'Clipboard paste available' },
          microphone: { status: 'ok', message: 'Permission granted' },
          sidecar: { status: 'ok', message: 'Connected and responsive' },
          model: { status: 'ok', message: 'Ready' },
        };
      case 'generate_diagnostics':
        return {
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
          config,
          self_check: {
            hotkey: { status: 'ok', message: 'Registered and working' },
            injection: { status: 'ok', message: 'Clipboard paste available' },
            microphone: { status: 'ok', message: 'Permission granted' },
            sidecar: { status: 'ok', message: 'Connected and responsive' },
            model: { status: 'ok', message: 'Ready' },
          },
          recent_logs: [],
        };
      default:
        return undefined;
    }
  });
}

describe('App diagnostics panels', () => {
  beforeEach(() => {
    resetAppStoreState();
    installAppHandler(
      makeConfig(),
      [
        {
          id: 'entry-1',
          text: 'Sample transcript text.',
          raw_text: 'Sample transcript text.',
          final_text: 'Sample transcript text.',
          timestamp: new Date().toISOString(),
          audio_duration_ms: 2400,
          transcription_duration_ms: 500,
          injection_result: { status: 'injected' },
        },
      ]
    );
  });

  it('mounts SelfCheck and Diagnostics flows in the main app', async () => {
    render(<App />);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('run_self_check');
    });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('generate_diagnostics');
    });

    expect(screen.getByRole('tab', { name: 'Status' })).toBeDefined();
    expect(screen.getByRole('tab', { name: 'History' })).toBeDefined();
    expect(screen.getByRole('tab', { name: 'Replacements' })).toBeDefined();
    expect(screen.getByRole('tab', { name: 'Settings' })).toBeDefined();
    expect(screen.getAllByText('Ready').length).toBeGreaterThan(0);

    fireEvent.click(screen.getByRole('tab', { name: 'History' }));
    expect(screen.getAllByText('Sample transcript text.').length).toBeGreaterThan(0);
    expect(screen.getByText('Injected')).toBeDefined();

    fireEvent.click(screen.getByText('Copy'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('copy_transcript', { id: 'entry-1' });
    });

    fireEvent.click(screen.getByRole('tab', { name: 'Settings' }));
    expect(screen.getByText('Settings')).toBeDefined();
    expect(screen.getByRole('tab', { name: 'Hotkeys' })).toBeDefined();
    expect(screen.getByRole('tab', { name: 'Injection' })).toBeDefined();
    expect(screen.getByText('Microphone Test')).toBeDefined();
    expect(screen.getByRole('button', { name: 'Start Test' })).toBeDefined();
    expect(screen.getByText('System Health Check')).toBeDefined();
    expect(screen.getAllByText('Diagnostics').length).toBeGreaterThan(0);
  });

  it('defaults to status tab and switches panel content on tab click', async () => {
    render(<App />);

    const statusTab = await screen.findByRole('tab', { name: 'Status' });
    const historyTab = screen.getByRole('tab', { name: 'History' });
    const replacementsTab = screen.getByRole('tab', { name: 'Replacements' });

    expect(statusTab.getAttribute('aria-selected')).toBe('true');
    expect(historyTab.getAttribute('aria-selected')).toBe('false');
    expect(screen.getByTestId('status-dashboard')).toBeDefined();

    fireEvent.click(replacementsTab);
    expect(replacementsTab.getAttribute('aria-selected')).toBe('true');
    expect(statusTab.getAttribute('aria-selected')).toBe('false');
    expect(screen.getByText('Replacement Rules')).toBeDefined();
    expect(screen.getByText('Preset Rule Sets')).toBeDefined();
    expect(screen.getByText('Medical Dictation')).toBeDefined();
    expect(screen.getByRole('button', { name: 'Add Rule' })).toBeDefined();

    fireEvent.click(screen.getByRole('checkbox'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('load_preset', { presetId: 'medical' });
    });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('set_replacement_rules', expect.anything());
    });
    expect(screen.getByRole('tab', { name: /Replacements/ }).textContent).toContain('2');

    fireEvent.click(historyTab);
    expect(historyTab.getAttribute('aria-selected')).toBe('true');
    expect(screen.getAllByText('Sample transcript text.').length).toBeGreaterThan(0);
  });

  it('renders skip link and main landmark for keyboard navigation', async () => {
    render(<App />);

    const skipLink = await screen.findByRole('link', { name: 'Skip to main content' });
    expect(skipLink.getAttribute('href')).toBe('#main-content');
    expect(screen.getByRole('main')).toBeDefined();
  });
});

describe('App onboarding gate', () => {
  beforeEach(() => {
    resetAppStoreState();
  });

  it('shows onboarding wizard and gates main app when onboarding is incomplete', async () => {
    const config = makeConfig({
      ui: {
        ...makeConfig().ui,
        onboarding_completed: false,
      },
    });
    installAppHandler(config);

    render(<App />);

    expect(await screen.findByText('Welcome to Voice Input Tool')).toBeDefined();
    expect(screen.queryByRole('tab', { name: 'Status' })).toBeNull();
  });

  it('treats missing onboarding_completed as completed and skips wizard', async () => {
    const config = makeConfig();
    delete config.ui.onboarding_completed;
    installAppHandler(config);

    render(<App />);

    expect(await screen.findByRole('tab', { name: 'Status' })).toBeDefined();
    expect(screen.queryByText('Welcome to Voice Input Tool')).toBeNull();
  });
});
