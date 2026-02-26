/**
 * Tests for the useTheme hook.
 *
 * Covers: theme resolution, class application, system preference
 * tracking, and edge cases per bead bdp.2.5.
 */

import { describe, test, expect, beforeEach, afterEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useTheme, resolveTheme } from './useTheme';
import { useAppStore } from '../store/appStore';
import type { AppConfig } from '../types';

// ── Helpers ───────────────────────────────────────────────────────

/** Minimal config with a given theme. */
function configWithTheme(theme: 'system' | 'light' | 'dark'): AppConfig {
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
      theme,
      onboarding_completed: true,
      overlay_enabled: false,
      locale: null,
      reduce_motion: false,
    },
    history: { persistence_mode: 'memory', max_entries: 100, encrypt_at_rest: false },
    presets: { enabled_presets: [] },
  };
}

let changeHandler: ((e: MediaQueryListEvent) => void) | null = null;
let darkMatches = false;

function mockMatchMedia() {
  const mql = {
    get matches() { return darkMatches; },
    media: '(prefers-color-scheme: dark)',
    addEventListener: vi.fn((_event: string, handler: (e: MediaQueryListEvent) => void) => {
      changeHandler = handler;
    }),
    removeEventListener: vi.fn((_event: string, _handler: (e: MediaQueryListEvent) => void) => {
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

// ── Setup / Teardown ──────────────────────────────────────────────

beforeEach(() => {
  document.documentElement.classList.remove('dark');
  darkMatches = false;
  changeHandler = null;
  mockMatchMedia();
  useAppStore.setState({ config: null });
});

afterEach(() => {
  vi.restoreAllMocks();
});

// ── Tests ─────────────────────────────────────────────────────────

describe('resolveTheme (pure)', () => {
  test('returns "dark" for dark preference', () => {
    expect(resolveTheme('dark')).toBe('dark');
  });

  test('returns "light" for light preference', () => {
    expect(resolveTheme('light')).toBe('light');
  });

  test('returns system theme for system preference', () => {
    darkMatches = true;
    mockMatchMedia();
    expect(resolveTheme('system')).toBe('dark');

    darkMatches = false;
    mockMatchMedia();
    expect(resolveTheme('system')).toBe('light');
  });
});

describe('useTheme', () => {
  test('defaults to system when config is null', () => {
    const { result } = renderHook(() => useTheme());
    expect(result.current.preference).toBe('system');
  });

  test('light mode removes dark class from document', () => {
    document.documentElement.classList.add('dark');
    useAppStore.setState({ config: configWithTheme('light') });
    renderHook(() => useTheme());
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  test('dark mode adds dark class to document', () => {
    useAppStore.setState({ config: configWithTheme('dark') });
    renderHook(() => useTheme());
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  test('system mode follows OS preference (light)', () => {
    darkMatches = false;
    mockMatchMedia();
    useAppStore.setState({ config: configWithTheme('system') });
    renderHook(() => useTheme());
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  test('system mode follows OS preference (dark)', () => {
    darkMatches = true;
    mockMatchMedia();
    useAppStore.setState({ config: configWithTheme('system') });
    renderHook(() => useTheme());
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  test('OS preference change triggers theme update when mode is system', () => {
    darkMatches = false;
    mockMatchMedia();
    useAppStore.setState({ config: configWithTheme('system') });
    renderHook(() => useTheme());
    expect(document.documentElement.classList.contains('dark')).toBe(false);

    // Simulate OS switching to dark
    act(() => {
      if (changeHandler) {
        changeHandler({ matches: true } as MediaQueryListEvent);
      }
    });
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  test('OS preference change ignored when mode is not system', () => {
    useAppStore.setState({ config: configWithTheme('light') });
    renderHook(() => useTheme());
    expect(changeHandler).toBeNull();
  });

  test('live toggle: changing config theme immediately applies', () => {
    useAppStore.setState({ config: configWithTheme('light') });
    const { rerender } = renderHook(() => useTheme());
    expect(document.documentElement.classList.contains('dark')).toBe(false);

    // Switch to dark
    act(() => {
      useAppStore.setState({ config: configWithTheme('dark') });
    });
    rerender();
    expect(document.documentElement.classList.contains('dark')).toBe(true);

    // Switch back to light
    act(() => {
      useAppStore.setState({ config: configWithTheme('light') });
    });
    rerender();
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  test('returns correct preference and resolved values', () => {
    useAppStore.setState({ config: configWithTheme('dark') });
    const { result } = renderHook(() => useTheme());
    expect(result.current.preference).toBe('dark');
    expect(result.current.resolved).toBe('dark');
  });
});
