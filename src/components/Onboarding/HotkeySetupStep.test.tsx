/**
 * Tests for HotkeySetupStep onboarding component.
 *
 * Covers: hotkey display, key recording, mode selection,
 * config update calls, error handling.
 */

import { describe, test, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import { HotkeySetupStep } from './HotkeySetupStep';
import { useAppStore } from '../../store/appStore';
import type { AppConfig } from '../../types';

// ── Mock Tauri invoke ─────────────────────────────────────────────

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

// ── Helpers ───────────────────────────────────────────────────────

function makeConfig(hotkeyOverrides: Partial<AppConfig['hotkeys']> = {}): AppConfig {
  return {
    schema_version: 1,
    audio: {
      audio_cues_enabled: true,
      trim_silence: false,
      vad_enabled: false,
      vad_silence_ms: 300,
      vad_min_speech_ms: 100,
    },
    hotkeys: {
      primary: 'Ctrl+Shift+Space',
      copy_last: 'Ctrl+Shift+C',
      mode: 'hold',
      ...hotkeyOverrides,
    },
    injection: {
      paste_delay_ms: 50,
      restore_clipboard: true,
      suffix: '',
      focus_guard_enabled: true,
    },
    model: null,
    replacements: [],
    ui: {
      show_on_startup: true,
      window_width: 800,
      window_height: 600,
      theme: 'system',
      onboarding_completed: false,
      overlay_enabled: false,
      locale: null,
      reduce_motion: false,
    },
    history: { persistence_mode: 'memory', max_entries: 100, encrypt_at_rest: false },
    presets: { enabled_presets: [] },
  };
}

// ── Setup ─────────────────────────────────────────────────────────

const updateHotkeyConfigSpy = vi.fn().mockResolvedValue(undefined);

beforeEach(() => {
  vi.clearAllMocks();
  useAppStore.setState({
    config: makeConfig(),
    updateHotkeyConfig: updateHotkeyConfigSpy,
  });
});

// ── Tests ─────────────────────────────────────────────────────────

describe('HotkeySetupStep', () => {
  test('renders heading and description', () => {
    render(<HotkeySetupStep onReady={vi.fn()} />);
    expect(screen.getByText('Hotkey Configuration')).toBeDefined();
    expect(screen.getByText(/keyboard shortcut/)).toBeDefined();
  });

  test('shows current primary hotkey', () => {
    render(<HotkeySetupStep onReady={vi.fn()} />);
    expect(screen.getByText('Ctrl+Shift+Space')).toBeDefined();
  });

  test('shows recording label', () => {
    render(<HotkeySetupStep onReady={vi.fn()} />);
    expect(screen.getByText('Recording Hotkey')).toBeDefined();
  });

  test('shows "Press keys..." when focused', () => {
    render(<HotkeySetupStep onReady={vi.fn()} />);
    const recorder = screen.getByRole('button', { name: 'Recording hotkey' });

    fireEvent.focus(recorder);

    expect(screen.getByText('Press keys...')).toBeDefined();
  });

  test('records new hotkey on key press', async () => {
    render(<HotkeySetupStep onReady={vi.fn()} />);
    const recorder = screen.getByRole('button', { name: 'Recording hotkey' });

    fireEvent.focus(recorder);

    await act(async () => {
      fireEvent.keyDown(recorder, {
        key: 'A',
        ctrlKey: true,
        shiftKey: false,
        altKey: false,
        metaKey: false,
      });
    });

    expect(updateHotkeyConfigSpy).toHaveBeenCalledWith({ primary: 'Ctrl+A' });
  });

  test('ignores modifier-only key presses', () => {
    render(<HotkeySetupStep onReady={vi.fn()} />);
    const recorder = screen.getByRole('button', { name: 'Recording hotkey' });

    fireEvent.focus(recorder);
    fireEvent.keyDown(recorder, { key: 'Shift' });

    // Should still be in recording mode
    expect(screen.getByText('Press keys...')).toBeDefined();
    expect(updateHotkeyConfigSpy).not.toHaveBeenCalled();
  });

  test('hold mode is selected by default', () => {
    render(<HotkeySetupStep onReady={vi.fn()} />);
    const holdRadio = screen.getByDisplayValue('hold') as HTMLInputElement;
    const toggleRadio = screen.getByDisplayValue('toggle') as HTMLInputElement;

    expect(holdRadio.checked).toBe(true);
    expect(toggleRadio.checked).toBe(false);
  });

  test('switching to toggle mode calls updateHotkeyConfig', async () => {
    render(<HotkeySetupStep onReady={vi.fn()} />);
    const toggleRadio = screen.getByDisplayValue('toggle');

    await act(async () => {
      fireEvent.click(toggleRadio);
    });

    expect(updateHotkeyConfigSpy).toHaveBeenCalledWith({ mode: 'toggle' });
  });

  test('switching to hold mode calls updateHotkeyConfig', async () => {
    useAppStore.setState({ config: makeConfig({ mode: 'toggle' }) });
    render(<HotkeySetupStep onReady={vi.fn()} />);
    const holdRadio = screen.getByDisplayValue('hold');

    await act(async () => {
      fireEvent.click(holdRadio);
    });

    expect(updateHotkeyConfigSpy).toHaveBeenCalledWith({ mode: 'hold' });
  });

  test('shows hold mode description', () => {
    render(<HotkeySetupStep onReady={vi.fn()} />);
    expect(screen.getByText('Hold to record')).toBeDefined();
    expect(screen.getByText(/Recording while the hotkey is held down/)).toBeDefined();
  });

  test('shows toggle mode description', () => {
    render(<HotkeySetupStep onReady={vi.fn()} />);
    expect(screen.getByText('Press to toggle')).toBeDefined();
    expect(screen.getByText(/Press once to start recording/)).toBeDefined();
  });

  test('shows error when hotkey change fails', async () => {
    updateHotkeyConfigSpy.mockRejectedValueOnce(new Error('Hotkey conflict'));
    render(<HotkeySetupStep onReady={vi.fn()} />);
    const recorder = screen.getByRole('button', { name: 'Recording hotkey' });

    fireEvent.focus(recorder);

    await act(async () => {
      fireEvent.keyDown(recorder, {
        key: 'A',
        ctrlKey: true,
        shiftKey: false,
        altKey: false,
        metaKey: false,
      });
    });

    expect(screen.getByText('Hotkey conflict')).toBeDefined();
  });

  test('shows error when mode change fails', async () => {
    updateHotkeyConfigSpy.mockRejectedValueOnce(new Error('Mode not supported'));
    render(<HotkeySetupStep onReady={vi.fn()} />);

    await act(async () => {
      fireEvent.click(screen.getByDisplayValue('toggle'));
    });

    expect(screen.getByText('Mode not supported')).toBeDefined();
  });

  test('renders toggle mode as checked when config says toggle', () => {
    useAppStore.setState({ config: makeConfig({ mode: 'toggle' }) });
    render(<HotkeySetupStep onReady={vi.fn()} />);

    const toggleRadio = screen.getByDisplayValue('toggle') as HTMLInputElement;
    expect(toggleRadio.checked).toBe(true);
  });

  test('handles multi-modifier key combo', async () => {
    render(<HotkeySetupStep onReady={vi.fn()} />);
    const recorder = screen.getByRole('button', { name: 'Recording hotkey' });

    fireEvent.focus(recorder);

    await act(async () => {
      fireEvent.keyDown(recorder, {
        key: 'F1',
        ctrlKey: true,
        shiftKey: true,
        altKey: true,
        metaKey: false,
      });
    });

    expect(updateHotkeyConfigSpy).toHaveBeenCalledWith({ primary: 'Ctrl+Alt+Shift+F1' });
  });

  test('falls back to defaults when config is null', () => {
    useAppStore.setState({ config: null });
    render(<HotkeySetupStep onReady={vi.fn()} />);

    // Should show default hotkey
    expect(screen.getByText('Ctrl+Shift+Space')).toBeDefined();
    // Hold mode should be default
    const holdRadio = screen.getByDisplayValue('hold') as HTMLInputElement;
    expect(holdRadio.checked).toBe(true);
  });
});
