/**
 * Tests for settings panel components.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { MicrophoneSelect } from '../components/Settings/MicrophoneSelect';
import { HotkeyConfig } from '../components/Settings/HotkeyConfig';
import { InjectionSettings } from '../components/Settings/InjectionSettings';
import { SettingsPanel } from '../components/Settings/SettingsPanel';
import type { AudioDevice, AppConfig, InjectionConfig } from '../types';

// Mock data
const mockDevices: AudioDevice[] = [
  { uid: 'device-1', name: 'Built-in Microphone', is_default: true, sample_rate: 48000, channels: 1 },
  { uid: 'device-2', name: 'USB Microphone', is_default: false, sample_rate: 44100, channels: 2 },
];

const mockInjectionConfig: InjectionConfig = {
  paste_delay_ms: 100,
  restore_clipboard: true,
  suffix: ' ',
  focus_guard_enabled: true,
};

const mockConfig: AppConfig = {
  schema_version: 1,
  audio: {
    device_uid: 'device-1',
    audio_cues_enabled: true,
    trim_silence: true,
    vad_enabled: false,
    vad_silence_ms: 1200,
    vad_min_speech_ms: 250,
  },
  hotkeys: { primary: 'Ctrl+Shift+A', copy_last: 'Ctrl+Shift+C', mode: 'hold' },
  injection: mockInjectionConfig,
  model: null,
  replacements: [],
  ui: {
    show_on_startup: true,
    window_width: 800,
    window_height: 600,
    theme: 'system',
    onboarding_completed: true,
    overlay_enabled: true,
    locale: null,
    reduce_motion: false,
  },
  history: {
    persistence_mode: 'memory',
    max_entries: 100,
    encrypt_at_rest: true,
  },
  presets: { enabled_presets: [] },
};

describe('MicrophoneSelect', () => {
  it('renders device list', () => {
    render(
      <MicrophoneSelect
        devices={mockDevices}
        selectedUid="device-1"
        audioCuesEnabled={true}
        onDeviceChange={vi.fn()}
        onAudioCuesChange={vi.fn()}
      />
    );
    expect(screen.getByText('Microphone')).toBeDefined();
    expect(screen.getByText(/Built-in Microphone/)).toBeDefined();
  });

  it('shows default device indicator', () => {
    render(
      <MicrophoneSelect
        devices={mockDevices}
        selectedUid="device-1"
        audioCuesEnabled={true}
        onDeviceChange={vi.fn()}
        onAudioCuesChange={vi.fn()}
      />
    );
    expect(screen.getByText(/\(Default\)/)).toBeDefined();
  });

  it('shows device specifications', () => {
    render(
      <MicrophoneSelect
        devices={mockDevices}
        selectedUid="device-1"
        audioCuesEnabled={true}
        onDeviceChange={vi.fn()}
        onAudioCuesChange={vi.fn()}
      />
    );
    expect(screen.getByText(/48kHz, 1 channel/)).toBeDefined();
  });

  it('calls onDeviceChange when selection changes', async () => {
    const onDeviceChange = vi.fn().mockResolvedValue(undefined);
    render(
      <MicrophoneSelect
        devices={mockDevices}
        selectedUid="device-1"
        audioCuesEnabled={true}
        onDeviceChange={onDeviceChange}
        onAudioCuesChange={vi.fn()}
      />
    );

    const select = screen.getByRole('combobox');
    fireEvent.change(select, { target: { value: 'device-2' } });

    expect(onDeviceChange).toHaveBeenCalledWith('device-2');
  });

  it('shows audio cues toggle', () => {
    render(
      <MicrophoneSelect
        devices={mockDevices}
        selectedUid="device-1"
        audioCuesEnabled={true}
        onDeviceChange={vi.fn()}
        onAudioCuesChange={vi.fn()}
      />
    );
    expect(screen.getByText('Audio Cues')).toBeDefined();
  });

  it('toggles audio cues', () => {
    const onAudioCuesChange = vi.fn().mockResolvedValue(undefined);
    render(
      <MicrophoneSelect
        devices={mockDevices}
        selectedUid="device-1"
        audioCuesEnabled={true}
        onDeviceChange={vi.fn()}
        onAudioCuesChange={onAudioCuesChange}
      />
    );

    const toggle = screen.getByRole('switch');
    fireEvent.click(toggle);

    expect(onAudioCuesChange).toHaveBeenCalledWith(false);
  });

  it('shows empty state when no devices', () => {
    render(
      <MicrophoneSelect
        devices={[]}
        selectedUid={undefined}
        audioCuesEnabled={true}
        onDeviceChange={vi.fn()}
        onAudioCuesChange={vi.fn()}
      />
    );
    expect(screen.getByText('No devices found')).toBeDefined();
  });
});

describe('HotkeyConfig', () => {
  it('renders primary hotkey', () => {
    render(
      <HotkeyConfig
        primaryHotkey="Ctrl+Shift+A"
        copyLastHotkey="Ctrl+Shift+C"
        mode="hold"
        onPrimaryChange={vi.fn()}
        onCopyLastChange={vi.fn()}
        onModeChange={vi.fn()}
      />
    );
    expect(screen.getByText('Recording Hotkey')).toBeDefined();
    expect(screen.getByText('Ctrl+Shift+A')).toBeDefined();
  });

  it('renders copy-last hotkey', () => {
    render(
      <HotkeyConfig
        primaryHotkey="Ctrl+Shift+A"
        copyLastHotkey="Ctrl+Shift+C"
        mode="hold"
        onPrimaryChange={vi.fn()}
        onCopyLastChange={vi.fn()}
        onModeChange={vi.fn()}
      />
    );
    expect(screen.getByText('Copy Last Hotkey')).toBeDefined();
    expect(screen.getByText('Ctrl+Shift+C')).toBeDefined();
  });

  it('renders mode selector', () => {
    render(
      <HotkeyConfig
        primaryHotkey="Ctrl+Shift+A"
        copyLastHotkey="Ctrl+Shift+C"
        mode="hold"
        onPrimaryChange={vi.fn()}
        onCopyLastChange={vi.fn()}
        onModeChange={vi.fn()}
      />
    );
    expect(screen.getByText('Hold to record')).toBeDefined();
    expect(screen.getByText('Press to toggle')).toBeDefined();
  });

  it('calls onModeChange when mode changes', () => {
    const onModeChange = vi.fn().mockResolvedValue(undefined);
    render(
      <HotkeyConfig
        primaryHotkey="Ctrl+Shift+A"
        copyLastHotkey="Ctrl+Shift+C"
        mode="hold"
        onPrimaryChange={vi.fn()}
        onCopyLastChange={vi.fn()}
        onModeChange={onModeChange}
      />
    );

    const toggleOption = screen.getByLabelText('Press to toggle');
    fireEvent.click(toggleOption);

    expect(onModeChange).toHaveBeenCalledWith('toggle');
  });

  it('shows effective mode warning when different from configured', () => {
    render(
      <HotkeyConfig
        primaryHotkey="Ctrl+Shift+A"
        copyLastHotkey="Ctrl+Shift+C"
        mode="hold"
        effectiveMode={{ configured: 'hold', effective: 'toggle', reason: 'Platform limitation' }}
        onPrimaryChange={vi.fn()}
        onCopyLastChange={vi.fn()}
        onModeChange={vi.fn()}
      />
    );
    expect(screen.getByText(/Using "toggle" mode instead of "hold"/)).toBeDefined();
    expect(screen.getByText(/Platform limitation/)).toBeDefined();
  });

  it('does not capture key presses when disabled', () => {
    const onPrimaryChange = vi.fn().mockResolvedValue(undefined);
    render(
      <HotkeyConfig
        primaryHotkey="Ctrl+Shift+A"
        copyLastHotkey="Ctrl+Shift+C"
        mode="hold"
        onPrimaryChange={onPrimaryChange}
        onCopyLastChange={vi.fn()}
        onModeChange={vi.fn()}
        isLoading={true}
      />
    );

    const hotkeyInput = screen.getByText('Ctrl+Shift+A');
    fireEvent.focus(hotkeyInput);
    fireEvent.keyDown(hotkeyInput, { key: 'x', ctrlKey: true });

    expect(hotkeyInput).toHaveAttribute('tabIndex', '-1');
    expect(screen.queryByText('Press keys...')).toBeNull();
    expect(onPrimaryChange).not.toHaveBeenCalled();
  });

  it('shows an error when clearing hotkey fails', async () => {
    const onPrimaryChange = vi.fn().mockRejectedValue(new Error('clear failed'));
    render(
      <HotkeyConfig
        primaryHotkey="Ctrl+Shift+A"
        copyLastHotkey=""
        mode="hold"
        onPrimaryChange={onPrimaryChange}
        onCopyLastChange={vi.fn()}
        onModeChange={vi.fn()}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'Clear' }));

    expect(onPrimaryChange).toHaveBeenCalledWith('');
    expect(await screen.findByText('clear failed')).toBeDefined();
  });

  it('does not capture Tab as a hotkey while recording', () => {
    const onPrimaryChange = vi.fn().mockResolvedValue(undefined);
    render(
      <HotkeyConfig
        primaryHotkey="Ctrl+Shift+A"
        copyLastHotkey="Ctrl+Shift+C"
        mode="hold"
        onPrimaryChange={onPrimaryChange}
        onCopyLastChange={vi.fn()}
        onModeChange={vi.fn()}
      />
    );

    const hotkeyInput = screen.getByText('Ctrl+Shift+A');
    fireEvent.click(hotkeyInput);
    expect(screen.getAllByText('Press keys...').length).toBeGreaterThan(0);

    fireEvent.keyDown(hotkeyInput, { key: 'Tab' });

    expect(onPrimaryChange).not.toHaveBeenCalled();
    expect(screen.queryByText('Press keys...')).toBeNull();
  });
});

describe('InjectionSettings', () => {
  it('renders paste delay slider', () => {
    render(
      <InjectionSettings
        config={mockInjectionConfig}
        onChange={vi.fn()}
      />
    );
    expect(screen.getByText('Paste Delay')).toBeDefined();
    expect(screen.getByText('100ms')).toBeDefined();
  });

  it('renders restore clipboard toggle', () => {
    render(
      <InjectionSettings
        config={mockInjectionConfig}
        onChange={vi.fn()}
      />
    );
    expect(screen.getByText('Restore Clipboard')).toBeDefined();
  });

  it('renders suffix selector', () => {
    render(
      <InjectionSettings
        config={mockInjectionConfig}
        onChange={vi.fn()}
      />
    );
    expect(screen.getByText('Text Suffix')).toBeDefined();
    expect(screen.getByText('None')).toBeDefined();
    expect(screen.getByText('Space')).toBeDefined();
    expect(screen.getByText('Newline')).toBeDefined();
  });

  it('renders focus guard toggle', () => {
    render(
      <InjectionSettings
        config={mockInjectionConfig}
        onChange={vi.fn()}
      />
    );
    expect(screen.getByText('Focus Guard')).toBeDefined();
  });

  it('shows focus guard explanation when enabled', () => {
    render(
      <InjectionSettings
        config={{ ...mockInjectionConfig, focus_guard_enabled: true }}
        onChange={vi.fn()}
      />
    );
    expect(screen.getByText(/text will only be copied to clipboard/)).toBeDefined();
  });

  it('calls onChange when suffix changes', () => {
    const onChange = vi.fn().mockResolvedValue(undefined);
    render(
      <InjectionSettings
        config={mockInjectionConfig}
        onChange={onChange}
      />
    );

    const newlineButton = screen.getByText('Newline');
    fireEvent.click(newlineButton);

    expect(onChange).toHaveBeenCalledWith('suffix', '\n');
  });

  it('calls onChange when focus guard toggles', () => {
    const onChange = vi.fn().mockResolvedValue(undefined);
    render(
      <InjectionSettings
        config={mockInjectionConfig}
        onChange={onChange}
      />
    );

    const toggles = screen.getAllByRole('switch');
    // Focus guard is the second toggle
    fireEvent.click(toggles[1]);

    expect(onChange).toHaveBeenCalledWith('focus_guard_enabled', false);
  });
});

describe('SettingsPanel', () => {
  it('renders tab navigation', () => {
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
      />
    );
    expect(screen.getByText('Audio')).toBeDefined();
    expect(screen.getByText('Hotkeys')).toBeDefined();
    expect(screen.getByText('Injection')).toBeDefined();
  });

  it('shows audio tab by default', () => {
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
      />
    );
    expect(screen.getByText('Microphone')).toBeDefined();
    expect(screen.getByText('Microphone Test')).toBeDefined();
    expect(screen.getByRole('button', { name: 'Start Test' })).toBeDefined();
  });

  it('switches to hotkeys tab', () => {
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
      />
    );

    fireEvent.click(screen.getByText('Hotkeys'));

    expect(screen.getByText('Recording Hotkey')).toBeDefined();
  });

  it('supports arrow key navigation between settings tabs', () => {
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
      />
    );

    const audioTab = screen.getByRole('tab', { name: 'Audio' });
    fireEvent.keyDown(audioTab, { key: 'ArrowRight' });

    const hotkeysTab = screen.getByRole('tab', { name: 'Hotkeys' });
    expect(hotkeysTab).toHaveFocus();
    expect(screen.getByText('Recording Hotkey')).toBeDefined();
  });

  it('switches to injection tab', () => {
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
      />
    );

    fireEvent.click(screen.getByText('Injection'));

    expect(screen.getByText('Text Injection')).toBeDefined();
  });

  it('calls onConfigChange with correct path', async () => {
    const onConfigChange = vi.fn().mockResolvedValue(undefined);
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={onConfigChange}
      />
    );

    // Toggle audio cues
    const toggle = screen.getByRole('switch', { name: /audio cues/i });
    fireEvent.click(toggle);

    expect(onConfigChange).toHaveBeenCalledWith(['audio', 'audio_cues_enabled'], false);
  });

  it('renders VAD toggle in audio tab', () => {
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
      />
    );

    expect(screen.getByText('Voice Activity Detection')).toBeDefined();
    expect(screen.getByText('Auto-Stop on Silence')).toBeDefined();
    const vadToggle = screen.getByRole('switch', { name: /auto-stop on silence/i });
    expect(vadToggle.getAttribute('aria-checked')).toBe('false');
  });

  it('does not show VAD sliders when VAD is disabled', () => {
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
      />
    );

    expect(screen.queryByLabelText(/silence before stop/i)).toBeNull();
    expect(screen.queryByLabelText(/min speech before stop/i)).toBeNull();
  });

  it('shows VAD sliders when VAD is enabled', () => {
    const vadConfig = {
      ...mockConfig,
      audio: { ...mockConfig.audio, vad_enabled: true },
    };
    render(
      <SettingsPanel
        config={vadConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
      />
    );

    expect(screen.getByLabelText(/silence before stop/i)).toBeDefined();
    expect(screen.getByLabelText(/min speech before stop/i)).toBeDefined();
  });

  it('toggles VAD enabled via onConfigChange', () => {
    const onConfigChange = vi.fn().mockResolvedValue(undefined);
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={onConfigChange}
      />
    );

    const vadToggle = screen.getByRole('switch', { name: /auto-stop on silence/i });
    fireEvent.click(vadToggle);

    expect(onConfigChange).toHaveBeenCalledWith(['audio', 'vad_enabled'], true);
  });

  it('updates VAD silence_ms slider via onConfigChange', () => {
    const onConfigChange = vi.fn().mockResolvedValue(undefined);
    const vadConfig = {
      ...mockConfig,
      audio: { ...mockConfig.audio, vad_enabled: true },
    };
    render(
      <SettingsPanel
        config={vadConfig}
        devices={mockDevices}
        onConfigChange={onConfigChange}
      />
    );

    const silenceSlider = screen.getByLabelText(/silence before stop/i);
    fireEvent.change(silenceSlider, { target: { value: '2000' } });

    expect(onConfigChange).toHaveBeenCalledWith(['audio', 'vad_silence_ms'], 2000);
  });

  it('updates VAD min_speech_ms slider via onConfigChange', () => {
    const onConfigChange = vi.fn().mockResolvedValue(undefined);
    const vadConfig = {
      ...mockConfig,
      audio: { ...mockConfig.audio, vad_enabled: true },
    };
    render(
      <SettingsPanel
        config={vadConfig}
        devices={mockDevices}
        onConfigChange={onConfigChange}
      />
    );

    const minSpeechSlider = screen.getByLabelText(/min speech before stop/i);
    fireEvent.change(minSpeechSlider, { target: { value: '500' } });

    expect(onConfigChange).toHaveBeenCalledWith(['audio', 'vad_min_speech_ms'], 500);
  });

  it('shows purge button in appearance tab when onPurgeHistory is provided', () => {
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
        onPurgeHistory={vi.fn().mockResolvedValue(undefined)}
        historyCount={5}
      />
    );
    fireEvent.click(screen.getByText('Appearance'));
    expect(screen.getByTestId('settings-purge-history-button')).toBeDefined();
    expect(screen.getByText(/5 entries stored/)).toBeDefined();
  });

  it('disables purge button when history is empty', () => {
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
        onPurgeHistory={vi.fn().mockResolvedValue(undefined)}
        historyCount={0}
      />
    );
    fireEvent.click(screen.getByText('Appearance'));
    expect(screen.getByTestId('settings-purge-history-button')).toBeDisabled();
    expect(screen.getByText(/No entries stored/)).toBeDefined();
  });

  it('shows purge confirmation and calls handler on confirm', async () => {
    const onPurgeHistory = vi.fn().mockResolvedValue(undefined);
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
        onPurgeHistory={onPurgeHistory}
        historyCount={3}
      />
    );
    fireEvent.click(screen.getByText('Appearance'));
    fireEvent.click(screen.getByTestId('settings-purge-history-button'));
    expect(screen.getByTestId('settings-purge-confirm-dialog')).toBeDefined();

    fireEvent.click(screen.getByTestId('settings-purge-confirm'));
    expect(onPurgeHistory).toHaveBeenCalledTimes(1);

    expect(await screen.findByTestId('settings-purge-success')).toBeDefined();
  });

  it('cancels purge confirmation without calling handler', () => {
    const onPurgeHistory = vi.fn().mockResolvedValue(undefined);
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
        onPurgeHistory={onPurgeHistory}
        historyCount={3}
      />
    );
    fireEvent.click(screen.getByText('Appearance'));
    fireEvent.click(screen.getByTestId('settings-purge-history-button'));
    fireEvent.click(screen.getByTestId('settings-purge-cancel'));
    expect(screen.queryByTestId('settings-purge-confirm-dialog')).toBeNull();
    expect(onPurgeHistory).not.toHaveBeenCalled();
  });

  it('shows persistence mode info for disk mode', () => {
    const diskConfig = {
      ...mockConfig,
      history: { ...mockConfig.history, persistence_mode: 'disk' as const },
    };
    render(
      <SettingsPanel
        config={diskConfig}
        devices={mockDevices}
        onConfigChange={vi.fn()}
        onPurgeHistory={vi.fn().mockResolvedValue(undefined)}
        historyCount={2}
      />
    );
    fireEvent.click(screen.getByText('Appearance'));
    expect(screen.getByText(/Saved to disk/)).toBeDefined();
  });

  it('calls onRefreshDevices when refresh is clicked in audio tab', () => {
    const onRefreshDevices = vi.fn().mockResolvedValue(undefined);
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onRefreshDevices={onRefreshDevices}
        onConfigChange={vi.fn()}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    expect(onRefreshDevices).toHaveBeenCalledTimes(1);
  });

  it('supports arrow key navigation for appearance theme radios', () => {
    const onConfigChange = vi.fn().mockResolvedValue(undefined);
    render(
      <SettingsPanel
        config={mockConfig}
        devices={mockDevices}
        onConfigChange={onConfigChange}
      />
    );

    fireEvent.click(screen.getByRole('tab', { name: 'Appearance' }));

    const systemRadio = screen.getByRole('radio', { name: 'system' });
    fireEvent.keyDown(systemRadio, { key: 'ArrowRight' });

    expect(onConfigChange).toHaveBeenCalledWith(['ui', 'theme'], 'light');
    expect(screen.getByRole('radio', { name: 'light' })).toHaveFocus();
  });
});
