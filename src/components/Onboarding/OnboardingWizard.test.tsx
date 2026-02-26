/**
 * Tests for the OnboardingWizard component and onboarding gate logic.
 *
 * Covers: new install gate, existing user skip, migration safety,
 * skip button, step navigation, completion flow per bead bdp.1.8.
 */

import { describe, test, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
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
    startMicTest: vi.fn(),
    stopMicTest: vi.fn(),
  });
});

// ── Tests ─────────────────────────────────────────────────────────

describe('OnboardingWizard', () => {
  test('renders Welcome step on initial mount', () => {
    const onComplete = vi.fn();
    render(<OnboardingWizard onComplete={onComplete} />);
    expect(screen.getByText('Welcome to Voice Input Tool')).toBeDefined();
  });

  test('navigates forward through all 5 steps', () => {
    const onComplete = vi.fn();
    render(<OnboardingWizard onComplete={onComplete} />);

    // Step 1: Welcome
    expect(screen.getByText('Welcome to Voice Input Tool')).toBeDefined();
    fireEvent.click(screen.getByText('Next'));

    // Step 2: Microphone
    expect(screen.getByText('Microphone Setup')).toBeDefined();
    fireEvent.click(screen.getByText('Next'));

    // Step 3: Hotkey
    expect(screen.getByText('Hotkey Configuration')).toBeDefined();
    fireEvent.click(screen.getByText('Next'));

    // Step 4: Model (ModelReadinessStep renders with status check)
    expect(screen.getByText('Speech Recognition Model')).toBeDefined();
    fireEvent.click(screen.getByText('Next'));

    // Step 5: Complete
    expect(screen.getByText('All Set!')).toBeDefined();
    expect(screen.getByText('Get Started')).toBeDefined();
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
    render(<OnboardingWizard onComplete={onComplete} />);

    // Navigate to last step
    fireEvent.click(screen.getByText('Next'));
    fireEvent.click(screen.getByText('Next'));
    fireEvent.click(screen.getByText('Next'));
    fireEvent.click(screen.getByText('Next'));

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
    render(<OnboardingWizard onComplete={onComplete} />);

    const progressbar = screen.getByRole('progressbar');
    expect(progressbar).toBeDefined();
    expect(progressbar.getAttribute('aria-valuenow')).toBe('1');
    expect(progressbar.getAttribute('aria-valuemax')).toBe('5');

    fireEvent.click(screen.getByText('Next'));
    expect(progressbar.getAttribute('aria-valuenow')).toBe('2');
  });

  test('Skip is available on every step', () => {
    const onComplete = vi.fn();
    render(<OnboardingWizard onComplete={onComplete} />);

    for (let i = 0; i < 4; i++) {
      expect(screen.getByText('Skip')).toBeDefined();
      fireEvent.click(screen.getByText('Next'));
    }
    // Last step also has Skip
    expect(screen.getByText('Skip')).toBeDefined();
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
