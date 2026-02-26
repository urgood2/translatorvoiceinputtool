import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it } from 'vitest';

import { StatusDashboard } from '../components/Status/StatusDashboard';
import { useAppStore } from '../store';
import { setMockInvokeHandler } from './setup';
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

function buildTranscript(id: string, text: string): TranscriptEntry {
  return {
    id,
    text,
    raw_text: text,
    final_text: text,
    timestamp: new Date().toISOString(),
    audio_duration_ms: 1100,
    transcription_duration_ms: 280,
    injection_result: { status: 'injected' },
  };
}

describe('Recording command integration', () => {
  beforeEach(() => {
    useAppStore.setState({
      appState: 'idle',
      enabled: true,
      errorDetail: undefined,
      config: buildConfig(),
      history: [],
      recordingStatus: null,
      modelStatus: { status: 'ready', model_id: 'nvidia/parakeet-tdt-0.6b-v3' },
      sidecarStatus: { state: 'ready', restart_count: 0 },
    });
  });

  it('start/stop/cancel invoke the correct Tauri commands', async () => {
    const invokeLog: Array<{ cmd: string; args?: unknown }> = [];
    setMockInvokeHandler((cmd, args) => {
      invokeLog.push({ cmd, args });
      console.debug('[recording.test] invoke', cmd, args);
      return undefined;
    });

    render(<StatusDashboard />);
    fireEvent.click(screen.getByTestId('recording-start-button'));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('start_recording');
    });

    act(() => {
      useAppStore.getState()._setRecordingStatus({
        phase: 'recording',
        session_id: 'session-1',
        started_at: '2026-02-19T00:00:00Z',
        seq: 1,
      });
    });
    console.debug('[recording.test] state', useAppStore.getState().appState);

    fireEvent.click(screen.getByTestId('recording-stop-button'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('stop_recording');
    });

    fireEvent.click(screen.getByTestId('recording-cancel-button'));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('cancel_recording');
    });

    const recordingCmds = invokeLog
      .map((entry) => entry.cmd)
      .filter((cmd) => ['start_recording', 'stop_recording', 'cancel_recording'].includes(cmd));
    expect(recordingCmds).toEqual([
      'start_recording',
      'stop_recording',
      'cancel_recording',
    ]);
  });

  it('transitions recording -> transcribing -> idle via recording:status updates', () => {
    render(<StatusDashboard />);

    act(() => {
      useAppStore.getState()._setRecordingStatus({
        phase: 'recording',
        session_id: 'session-2',
        started_at: '2026-02-19T00:00:00Z',
        seq: 2,
      });
    });
    console.debug('[recording.test] state', useAppStore.getState().appState);
    expect(useAppStore.getState().appState).toBe('recording');
    expect(screen.getByText('Recording')).toBeDefined();
    expect(screen.queryByTestId('recording-start-button')).toBeNull();
    expect(screen.getByTestId('recording-stop-button')).toBeDefined();

    act(() => {
      useAppStore.getState()._setRecordingStatus({
        phase: 'transcribing',
        session_id: 'session-2',
        audio_ms: 1450,
        seq: 3,
      });
    });
    console.debug('[recording.test] state', useAppStore.getState().appState);
    expect(useAppStore.getState().appState).toBe('transcribing');
    expect(screen.getByText('Transcribing')).toBeDefined();

    act(() => {
      useAppStore.getState()._setRecordingStatus({
        phase: 'idle',
        session_id: 'session-2',
        seq: 4,
      });
    });
    console.debug('[recording.test] state', useAppStore.getState().appState);
    expect(useAppStore.getState().appState).toBe('idle');
    expect(screen.getByText('Idle')).toBeDefined();
  });

  it('cancel returns to idle without adding transcript entry', () => {
    const existing = buildTranscript('t-existing', 'already here');
    useAppStore.setState({
      appState: 'recording',
      history: [existing],
    });

    render(<StatusDashboard />);

    act(() => {
      useAppStore.getState()._setRecordingStatus({
        phase: 'idle',
        session_id: 'session-cancel',
        seq: 5,
      });
    });
    console.debug('[recording.test] state', useAppStore.getState().appState);

    expect(useAppStore.getState().appState).toBe('idle');
    expect(useAppStore.getState().history).toEqual([existing]);
  });

  it('handles invoke failures by entering error state without crashing UI', async () => {
    setMockInvokeHandler((cmd) => {
      if (cmd === 'start_recording') {
        throw new Error('start failed');
      }
      return undefined;
    });

    render(<StatusDashboard />);
    fireEvent.click(screen.getByTestId('recording-start-button'));

    await waitFor(() => {
      expect(useAppStore.getState().appState).toBe('error');
      expect(useAppStore.getState().errorDetail).toContain('start failed');
    });

    expect(screen.getByTestId('status-dashboard')).toBeDefined();
  });
});
