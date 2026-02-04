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
  ModelStatus,
  Progress,
  StateEvent,
  TranscriptEntry,
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

    const setupListeners = async () => {
      const store = useAppStore.getState();

      // Subscribe to app state changes
      const unlistenState = await listen<StateEvent>(
        EVENTS.STATE_CHANGED,
        (event) => {
          console.debug('Event: state_changed', event.payload);
          store._setAppState(event.payload);
        }
      );
      unlistenersRef.current.push(unlistenState);

      // Subscribe to model status changes
      const unlistenModelStatus = await listen<ModelStatus>(
        EVENTS.MODEL_STATUS,
        (event) => {
          console.debug('Event: model:status', event.payload);
          store._setModelStatus(event.payload);
        }
      );
      unlistenersRef.current.push(unlistenModelStatus);

      // Subscribe to model download progress
      const unlistenModelProgress = await listen<Progress>(
        EVENTS.MODEL_PROGRESS,
        (event) => {
          console.debug('Event: model:progress', event.payload);
          store._setDownloadProgress(event.payload);
        }
      );
      unlistenersRef.current.push(unlistenModelProgress);

      // Subscribe to audio level updates (during mic test)
      const unlistenAudioLevel = await listen<AudioLevelEvent>(
        EVENTS.AUDIO_LEVEL,
        (event) => {
          // Don't log audio levels - too noisy
          store._setAudioLevel(event.payload);
        }
      );
      unlistenersRef.current.push(unlistenAudioLevel);

      // Subscribe to transcript completions
      const unlistenTranscript = await listen<{ entry: TranscriptEntry }>(
        EVENTS.TRANSCRIPT_COMPLETE,
        (event) => {
          console.debug('Event: transcript:complete', event.payload);
          store._addHistoryEntry(event.payload.entry);
        }
      );
      unlistenersRef.current.push(unlistenTranscript);

      // Subscribe to error events
      const unlistenError = await listen<{ message: string; recoverable: boolean }>(
        EVENTS.ERROR,
        (event) => {
          console.error('Event: app:error', event.payload);
          store._setError(event.payload.message);
        }
      );
      unlistenersRef.current.push(unlistenError);

      // Subscribe to sidecar status changes
      const unlistenSidecar = await listen<{
        state: string;
        restart_count: number;
        message?: string;
      }>(EVENTS.SIDECAR_STATUS, (event) => {
        console.debug('Event: sidecar:status', event.payload);
        // Could update a dedicated sidecar state slice if needed
      });
      unlistenersRef.current.push(unlistenSidecar);

      console.log('Tauri event listeners set up');
    };

    setupListeners().catch((error) => {
      console.error('Failed to set up Tauri event listeners:', error);
    });

    // Cleanup on unmount
    return () => {
      console.log('Cleaning up Tauri event listeners');
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
    let unlisten: UnlistenFn | null = null;

    listen<T>(eventName, (event) => {
      handlerRef.current(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, [eventName]);
}
