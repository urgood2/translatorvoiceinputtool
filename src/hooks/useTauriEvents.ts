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
  ErrorEvent,
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
  STATE_CHANGED: 'state:changed',
  STATE_CHANGED_LEGACY: 'state_changed',

  // Model events
  MODEL_STATUS: 'model:status',
  MODEL_PROGRESS: 'model:progress',

  // Audio events
  AUDIO_LEVEL: 'audio:level',

  // Transcript events
  TRANSCRIPT_COMPLETE: 'transcript:complete',
  TRANSCRIPT_COMPLETE_LEGACY: 'transcription:complete',
  TRANSCRIPT_ERROR: 'transcript:error',
  TRANSCRIPT_ERROR_LEGACY: 'transcription:error',

  // Error events
  APP_ERROR: 'app:error',

  // Sidecar events
  SIDECAR_STATUS: 'sidecar:status',
  SIDECAR_STATUS_LEGACY: 'status:changed',

  // Recording events
  RECORDING_STATUS: 'recording:status',
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
    const rawText =
      typeof payload.entry.raw_text === 'string' && payload.entry.raw_text.length > 0
        ? payload.entry.raw_text
        : payload.entry.text;
    const finalText =
      typeof payload.entry.final_text === 'string' && payload.entry.final_text.length > 0
        ? payload.entry.final_text
        : payload.entry.text;
    return {
      ...payload.entry,
      text: finalText,
      raw_text: rawText,
      final_text: finalText,
    };
  }

  const rawText =
    typeof payload.raw_text === 'string' && payload.raw_text.length > 0
      ? payload.raw_text
      : payload.text;
  const finalText =
    typeof payload.final_text === 'string' && payload.final_text.length > 0
      ? payload.final_text
      : payload.text;

  return {
    id: payload.session_id,
    text: finalText,
    raw_text: rawText,
    final_text: finalText,
    timestamp: new Date().toISOString(),
    audio_duration_ms: payload.audio_duration_ms,
    transcription_duration_ms: payload.processing_duration_ms,
    session_id: payload.session_id,
    language: typeof payload.language === 'string' ? payload.language : undefined,
    confidence: typeof payload.confidence === 'number' ? payload.confidence : undefined,
    injection_result: normalizeInjectionResult(payload.injection_result),
    timings: payload.timings,
  };
}

function extractSeq(payload: unknown): number | undefined {
  if (!payload || typeof payload !== 'object') {
    return undefined;
  }

  const seq = (payload as { seq?: unknown }).seq;
  if (typeof seq === 'number' && Number.isFinite(seq)) {
    return seq;
  }
  return undefined;
}

function shouldProcessWithSeqDedupe(
  streamKey: string,
  payload: unknown,
  lastSeqByStream: Map<string, number>
): boolean {
  const seq = extractSeq(payload);
  if (seq === undefined) {
    return true;
  }

  const previous = lastSeqByStream.get(streamKey);
  if (previous !== undefined && seq <= previous) {
    return false;
  }
  lastSeqByStream.set(streamKey, seq);
  return true;
}

function errorMessageFromPayload(payload: ErrorEvent): string {
  if (typeof payload.message === 'string' && payload.message.length > 0) {
    return payload.message;
  }
  if (typeof payload.error === 'string' && payload.error.length > 0) {
    return payload.error;
  }
  if (
    payload.error &&
    typeof payload.error === 'object' &&
    'message' in payload.error &&
    typeof payload.error.message === 'string'
  ) {
    return payload.error.message;
  }
  if (
    payload.app_error &&
    typeof payload.app_error === 'object' &&
    'message' in payload.app_error &&
    typeof payload.app_error.message === 'string'
  ) {
    return payload.app_error.message;
  }
  return 'Unknown error';
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
      const lastSeqByStream = new Map<string, number>();

      const dedupeHandler = <T,>(
        streamKey: string,
        handler: (payload: T) => void
      ) => {
        return (event: { payload: T }) => {
          if (!shouldProcessWithSeqDedupe(streamKey, event.payload, lastSeqByStream)) {
            return;
          }
          handler(event.payload);
        };
      };

      // Subscribe to app state changes
      const stateRegistered = await registerListener<StateEvent>(
        EVENTS.STATE_CHANGED,
        dedupeHandler('state:changed', (payload) => {
          console.debug('Event: state:changed', payload);
          store._setAppState(payload);
        })
      );
      if (!stateRegistered) return;
      const stateLegacyRegistered = await registerListener<StateEvent>(
        EVENTS.STATE_CHANGED_LEGACY,
        dedupeHandler('state:changed', (payload) => {
          console.debug('Event: state_changed', payload);
          store._setAppState(payload);
        })
      );
      if (!stateLegacyRegistered) return;

      // Subscribe to model status changes
      const modelStatusRegistered = await registerListener<ModelStatus>(
        EVENTS.MODEL_STATUS,
        dedupeHandler('model:status', (payload) => {
          console.debug('Event: model:status', payload);
          store._setModelStatus(payload);
        })
      );
      if (!modelStatusRegistered) return;

      // Subscribe to model download progress
      const modelProgressRegistered = await registerListener<Progress>(
        EVENTS.MODEL_PROGRESS,
        dedupeHandler('model:progress', (payload) => {
          console.debug('Event: model:progress', payload);
          store._setDownloadProgress(payload);
        })
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
        dedupeHandler('transcript:complete', (payload) => {
          console.debug('Event: transcript:complete', payload);
          store._addHistoryEntry(normalizeTranscriptPayload(payload));
        })
      );
      if (!transcriptRegistered) return;
      const transcriptLegacyRegistered = await registerListener<TranscriptEventPayload>(
        EVENTS.TRANSCRIPT_COMPLETE_LEGACY,
        dedupeHandler('transcript:complete', (payload) => {
          console.debug('Event: transcription:complete', payload);
          store._addHistoryEntry(normalizeTranscriptPayload(payload));
        })
      );
      if (!transcriptLegacyRegistered) return;

      // Subscribe to error events
      const appErrorRegistered = await registerListener<ErrorEvent>(
        EVENTS.APP_ERROR,
        dedupeHandler('app:error', (payload) => {
          console.error('Event: app:error', payload);
          store._setError(errorMessageFromPayload(payload));
        })
      );
      if (!appErrorRegistered) return;

      const transcriptErrorRegistered = await registerListener<ErrorEvent>(
        EVENTS.TRANSCRIPT_ERROR,
        dedupeHandler('transcript:error', (payload) => {
          console.error('Event: transcript:error', payload);
          store._setError(errorMessageFromPayload(payload));
        })
      );
      if (!transcriptErrorRegistered) return;
      const transcriptErrorLegacyRegistered = await registerListener<ErrorEvent>(
        EVENTS.TRANSCRIPT_ERROR_LEGACY,
        dedupeHandler('transcript:error', (payload) => {
          console.error('Event: transcription:error', payload);
          store._setError(errorMessageFromPayload(payload));
        })
      );
      if (!transcriptErrorLegacyRegistered) return;

      // Subscribe to sidecar status changes
      const sidecarRegistered = await registerListener<{
        state: string;
        restart_count: number;
        message?: string;
      }>(
        EVENTS.SIDECAR_STATUS,
        dedupeHandler('sidecar:status', (payload) => {
          console.debug('Event: sidecar:status', payload);
          // Could update a dedicated sidecar state slice if needed.
        })
      );
      if (!sidecarRegistered) return;
      const sidecarLegacyRegistered = await registerListener<Record<string, unknown>>(
        EVENTS.SIDECAR_STATUS_LEGACY,
        dedupeHandler('sidecar:status', (payload) => {
          console.debug('Event: status:changed', payload);
        })
      );
      if (!sidecarLegacyRegistered) return;

      // Subscribe to recording status updates (new stream; optional producer).
      const recordingStatusRegistered = await registerListener<Record<string, unknown>>(
        EVENTS.RECORDING_STATUS,
        dedupeHandler('recording:status', (payload) => {
          console.debug('Event: recording:status', payload);
        })
      );
      if (!recordingStatusRegistered) return;

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
