/**
 * Hook for subscribing to Tauri events from the Rust backend.
 *
 * Sets up event listeners on mount and cleans them up on unmount.
 * Events update the Zustand store directly via internal actions.
 */

import { useEffect, useRef } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useAppStore } from '../store/appStore';
import type {
  AudioLevelEvent,
  InjectionResult,
  ModelStatus,
  Progress,
  StateEvent,
  TranscriptEntry,
  TranscriptEventPayload,
} from '../types';

// Event names emitted by Rust backend
const EVENTS = {
  // App state changes
  STATE_CHANGED: 'state_changed',

  // Model events
  MODEL_STATUS: 'model:status',
  MODEL_PROGRESS: 'model:progress',

  // Audio events
  AUDIO_LEVEL: 'audio:level',

  // Transcript events
  TRANSCRIPT_COMPLETE: 'transcript:complete',

  // Error events
  ERROR: 'app:error',

  // Sidecar events
  SIDECAR_STATUS: 'sidecar:status',
} as const;

function defaultInjectionResult(): InjectionResult {
  return { status: 'injected' };
}

function normalizeInjectionResult(result: unknown): InjectionResult {
  if (!result || typeof result !== 'object') {
    return defaultInjectionResult();
  }

  const payload = result as Record<string, unknown>;
  const status = payload.status;
  if (status === 'injected') {
    return { status: 'injected' };
  }
  if (status === 'clipboard_only') {
    return {
      status: 'clipboard_only',
      reason:
        typeof payload.reason === 'string' && payload.reason.length > 0
          ? payload.reason
          : 'clipboard_only',
    };
  }
  if (status === 'error') {
    return {
      status: 'error',
      message:
        typeof payload.message === 'string' && payload.message.length > 0
          ? payload.message
          : 'injection error',
    };
  }
  if (status === 'failed') {
    return {
      status: 'error',
      message:
        typeof payload.error === 'string' && payload.error.length > 0
          ? payload.error
          : 'injection failed',
    };
  }

  return defaultInjectionResult();
}

function normalizeTranscriptPayload(payload: TranscriptEventPayload): TranscriptEntry {
  if ('entry' in payload) {
    return payload.entry;
  }

  return {
    id: payload.session_id,
    text: payload.text,
    timestamp: new Date().toISOString(),
    audio_duration_ms: payload.audio_duration_ms,
    transcription_duration_ms: payload.processing_duration_ms,
    injection_result: normalizeInjectionResult(payload.injection_result),
    timings: payload.timings,
  };
}

/**
 * Hook that subscribes to all Tauri events and updates the store.
 *
 * Should be called once at the app root level (e.g., in App.tsx).
 */
export function useTauriEvents(): void {
  const unlistenersRef = useRef<UnlistenFn[]>([]);
  const isSetupRef = useRef(false);

  useEffect(() => {
    // Prevent double setup in StrictMode
    if (isSetupRef.current) return;
    isSetupRef.current = true;
    let cancelled = false;

    const registerListener = async <T>(
      eventName: string,
      onEvent: Parameters<typeof listen<T>>[1]
    ): Promise<boolean> => {
      const unlisten = await listen<T>(eventName, onEvent);
      if (cancelled) {
        // Component unmounted before async listener setup completed.
        unlisten();
        return false;
      }
      unlistenersRef.current.push(unlisten);
      return true;
    };

    const setupListeners = async () => {
      const store = useAppStore.getState();

      // Subscribe to app state changes
      const stateRegistered = await registerListener<StateEvent>(
        EVENTS.STATE_CHANGED,
        (event) => {
          console.debug('Event: state_changed', event.payload);
          store._setAppState(event.payload);
        }
      );
      if (!stateRegistered) return;

      // Subscribe to model status changes
      const modelStatusRegistered = await registerListener<ModelStatus>(
        EVENTS.MODEL_STATUS,
        (event) => {
          console.debug('Event: model:status', event.payload);
          store._setModelStatus(event.payload);
        }
      );
      if (!modelStatusRegistered) return;

      // Subscribe to model download progress
      const modelProgressRegistered = await registerListener<Progress>(
        EVENTS.MODEL_PROGRESS,
        (event) => {
          console.debug('Event: model:progress', event.payload);
          store._setDownloadProgress(event.payload);
        }
      );
      if (!modelProgressRegistered) return;

      // Subscribe to audio level updates (during mic test)
      const audioLevelRegistered = await registerListener<AudioLevelEvent>(
        EVENTS.AUDIO_LEVEL,
        (event) => {
          // Don't log audio levels - too noisy
          store._setAudioLevel(event.payload);
        }
      );
      if (!audioLevelRegistered) return;

      // Subscribe to transcript completions
      const transcriptRegistered = await registerListener<TranscriptEventPayload>(
        EVENTS.TRANSCRIPT_COMPLETE,
        (event) => {
          console.debug('Event: transcript:complete', event.payload);
          store._addHistoryEntry(normalizeTranscriptPayload(event.payload));
        }
      );
      if (!transcriptRegistered) return;

      // Subscribe to error events
      const errorRegistered = await registerListener<{ message: string; recoverable: boolean }>(
        EVENTS.ERROR,
        (event) => {
          console.error('Event: app:error', event.payload);
          store._setError(event.payload.message);
        }
      );
      if (!errorRegistered) return;

      // Subscribe to sidecar status changes
      const sidecarRegistered = await registerListener<{
        state: string;
        restart_count: number;
        message?: string;
      }>(EVENTS.SIDECAR_STATUS, (event) => {
        console.debug('Event: sidecar:status', event.payload);
        // Could update a dedicated sidecar state slice if needed
      });
      if (!sidecarRegistered) return;

      console.log('Tauri event listeners set up');
    };

    setupListeners().catch((error) => {
      console.error('Failed to set up Tauri event listeners:', error);
    });

    // Cleanup on unmount
    return () => {
      console.log('Cleaning up Tauri event listeners');
      cancelled = true;
      unlistenersRef.current.forEach((unlisten) => unlisten());
      unlistenersRef.current = [];
      isSetupRef.current = false;
    };
  }, []);
}

/**
 * Hook for subscribing to a specific Tauri event.
 *
 * Useful for components that need custom event handling beyond
 * what the store provides.
 */
export function useTauriEvent<T>(
  eventName: string,
  handler: (payload: T) => void
): void {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | null = null;

    void listen<T>(eventName, (event) => {
      handlerRef.current(event.payload);
    }).then((fn) => {
      if (cancelled) {
        // Component unmounted before async listen resolved: clean up immediately.
        fn();
        return;
      }
      unlisten = fn;
    }).catch((error) => {
      console.error(`Failed to listen for event ${eventName}:`, error);
    });

    return () => {
      cancelled = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, [eventName]);
}
