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
  audio: { device_uid: 'device-1', audio_cues_enabled: true, trim_silence: true },
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
    const toggle = screen.getByRole('switch');
    fireEvent.click(toggle);

    expect(onConfigChange).toHaveBeenCalledWith(['audio', 'audio_cues_enabled'], false);
  });
});
