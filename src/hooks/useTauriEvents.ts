/**
 * Hook for subscribing to Tauri events from the Rust backend.
 *
 * Sets up event listeners on mount and cleans them up on unmount.
 * Events update the Zustand store directly via internal actions.
 */

import { useEffect, useRef } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useAppStore } from '../store/appStore';
import { createSeqDedupeTracker, type DedupeStreamKey } from '../utils/dedupeTracker';
import type {
  AudioLevelEvent,
  ErrorEvent,
  InjectionResult,
  Progress,
  RecordingStatusEvent,
  SidecarStatusEvent,
  StateEventPayload,
  TranscriptEntry,
  TranscriptEventPayload,
  ModelStatusPayload,
} from '../types';

// Event names emitted by Rust backend
const EVENTS = {
  // App state changes
  STATE_CHANGED: 'state:changed',

  // Model events
  MODEL_STATUS: 'model:status',
  MODEL_PROGRESS: 'model:progress',

  // Audio events
  AUDIO_LEVEL: 'audio:level',

  // Transcript events
  TRANSCRIPT_COMPLETE: 'transcript:complete',
  TRANSCRIPT_ERROR: 'transcript:error',

  // Error events
  APP_ERROR: 'app:error',

  // Sidecar events
  SIDECAR_STATUS: 'sidecar:status',

  // Recording events
  RECORDING_STATUS: 'recording:status',
} as const;

const STREAM_KEYS: Record<string, DedupeStreamKey> = {
  STATE: 'state',
  TRANSCRIPT: 'transcript',
  TRANSCRIPT_ERROR: 'transcriptError',
  SIDECAR: 'sidecar',
  MODEL: 'model',
  RECORDING: 'recording',
  AUDIO: 'audio',
  ERROR: 'error',
} as const;

const TRANSCRIPT_PAYLOAD_DEBUG_LOG_KEY = 'openvoicy.debug.transcript_payload_logs';

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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isTranscriptPayloadDebugLoggingEnabled(): boolean {
  try {
    if (typeof window === 'undefined' || !window.localStorage) {
      return false;
    }
    const flag = window.localStorage.getItem(TRANSCRIPT_PAYLOAD_DEBUG_LOG_KEY);
    return flag === '1' || flag === 'true';
  } catch {
    return false;
  }
}

function readNumericField(record: Record<string, unknown>, key: string): number | undefined {
  const value = record[key];
  return typeof value === 'number' ? value : undefined;
}

function transcriptPayloadMetadata(payload: unknown): Record<string, unknown> {
  if (!isRecord(payload)) {
    return { payload_type: typeof payload };
  }

  if ('entry' in payload && isRecord(payload.entry)) {
    const entry = payload.entry;
    const text =
      typeof entry.final_text === 'string'
        ? entry.final_text
        : typeof entry.text === 'string'
          ? entry.text
          : typeof entry.raw_text === 'string'
            ? entry.raw_text
            : '';
    return {
      session_id: typeof entry.session_id === 'string' ? entry.session_id : undefined,
      entry_id: typeof entry.id === 'string' ? entry.id : undefined,
      seq: readNumericField(payload, 'seq'),
      text_length: text.length,
      has_timings: isRecord(entry.timings),
      has_injection_result: isRecord(entry.injection_result),
    };
  }

  const text =
    typeof payload.final_text === 'string'
      ? payload.final_text
      : typeof payload.text === 'string'
        ? payload.text
        : typeof payload.raw_text === 'string'
          ? payload.raw_text
          : '';
  return {
    session_id: typeof payload.session_id === 'string' ? payload.session_id : undefined,
    seq: readNumericField(payload, 'seq'),
    text_length: text.length,
    has_timings: isRecord(payload.timings),
    has_injection_result: isRecord(payload.injection_result),
  };
}

function transcriptErrorMetadata(payload: unknown): Record<string, unknown> {
  if (!isRecord(payload)) {
    return { payload_type: typeof payload };
  }

  const appError = isRecord(payload.app_error)
    ? payload.app_error
    : isRecord(payload.error)
      ? payload.error
      : null;
  return {
    session_id: typeof payload.session_id === 'string' ? payload.session_id : undefined,
    seq: readNumericField(payload, 'seq'),
    recoverable: typeof payload.recoverable === 'boolean' ? payload.recoverable : undefined,
    error_code: appError && typeof appError.code === 'string' ? appError.code : undefined,
    has_message:
      typeof payload.message === 'string'
      || typeof payload.error === 'string'
      || (appError !== null && typeof appError.message === 'string'),
  };
}

function normalizeTranscriptPayload(payload: unknown): TranscriptEntry | null {
  if (!isRecord(payload)) {
    return null;
  }

  if ('entry' in payload && isRecord(payload.entry)) {
    const entry = payload.entry;
    const entryText = typeof entry.text === 'string' ? entry.text : null;
    if (!entryText) {
      return null;
    }
    const rawText =
      typeof entry.raw_text === 'string' && entry.raw_text.length > 0
        ? entry.raw_text
        : entryText;
    const finalText =
      typeof entry.final_text === 'string' && entry.final_text.length > 0
        ? entry.final_text
        : entryText;
    const entryId =
      typeof entry.id === 'string' && entry.id.length > 0
        ? entry.id
        : typeof entry.session_id === 'string' && entry.session_id.length > 0
          ? entry.session_id
          : 'unknown-session';
    const timestamp =
      typeof entry.timestamp === 'string' && entry.timestamp.length > 0
        ? entry.timestamp
        : new Date().toISOString();
    return {
      ...entry,
      id: entryId,
      text: finalText,
      raw_text: rawText,
      final_text: finalText,
      timestamp,
      audio_duration_ms:
        typeof entry.audio_duration_ms === 'number' ? entry.audio_duration_ms : 0,
      transcription_duration_ms:
        typeof entry.transcription_duration_ms === 'number'
          ? entry.transcription_duration_ms
          : 0,
      injection_result: normalizeInjectionResult(entry.injection_result),
    };
  }

  const payloadText = typeof payload.text === 'string' ? payload.text : null;
  if (!payloadText) {
    return null;
  }
  const sessionId =
    typeof payload.session_id === 'string' && payload.session_id.length > 0
      ? payload.session_id
      : 'unknown-session';
  const rawText =
    typeof payload.raw_text === 'string' && payload.raw_text.length > 0
      ? payload.raw_text
      : payloadText;
  const finalText =
    typeof payload.final_text === 'string' && payload.final_text.length > 0
      ? payload.final_text
      : payloadText;

  return {
    id: sessionId,
    text: finalText,
    raw_text: rawText,
    final_text: finalText,
    timestamp: new Date().toISOString(),
    audio_duration_ms:
      typeof payload.audio_duration_ms === 'number' ? payload.audio_duration_ms : 0,
    transcription_duration_ms:
      typeof payload.processing_duration_ms === 'number'
        ? payload.processing_duration_ms
        : 0,
    session_id: sessionId,
    language: typeof payload.language === 'string' ? payload.language : undefined,
    confidence: typeof payload.confidence === 'number' ? payload.confidence : undefined,
    injection_result: normalizeInjectionResult(payload.injection_result),
    timings: payload.timings,
  };
}

function isRecordingStatusEvent(payload: unknown): payload is RecordingStatusEvent {
  return (
    typeof payload === 'object'
    && payload !== null
    && 'phase' in payload
    && typeof payload.phase === 'string'
  );
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
    const dedupeTracker = createSeqDedupeTracker();

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

      const dedupeHandler = <T,>(
        streamKey: DedupeStreamKey,
        handler: (payload: T) => void
      ) => {
        return (event: { payload: T }) => {
          if (!dedupeTracker.shouldProcess(streamKey, event.payload)) {
            return;
          }
          handler(event.payload);
        };
      };

      // Subscribe to app state changes
      const stateRegistered = await registerListener<StateEventPayload>(
        EVENTS.STATE_CHANGED,
        dedupeHandler(STREAM_KEYS.STATE, (payload) => {
          console.debug('Event: state:changed', payload);
          store._setAppState(payload);
        })
      );
      if (!stateRegistered) return;

      // Subscribe to model status changes
      const modelStatusRegistered = await registerListener<ModelStatusPayload>(
        EVENTS.MODEL_STATUS,
        dedupeHandler(STREAM_KEYS.MODEL, (payload) => {
          console.debug('Event: model:status', payload);
          store._setModelStatus(payload);
        })
      );
      if (!modelStatusRegistered) return;

      // Subscribe to model download progress
      const modelProgressRegistered = await registerListener<Progress>(
        EVENTS.MODEL_PROGRESS,
        dedupeHandler(STREAM_KEYS.MODEL, (payload) => {
          console.debug('Event: model:progress', payload);
          store._setDownloadProgress(payload);
        })
      );
      if (!modelProgressRegistered) return;

      // Subscribe to audio level updates (during mic test)
      const audioLevelRegistered = await registerListener<AudioLevelEvent>(
        EVENTS.AUDIO_LEVEL,
        dedupeHandler(STREAM_KEYS.AUDIO, (payload) => {
          // Don't log audio levels - too noisy
          store._setAudioLevel(payload);
        })
      );
      if (!audioLevelRegistered) return;

      // Subscribe to transcript completions
      const transcriptRegistered = await registerListener<TranscriptEventPayload>(
        EVENTS.TRANSCRIPT_COMPLETE,
        dedupeHandler(STREAM_KEYS.TRANSCRIPT, (payload) => {
          console.debug('Event: transcript:complete', transcriptPayloadMetadata(payload));
          if (isTranscriptPayloadDebugLoggingEnabled()) {
            console.debug('Event: transcript:complete:payload', payload);
          }
          const normalized = normalizeTranscriptPayload(payload);
          if (!normalized) {
            console.warn('Ignoring malformed transcript payload', transcriptPayloadMetadata(payload));
            return;
          }
          store._addHistoryEntry(normalized);
        })
      );
      if (!transcriptRegistered) return;

      // Subscribe to error events
      const appErrorRegistered = await registerListener<ErrorEvent>(
        EVENTS.APP_ERROR,
        dedupeHandler(STREAM_KEYS.ERROR, (payload) => {
          console.error('Event: app:error', payload);
          store._setError(payload);
        })
      );
      if (!appErrorRegistered) return;

      const transcriptErrorRegistered = await registerListener<ErrorEvent>(
        EVENTS.TRANSCRIPT_ERROR,
        dedupeHandler(STREAM_KEYS.TRANSCRIPT_ERROR, (payload) => {
          console.error('Event: transcript:error', transcriptErrorMetadata(payload));
          if (isTranscriptPayloadDebugLoggingEnabled()) {
            console.debug('Event: transcript:error:payload', payload);
          }
          store._setTranscriptError(payload);
        })
      );
      if (!transcriptErrorRegistered) return;

      // Subscribe to sidecar status changes
      const sidecarRegistered = await registerListener<SidecarStatusEvent>(
        EVENTS.SIDECAR_STATUS,
        dedupeHandler(STREAM_KEYS.SIDECAR, (payload) => {
          console.debug('Event: sidecar:status', payload);
          store._setSidecarStatus(payload);
        })
      );
      if (!sidecarRegistered) return;

      // Subscribe to recording status updates (new stream; optional producer).
      const recordingStatusRegistered = await registerListener<Record<string, unknown>>(
        EVENTS.RECORDING_STATUS,
        dedupeHandler(STREAM_KEYS.RECORDING, (payload) => {
          console.debug('Event: recording:status', payload);
          if (isRecordingStatusEvent(payload)) {
            store._setRecordingStatus(payload);
          }
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
      dedupeTracker.reset();
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
