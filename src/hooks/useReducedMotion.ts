import { useEffect, useMemo, useState } from 'react';
import { useAppStore, selectConfig } from '../store/appStore';

const MEDIA_QUERY = '(prefers-reduced-motion: reduce)';
const ROOT_CLASS = 'reduce-motion';

function getSystemReducedMotion(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return false;
  }
  return window.matchMedia(MEDIA_QUERY).matches;
}

function applyReduceMotionClass(enabled: boolean): void {
  const root = document.documentElement;
  if (enabled) {
    root.classList.add(ROOT_CLASS);
  } else {
    root.classList.remove(ROOT_CLASS);
  }
}

export function useReducedMotion() {
  const config = useAppStore(selectConfig);
  const configEnabled = Boolean(config?.ui.reduce_motion);
  const [systemEnabled, setSystemEnabled] = useState<boolean>(() => getSystemReducedMotion());

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return;
    }

    const media = window.matchMedia(MEDIA_QUERY);
    const handler = (event: MediaQueryListEvent) => {
      setSystemEnabled(event.matches);
    };
    setSystemEnabled(media.matches);

    if (typeof media.addEventListener === 'function') {
      media.addEventListener('change', handler);
      return () => media.removeEventListener('change', handler);
    }

    media.addListener(handler);
    return () => media.removeListener(handler);
  }, []);

  const enabled = useMemo(() => configEnabled || systemEnabled, [configEnabled, systemEnabled]);

  useEffect(() => {
    applyReduceMotionClass(enabled);
  }, [enabled]);

  return { enabled, configEnabled, systemEnabled } as const;
}
