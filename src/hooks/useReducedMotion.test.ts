import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import type { AppConfig } from '../types';
import { useAppStore } from '../store/appStore';
import { useReducedMotion } from './useReducedMotion';

function configWithReduceMotion(reduceMotion: boolean): AppConfig {
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
      onboarding_completed: true,
      overlay_enabled: false,
      locale: null,
      reduce_motion: reduceMotion,
    },
    history: { persistence_mode: 'memory', max_entries: 100, encrypt_at_rest: false },
    presets: { enabled_presets: [] },
  };
}

let changeHandler: ((event: MediaQueryListEvent) => void) | null = null;
let reducedMotionMatches = false;

function mockMatchMedia() {
  const mql = {
    get matches() {
      return reducedMotionMatches;
    },
    media: '(prefers-reduced-motion: reduce)',
    addEventListener: vi.fn((_event: string, handler: (event: MediaQueryListEvent) => void) => {
      changeHandler = handler;
    }),
    removeEventListener: vi.fn(() => {
      changeHandler = null;
    }),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    onchange: null,
    dispatchEvent: vi.fn(),
  };
  vi.stubGlobal('matchMedia', vi.fn(() => mql));
  return mql;
}

beforeEach(() => {
  reducedMotionMatches = false;
  changeHandler = null;
  mockMatchMedia();
  document.documentElement.classList.remove('reduce-motion');
  useAppStore.setState({ config: null });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('useReducedMotion', () => {
  test('applies class when config enables reduce motion', () => {
    useAppStore.setState({ config: configWithReduceMotion(true) });
    const { result } = renderHook(() => useReducedMotion());

    expect(result.current.enabled).toBe(true);
    expect(document.documentElement.classList.contains('reduce-motion')).toBe(true);
  });

  test('applies class when system prefers reduced motion', () => {
    reducedMotionMatches = true;
    mockMatchMedia();
    useAppStore.setState({ config: configWithReduceMotion(false) });

    const { result } = renderHook(() => useReducedMotion());
    expect(result.current.systemEnabled).toBe(true);
    expect(document.documentElement.classList.contains('reduce-motion')).toBe(true);
  });

  test('removes class when both config and system are disabled', () => {
    useAppStore.setState({ config: configWithReduceMotion(false) });
    const { result } = renderHook(() => useReducedMotion());

    expect(result.current.enabled).toBe(false);
    expect(document.documentElement.classList.contains('reduce-motion')).toBe(false);
  });

  test('responds to system reduced-motion preference changes', () => {
    reducedMotionMatches = false;
    mockMatchMedia();
    useAppStore.setState({ config: configWithReduceMotion(false) });

    const { result } = renderHook(() => useReducedMotion());
    expect(result.current.enabled).toBe(false);

    act(() => {
      changeHandler?.({ matches: true } as MediaQueryListEvent);
    });

    expect(document.documentElement.classList.contains('reduce-motion')).toBe(true);
  });
});
