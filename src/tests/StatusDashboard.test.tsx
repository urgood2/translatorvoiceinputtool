import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { StatusDashboard } from '../components/Status/StatusDashboard';
import { useAppStore } from '../store';
import type { AppConfig, TranscriptEntry } from '../types';
import { setMockInvokeHandler } from './setup';

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
  let startRecordingMock: ReturnType<typeof vi.fn>;
  let stopRecordingMock: ReturnType<typeof vi.fn>;
  let cancelRecordingMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    setMockInvokeHandler(() => []);

    startRecordingMock = vi.fn(async () => {});
    stopRecordingMock = vi.fn(async () => {});
    cancelRecordingMock = vi.fn(async () => {});

    useAppStore.setState({
      appState: 'idle',
      enabled: true,
      config: buildConfig(),
      history: [],
      modelStatus: { status: 'ready', model_id: 'nvidia/parakeet-tdt-0.6b-v3' },
      sidecarStatus: { state: 'ready', restart_count: 0 },
      startRecording: startRecordingMock,
      stopRecording: stopRecordingMock,
      cancelRecording: cancelRecordingMock,
    });
  });

  it('renders all dashboard sections with store-backed values', () => {
    render(<StatusDashboard />);

    expect(screen.getByLabelText('App State')).toBeDefined();
    expect(screen.getByLabelText('Hotkey')).toBeDefined();
    expect(screen.getByLabelText('Last Transcript')).toBeDefined();
    expect(screen.getByLabelText('Model')).toBeDefined();
    expect(screen.getByLabelText('Sidecar')).toBeDefined();

    expect(screen.getByText('Idle')).toBeDefined();
    expect(screen.getByText('Ctrl+Shift+Space')).toBeDefined();
    expect(screen.getByText('(Push-to-Talk)')).toBeDefined();
    expect(screen.getByText('No transcripts yet.')).toBeDefined();
    expect(screen.getByTestId('model-badge-name').textContent).toBe('nvidia/parakeet-tdt-0.6b-v3');
    expect(screen.getByTestId('model-badge-status').textContent).toContain('Ready');
    expect(screen.getByTestId('sidecar-badge-state').textContent).toContain('Ready');
    expect(screen.getByTestId('sidecar-badge-restarts').textContent).toContain('Restarts: 0');
    expect(screen.getByTestId('recording-start-button')).toBeDefined();
  });

  it('updates reactively when store state changes', async () => {
    const { container } = render(<StatusDashboard />);

    act(() => {
      useAppStore.setState({
        appState: 'recording',
        history: [buildTranscript('hello from status dashboard test')],
        modelStatus: {
          status: 'loading',
          model_id: 'parakeet-next',
          progress: { current: 20, total: 100, unit: 'percent' },
        },
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
    });

    await waitFor(() => {
      expect(screen.getByText('Recording')).toBeDefined();
    });

    expect(screen.getByText('Alt+Space')).toBeDefined();
    expect(screen.getByText('(Toggle)')).toBeDefined();
    expect(screen.getByText('hello from status dashboard test')).toBeDefined();
    expect(screen.getByText('Injected')).toBeDefined();
    expect(screen.getByTestId('model-badge-name').textContent).toBe('parakeet-next');
    expect(screen.getByTestId('model-badge-status').textContent).toContain('Loading');
    expect(screen.getByTestId('model-badge-progress').textContent).toContain('Progress: 20%');
    expect(screen.getByTestId('sidecar-badge-state').textContent).toContain('Restarting');
    expect(screen.getByTestId('sidecar-badge-restarts').textContent).toContain('Restarts: 3');

    const pulseDot = container.querySelector('.animate-pulse');
    expect(pulseDot).not.toBeNull();
  });

  it('shows loading model state badge with animated indicator', async () => {
    const { container } = render(<StatusDashboard />);

    act(() => {
      useAppStore.setState({ appState: 'loading_model' });
    });

    await waitFor(() => {
      expect(screen.getByText('Loading Model')).toBeDefined();
    });

    const pulseDot = container.querySelector('.animate-pulse');
    expect(pulseDot).not.toBeNull();
  });

  it('shows error state badge without pulse animation', async () => {
    const { container } = render(<StatusDashboard />);

    act(() => {
      useAppStore.setState({ appState: 'error', errorDetail: 'Microphone device unavailable' });
    });

    await waitFor(() => {
      expect(screen.getByText('Error')).toBeDefined();
    });

    const pulseDot = container.querySelector('.animate-pulse');
    expect(pulseDot).toBeNull();
    expect(screen.getByTestId('app-state-error-detail').textContent).toContain('Microphone device unavailable');
  });

  it('shows transcribing state badge with animation', async () => {
    const { container } = render(<StatusDashboard />);

    act(() => {
      useAppStore.setState({ appState: 'transcribing' });
    });

    await waitFor(() => {
      expect(screen.getByText('Transcribing')).toBeDefined();
    });

    const pulseDot = container.querySelector('.animate-pulse');
    expect(pulseDot).not.toBeNull();
  });

  it('shows missing hotkey message when binding is empty', async () => {
    render(<StatusDashboard />);

    act(() => {
      useAppStore.setState({
        config: {
          ...buildConfig(),
          hotkeys: {
            primary: '',
            copy_last: 'Ctrl+Shift+V',
            mode: 'hold',
          },
        },
      });
    });

    await waitFor(() => {
      expect(screen.getByText('No hotkey configured.')).toBeDefined();
    });
    expect(screen.getByText('Configure it in Settings.')).toBeDefined();
  });

  it('truncates long transcript preview to around 100 characters', async () => {
    const longText =
      'This transcript is intentionally much longer than one hundred characters so the dashboard preview should truncate it cleanly with ellipsis.';

    render(<StatusDashboard />);
    act(() => {
      useAppStore.setState({ history: [buildTranscript(longText)] });
    });

    await waitFor(() => {
      expect(screen.getByText(/\.{3}$/)).toBeDefined();
    });
    expect(screen.queryByText(longText)).toBeNull();
  });

  it('invokes startRecording action when start button is pressed', async () => {
    render(<StatusDashboard />);

    fireEvent.click(screen.getByTestId('recording-start-button'));

    await waitFor(() => {
      expect(startRecordingMock).toHaveBeenCalledTimes(1);
    });
  });

  it('invokes stop and cancel actions when recording controls are pressed', async () => {
    useAppStore.setState({ appState: 'recording' });

    render(<StatusDashboard />);

    fireEvent.click(screen.getByTestId('recording-stop-button'));

    await waitFor(() => {
      expect(stopRecordingMock).toHaveBeenCalledTimes(1);
    });

    fireEvent.click(screen.getByTestId('recording-cancel-button'));

    await waitFor(() => {
      expect(cancelRecordingMock).toHaveBeenCalledTimes(1);
    });
  });

  it('uses display_name from model catalog when available', async () => {
    setMockInvokeHandler((cmd) => {
      if (cmd === 'get_model_catalog') {
        return [
          {
            model_id: 'nvidia/parakeet-tdt-0.6b-v3',
            display_name: 'NVIDIA Parakeet 0.6B',
          },
        ];
      }
      return [];
    });

    render(<StatusDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('model-badge-name').textContent).toBe('NVIDIA Parakeet 0.6B');
    });
  });

  it('shows not installed state and download action', async () => {
    const downloadModelMock = vi.fn(async () => {});
    useAppStore.setState({
      modelStatus: { status: 'missing', model_id: 'nvidia/parakeet-tdt-0.6b-v3' },
      downloadModel: downloadModelMock,
    });

    render(<StatusDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('model-badge-status').textContent).toContain('Not Installed');
    });

    fireEvent.click(screen.getByTestId('model-badge-download'));

    await waitFor(() => {
      expect(downloadModelMock).toHaveBeenCalledTimes(1);
    });
  });

  it('shows error status details for model failures', async () => {
    useAppStore.setState({
      modelStatus: {
        status: 'error',
        model_id: 'nvidia/parakeet-tdt-0.6b-v3',
        error: 'Failed to load model file',
      },
    });

    render(<StatusDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('model-badge-status').textContent).toContain('Error');
    });
    expect(screen.getByTestId('model-badge-error').textContent).toContain('Failed to load model file');
  });

  it('shows no-model guidance when model info is unavailable', async () => {
    useAppStore.setState({
      modelStatus: null,
      config: { ...buildConfig(), model: null },
    });

    render(<StatusDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('model-badge-empty').textContent).toContain('No model');
    });
    expect(screen.getByText('Select or download a model in Settings.')).toBeDefined();
  });

  it('shows failed sidecar state with restart action', async () => {
    useAppStore.setState({
      sidecarStatus: {
        state: 'failed',
        restart_count: 2,
        message: 'Health checks failed',
      },
    });

    render(<StatusDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('sidecar-badge-state').textContent).toContain('Failed');
    });
    expect(screen.getByTestId('sidecar-badge-message').textContent).toContain('Health checks failed');

    fireEvent.click(screen.getByTestId('sidecar-badge-restart'));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('restart_sidecar');
    });
  });
});
