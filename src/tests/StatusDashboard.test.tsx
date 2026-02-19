import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatusDashboard } from '../components/Status/StatusDashboard';
import { useAppStore } from '../store';
import type { AppConfig, TranscriptEntry } from '../types';

function buildConfig(): AppConfig {
  return {
    schema_version: 1,
    audio: {
      device_uid: undefined,
      audio_cues_enabled: true,
      trim_silence: true,
      vad_enabled: false,
      vad_silence_ms: 1200,
      vad_min_speech_ms: 250,
    },
    hotkeys: {
      primary: 'Ctrl+Shift+Space',
      copy_last: 'Ctrl+Shift+V',
      mode: 'hold',
    },
    injection: {
      paste_delay_ms: 40,
      restore_clipboard: true,
      suffix: ' ',
      focus_guard_enabled: true,
      app_overrides: {},
    },
    model: {
      model_id: 'nvidia/parakeet-tdt-0.6b-v3',
      device: null,
      preferred_device: 'auto',
      language: null,
    },
    replacements: [],
    ui: {
      show_on_startup: true,
      window_width: 600,
      window_height: 500,
      theme: 'system',
      onboarding_completed: false,
      overlay_enabled: true,
      locale: null,
      reduce_motion: false,
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
}

function buildTranscript(text: string): TranscriptEntry {
  return {
    id: 'test-transcript-id',
    text,
    raw_text: text,
    final_text: text,
    timestamp: new Date().toISOString(),
    audio_duration_ms: 1200,
    transcription_duration_ms: 300,
    injection_result: { status: 'injected' },
  };
}

describe('StatusDashboard', () => {
  beforeEach(() => {
    useAppStore.setState({
      appState: 'idle',
      enabled: true,
      config: buildConfig(),
      history: [],
      modelStatus: { status: 'ready', model_id: 'nvidia/parakeet-tdt-0.6b-v3' },
      sidecarStatus: { state: 'ready', restart_count: 0 },
    });
  });

  it('renders all dashboard sections with store-backed values', () => {
    render(<StatusDashboard />);

    expect(screen.getByLabelText('App State')).toBeDefined();
    expect(screen.getByLabelText('Hotkey')).toBeDefined();
    expect(screen.getByLabelText('Last Transcript')).toBeDefined();
    expect(screen.getByLabelText('Model')).toBeDefined();
    expect(screen.getByLabelText('Sidecar')).toBeDefined();

    expect(screen.getByText('Ready')).toBeDefined();
    expect(screen.getByText('Ctrl+Shift+Space')).toBeDefined();
    expect(screen.getByText('Hold')).toBeDefined();
    expect(screen.getByText('No transcript available yet.')).toBeDefined();
    expect(screen.getByText('nvidia/parakeet-tdt-0.6b-v3')).toBeDefined();
    expect(screen.getByText('ready')).toBeDefined();
    expect(screen.getByText('0')).toBeDefined();
  });

  it('updates reactively when store state changes', () => {
    const { container } = render(<StatusDashboard />);

    useAppStore.setState({
      appState: 'recording',
      history: [buildTranscript('hello from status dashboard test')],
      modelStatus: { status: 'loading', model_id: 'parakeet-next' },
      sidecarStatus: { state: 'restarting', restart_count: 3 },
      config: {
        ...buildConfig(),
        hotkeys: {
          primary: 'Alt+Space',
          copy_last: 'Ctrl+Shift+V',
          mode: 'toggle',
        },
      },
    });

    expect(screen.getByText('Recording')).toBeDefined();
    expect(screen.getByText('Alt+Space')).toBeDefined();
    expect(screen.getByText('Toggle')).toBeDefined();
    expect(screen.getByText('hello from status dashboard test')).toBeDefined();
    expect(screen.getByText('parakeet-next')).toBeDefined();
    expect(screen.getByText('loading')).toBeDefined();
    expect(screen.getByText('restarting')).toBeDefined();
    expect(screen.getByText('3')).toBeDefined();

    const pulseDot = container.querySelector('.animate-pulse');
    expect(pulseDot).not.toBeNull();
  });
});
