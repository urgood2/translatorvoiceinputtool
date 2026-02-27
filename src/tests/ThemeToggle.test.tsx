import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it } from 'vitest';

import App from '../App';
import { setMockInvokeHandler } from './setup';

describe('Theme toggle persistence', () => {
  let currentConfig: any;

  beforeEach(() => {
    document.documentElement.classList.remove('dark');
    document.documentElement.style.colorScheme = '';

    currentConfig = {
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
        language: 'en',
      },
      replacements: [],
      ui: {
        show_on_startup: false,
        window_width: 800,
        window_height: 600,
        theme: 'system',
        onboarding_completed: true,
        overlay_enabled: true,
        locale: 'en-US',
        reduce_motion: false,
      },
      history: {
        persistence_mode: 'memory',
        max_entries: 100,
        encrypt_at_rest: false,
      },
      presets: {
        enabled_presets: [],
      },
    };

    setMockInvokeHandler((cmd, args) => {
      switch (cmd) {
        case 'get_config':
          return currentConfig;
        case 'update_config':
          currentConfig = (args as { config: any }).config;
          return undefined;
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
          return [];
        case 'get_hotkey_status':
          return {
            primary: 'Ctrl+Shift+Space',
            copy_last: 'Ctrl+Shift+C',
            mode: 'hold',
            registered: true,
          };
        case 'get_available_presets':
          return [];
        case 'get_app_state':
          return { state: 'idle', enabled: true };
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
            config: currentConfig,
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

  it('persists theme changes from Settings appearance controls', async () => {
    render(<App />);

    fireEvent.click(await screen.findByRole('tab', { name: 'Settings' }));
    fireEvent.click(screen.getByRole('tab', { name: 'Appearance' }));

    fireEvent.click(screen.getByRole('radio', { name: 'dark' }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'update_config',
        expect.objectContaining({
          config: expect.objectContaining({
            ui: expect.objectContaining({
              theme: 'dark',
            }),
          }),
        })
      );
    });
    expect(currentConfig.ui.theme).toBe('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
    expect(document.documentElement.style.colorScheme).toBe('dark');
    expect(screen.getByRole('radio', { name: 'dark' })).toHaveAttribute('aria-checked', 'true');

    fireEvent.click(screen.getByRole('radio', { name: 'light' }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'update_config',
        expect.objectContaining({
          config: expect.objectContaining({
            ui: expect.objectContaining({
              theme: 'light',
            }),
          }),
        })
      );
    });
    expect(currentConfig.ui.theme).toBe('light');
    expect(document.documentElement.classList.contains('dark')).toBe(false);
    expect(document.documentElement.style.colorScheme).toBe('light');
    expect(screen.getByRole('radio', { name: 'light' })).toHaveAttribute('aria-checked', 'true');
  });
});
