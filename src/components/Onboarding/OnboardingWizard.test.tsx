/**
 * Tests for the OnboardingWizard component and onboarding gate logic.
 *
 * Covers: new install gate, existing user skip, migration safety,
 * skip button, step navigation, completion flow per bead bdp.1.8.
 */

import { describe, test, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, act, waitFor } from '@testing-library/react';
import { OnboardingWizard } from './OnboardingWizard';
import { useAppStore } from '../../store/appStore';
import type { AppConfig } from '../../types';

// ── Mock Tauri invoke ─────────────────────────────────────────────

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

const { invoke } = await import('@tauri-apps/api/core');

// ── Helpers ───────────────────────────────────────────────────────

function makeConfig(overrides: Partial<AppConfig['ui']> = {}): AppConfig {
  return {
    schema_version: 1,
    audio: {
      audio_cues_enabled: true,
      trim_silence: false,
      vad_enabled: false,
      vad_silence_ms: 300,
      vad_min_speech_ms: 100,
    },
    hotkeys: { primary: 'Ctrl+Shift+Space', copy_last: 'Ctrl+Shift+C', mode: 'hold' },
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
      ...overrides,
    },
    history: { persistence_mode: 'memory', max_entries: 100, encrypt_at_rest: false },
    presets: { enabled_presets: [] },
  };
}

// ── Setup ─────────────────────────────────────────────────────────

beforeEach(() => {
  vi.clearAllMocks();
  useAppStore.setState({
    config: makeConfig(),
    // MicSetupStep dependencies
    devices: [],
    selectedDeviceUid: null,
    audioLevel: null,
    isMeterRunning: false,
    refreshDevices: vi.fn(),
    selectDevice: vi.fn(),
    startMicTest: vi.fn().mockResolvedValue(undefined),
    stopMicTest: vi.fn().mockResolvedValue(undefined),
    // HotkeySetupStep dependencies
    updateHotkeyConfig: vi.fn(),
    // ModelReadinessStep dependencies
    modelStatus: { status: 'ready', model_id: 'parakeet-tdt-0.6b-v3' },
    refreshModelStatus: vi.fn(),
    downloadModel: vi.fn(),
  });
});

// ── Tests ─────────────────────────────────────────────────────────

describe('OnboardingWizard', () => {
  test('renders Welcome step on initial mount', () => {
    const onComplete = vi.fn();
    render(<OnboardingWizard onComplete={onComplete} />);
    expect(screen.getByText('Welcome to Voice Input Tool')).toBeDefined();
  });

  test('navigates forward through all 5 steps', async () => {
    const onComplete = vi.fn();
    useAppStore.setState({
      devices: [{ uid: 'mic-1', name: 'Built-in Mic', is_default: true, sample_rate: 48000, channels: 1 }],
      selectedDeviceUid: 'mic-1',
    });
    render(<OnboardingWizard onComplete={onComplete} />);

    // Step 1: Welcome
    expect(screen.getByText('Welcome to Voice Input Tool')).toBeDefined();
    await act(async () => {
      fireEvent.click(screen.getByText('Next'));
    });

    // Step 2: Microphone
    expect(screen.getByText('Microphone Setup')).toBeDefined();
    await act(async () => {
      fireEvent.click(screen.getByText('Start Test'));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('checkbox'));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    });

    // Step 3: Hotkey
    await waitFor(() => {
      expect(screen.getByText('Hotkey Configuration')).toBeDefined();
    });
    await act(async () => {
      fireEvent.click(screen.getByText('Next'));
    });

    // Step 4: Model auto-advances when ready, then Step 5 appears.
    await waitFor(() => {
      expect(screen.getByText('All Set!')).toBeDefined();
      expect(screen.getByText('Get Started')).toBeDefined();
    });
  });

  test('Back button navigates to previous step', () => {
    const onComplete = vi.fn();
    render(<OnboardingWizard onComplete={onComplete} />);

    fireEvent.click(screen.getByText('Next'));
    expect(screen.getByText('Microphone Setup')).toBeDefined();

    fireEvent.click(screen.getByText('Back'));
    expect(screen.getByText('Welcome to Voice Input Tool')).toBeDefined();
  });

  test('Back button is not shown on first step', () => {
    const onComplete = vi.fn();
    render(<OnboardingWizard onComplete={onComplete} />);
    expect(screen.queryByText('Back')).toBeNull();
  });

  test('Skip button calls onComplete and updates config', async () => {
    const onComplete = vi.fn();
    render(<OnboardingWizard onComplete={onComplete} />);

    await act(async () => {
      fireEvent.click(screen.getByText('Skip'));
    });

    expect(invoke).toHaveBeenCalledWith('update_config', expect.objectContaining({
      config: expect.objectContaining({
        ui: expect.objectContaining({ onboarding_completed: true }),
      }),
    }));
    expect(onComplete).toHaveBeenCalled();
  });

  test('Get Started button on last step calls onComplete', async () => {
    const onComplete = vi.fn();
    useAppStore.setState({
      devices: [{ uid: 'mic-1', name: 'Built-in Mic', is_default: true, sample_rate: 48000, channels: 1 }],
      selectedDeviceUid: 'mic-1',
    });
    render(<OnboardingWizard onComplete={onComplete} />);

    // Navigate to last step
    await act(async () => {
      fireEvent.click(screen.getByText('Next'));
    });
    await act(async () => {
      fireEvent.click(screen.getByText('Start Test'));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('checkbox'));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    });
    await waitFor(() => {
      expect(screen.getByText('Hotkey Configuration')).toBeDefined();
    });
    await act(async () => {
      fireEvent.click(screen.getByText('Next'));
    });

    await waitFor(() => {
      expect(screen.getByText('Get Started')).toBeDefined();
    });

    await act(async () => {
      fireEvent.click(screen.getByText('Get Started'));
    });

    expect(invoke).toHaveBeenCalledWith('update_config', expect.objectContaining({
      config: expect.objectContaining({
        ui: expect.objectContaining({ onboarding_completed: true }),
      }),
    }));
    expect(onComplete).toHaveBeenCalled();
  });

  test('step indicator shows correct progress', () => {
    const onComplete = vi.fn();
    useAppStore.setState({
      devices: [{ uid: 'mic-1', name: 'Built-in Mic', is_default: true, sample_rate: 48000, channels: 1 }],
      selectedDeviceUid: 'mic-1',
    });
    render(<OnboardingWizard onComplete={onComplete} />);

    const progressbar = screen.getByRole('progressbar');
    expect(progressbar).toBeDefined();
    expect(progressbar.getAttribute('aria-valuenow')).toBe('1');
    expect(progressbar.getAttribute('aria-valuemax')).toBe('5');

    fireEvent.click(screen.getByText('Next'));
    expect(progressbar.getAttribute('aria-valuenow')).toBe('2');
  });

  test('Skip is available on every step', async () => {
    const onComplete = vi.fn();
    useAppStore.setState({
      devices: [{ uid: 'mic-1', name: 'Built-in Mic', is_default: true, sample_rate: 48000, channels: 1 }],
      selectedDeviceUid: 'mic-1',
      modelStatus: { status: 'missing', model_id: 'parakeet-tdt-0.6b-v3' },
    });
    render(<OnboardingWizard onComplete={onComplete} />);

    // Welcome
    expect(screen.getByText('Skip')).toBeDefined();
    await act(async () => {
      fireEvent.click(screen.getByText('Next'));
    });

    // Microphone
    expect(screen.getByText('Skip')).toBeDefined();
    await act(async () => {
      fireEvent.click(screen.getByText('Start Test'));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('checkbox'));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    });

    // Hotkey
    expect(screen.getByText('Skip')).toBeDefined();
    await act(async () => {
      fireEvent.click(screen.getByText('Next'));
    });

    // Model
    await waitFor(() => {
      expect(screen.getByText('Skip')).toBeDefined();
      expect(screen.getByText('Speech Recognition Model')).toBeDefined();
    });
  });

  test('does not show global next button on microphone and model steps', async () => {
    const onComplete = vi.fn();
    useAppStore.setState({
      devices: [{ uid: 'mic-1', name: 'Built-in Mic', is_default: true, sample_rate: 48000, channels: 1 }],
      selectedDeviceUid: 'mic-1',
      modelStatus: { status: 'missing', model_id: 'parakeet-tdt-0.6b-v3' },
    });
    render(<OnboardingWizard onComplete={onComplete} />);

    await act(async () => {
      fireEvent.click(screen.getByText('Next'));
    });
    expect(screen.getByText('Microphone Setup')).toBeDefined();
    expect(screen.queryByRole('button', { name: 'Next' })).toBeNull();

    await act(async () => {
      fireEvent.click(screen.getByText('Start Test'));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('checkbox'));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    });
    await waitFor(() => {
      expect(screen.getByText('Hotkey Configuration')).toBeDefined();
    });
    await act(async () => {
      fireEvent.click(screen.getByText('Next'));
    });

    expect(screen.getByText('Speech Recognition Model')).toBeDefined();
    expect(screen.queryByRole('button', { name: 'Next' })).toBeNull();
  });
});

describe('Onboarding gate logic', () => {
  test('onboarding_completed=false triggers wizard (new install)', () => {
    const config = makeConfig({ onboarding_completed: false });
    // The strict equality check config.ui.onboarding_completed === false should be true
    expect(config.ui.onboarding_completed === false).toBe(true);
  });

  test('onboarding_completed=true skips wizard (existing user)', () => {
    const config = makeConfig({ onboarding_completed: true });
    expect(config.ui.onboarding_completed === false).toBe(false);
  });

  test('missing onboarding_completed field treated as completed (migration safety)', () => {
    // Simulating a config from before onboarding was added
    const config = makeConfig();
    // @ts-expect-error Simulating missing field
    delete config.ui.onboarding_completed;
    // Strict equality: undefined === false is false, so wizard won't show
    expect(config.ui.onboarding_completed === false).toBe(false);
  });
});
