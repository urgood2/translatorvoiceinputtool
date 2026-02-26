/**
 * Theme hook — reads ui.theme from config store and applies the
 * appropriate class to the document root for Tailwind dark mode.
 *
 * Supports three modes:
 *   - "light"  → removes "dark" class
 *   - "dark"   → adds "dark" class
 *   - "system" → follows OS prefers-color-scheme, updating reactively
 */

import { useEffect, useMemo } from 'react';
import { useAppStore, selectConfig } from '../store/appStore';

export type ThemePreference = 'system' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';

function getSystemTheme(): ResolvedTheme {
  if (typeof window === 'undefined') return 'light';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function applyTheme(resolved: ResolvedTheme): void {
  const root = document.documentElement;
  if (resolved === 'dark') {
    root.classList.add('dark');
  } else {
    root.classList.remove('dark');
  }
}

export function resolveTheme(preference: ThemePreference): ResolvedTheme {
  if (preference === 'dark') return 'dark';
  if (preference === 'light') return 'light';
  return getSystemTheme();
}

export function useTheme() {
  const config = useAppStore(selectConfig);
  const preference: ThemePreference = config?.ui.theme ?? 'system';

  // Resolve theme, accounting for system preference
  const resolved = useMemo(() => resolveTheme(preference), [preference]);

  // Apply the class on mount and when preference changes
  useEffect(() => {
    applyTheme(resolveTheme(preference));
  }, [preference]);

  // Listen for OS color-scheme changes when mode is "system"
  useEffect(() => {
    if (preference !== 'system') return;

    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => {
      applyTheme(e.matches ? 'dark' : 'light');
    };
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, [preference]);

  return { preference, resolved } as const;
}
