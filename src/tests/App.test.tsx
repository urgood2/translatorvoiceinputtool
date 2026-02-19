import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it } from 'vitest';

import App from '../App';
import { setMockInvokeHandler } from './setup';

describe('App diagnostics panels', () => {
  beforeEach(() => {
    setMockInvokeHandler((cmd) => {
      switch (cmd) {
        case 'get_config':
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
          };
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
          return [
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
          ];
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
          return [];
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
            config: {
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
            },
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
    expect(screen.getByRole('button', { name: 'Hotkeys' })).toBeDefined();
    expect(screen.getByRole('button', { name: 'Injection' })).toBeDefined();
    expect(screen.getByText('Microphone Test')).toBeDefined();
    expect(screen.getByRole('button', { name: 'Start Test' })).toBeDefined();
    expect(screen.getByText('System Health Check')).toBeDefined();
    expect(screen.getAllByText('Diagnostics').length).toBeGreaterThan(0);
  });
});
