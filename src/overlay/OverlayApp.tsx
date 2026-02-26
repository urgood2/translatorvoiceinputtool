import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useEffect, useMemo, useRef, useState } from 'react';
import type {
  TauriCommandGetAppStateResult,
  TauriEventAudioLevelPayload,
  TauriEventRecordingStatusPayload,
  TauriEventSidecarStatusPayload,
  TauriEventStateChangedPayload,
} from '../types.contracts';
import { RecordingPill } from './RecordingPill';
import { SessionTimer } from './SessionTimer';
import { Waveform } from './Waveform';

type RecordingPhase = 'idle' | 'recording' | 'transcribing';
type SidecarState = 'starting' | 'ready' | 'failed' | 'restarting' | 'stopped' | 'unknown';

const WAVEFORM_INTERVAL_MS = 67; // ~15Hz max waveform updates.

type OverlayTogglePayload = {
  enabled?: boolean;
};

function parseStartTime(value?: string): number | null {
  if (!value) {
    return null;
  }
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function normalizeAudioLevel(payload: TauriEventAudioLevelPayload): number {
  if (Number.isFinite(payload.rms) && payload.rms <= 0) {
    const linear = Math.pow(10, payload.rms / 20);
    return Math.max(0, Math.min(1, linear));
  }

  if (Number.isFinite(payload.peak)) {
    return Math.max(0, Math.min(1, payload.peak));
  }

  return 0;
}

function mapAppStateToPhase(state: TauriCommandGetAppStateResult['state']): RecordingPhase {
  if (state === 'recording') {
    return 'recording';
  }
  if (state === 'transcribing') {
    return 'transcribing';
  }
  return 'idle';
}

export function OverlayApp() {
  const [phase, setPhase] = useState<RecordingPhase>('idle');
  const [audioMs, setAudioMs] = useState(0);
  const [startedAtMs, setStartedAtMs] = useState<number | null>(null);
  const [waveLevel, setWaveLevel] = useState(0);
  const [overlayEnabled, setOverlayEnabled] = useState(true);
  const [documentVisible, setDocumentVisible] = useState(
    () => document.visibilityState !== 'hidden',
  );
  const [sidecarState, setSidecarState] = useState<SidecarState>('unknown');

  const shouldProcess = overlayEnabled && documentVisible;
  const shouldProcessRef = useRef(shouldProcess);
  const waveformActiveRef = useRef(false);
  const lastWaveformTickRef = useRef(0);

  useEffect(() => {
    shouldProcessRef.current = shouldProcess;
    waveformActiveRef.current = shouldProcess && (phase === 'recording' || phase === 'transcribing');

    if (!shouldProcess) {
      setWaveLevel(0);
    }
  }, [phase, shouldProcess]);

  useEffect(() => {
    const onVisibilityChange = () => {
      setDocumentVisible(document.visibilityState !== 'hidden');
    };

    document.addEventListener('visibilitychange', onVisibilityChange);
    return () => {
      document.removeEventListener('visibilitychange', onVisibilityChange);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    const refreshState = async () => {
      try {
        const appState = await invoke<TauriCommandGetAppStateResult>('get_app_state');
        if (cancelled) {
          return;
        }
        setPhase(mapAppStateToPhase(appState.state));
      } catch {
        // Overlay remains functional without immediate state snapshot.
      }
    };

    const setup = async () => {
      await refreshState();

      const subscribe = async <TPayload,>(
        eventName: string,
        handler: (payload: TPayload) => void,
      ) => {
        const unlisten = await listen<TPayload>(eventName, (event) => {
          handler(event.payload);
        });
        if (cancelled) {
          unlisten();
          return;
        }
        unlisteners.push(unlisten);
      };

      await subscribe<TauriEventRecordingStatusPayload>('recording:status', (payload) => {
        if (!shouldProcessRef.current) {
          return;
        }

        setPhase(payload.phase);
        setAudioMs(payload.audio_ms ?? 0);

        if (payload.phase === 'recording') {
          setStartedAtMs(parseStartTime(payload.started_at));
        } else {
          setStartedAtMs(null);
        }
      });

      await subscribe<TauriEventStateChangedPayload>('state:changed', (payload) => {
        if (!shouldProcessRef.current) {
          return;
        }
        setPhase(mapAppStateToPhase(payload.state));
      });

      await subscribe<TauriEventSidecarStatusPayload>('sidecar:status', (payload) => {
        if (!shouldProcessRef.current) {
          return;
        }
        setSidecarState(payload.state);
      });

      await subscribe<TauriEventAudioLevelPayload>('audio:level', (payload) => {
        if (!shouldProcessRef.current || !waveformActiveRef.current) {
          return;
        }

        const now = performance.now();
        if (now - lastWaveformTickRef.current < WAVEFORM_INTERVAL_MS) {
          return;
        }
        lastWaveformTickRef.current = now;
        setWaveLevel(normalizeAudioLevel(payload));
      });

      await subscribe<OverlayTogglePayload>('overlay:toggle', (payload) => {
        if (typeof payload.enabled === 'boolean') {
          setOverlayEnabled(payload.enabled);
        }
      });
    };

    void setup();

    return () => {
      cancelled = true;
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

  const overlayVisible = useMemo(() => shouldProcess, [shouldProcess]);

  if (!overlayVisible) {
    return null;
  }

  return (
    <div
      style={{
        alignItems: 'flex-end',
        display: 'flex',
        height: '100vh',
        justifyContent: 'center',
        padding: 16,
        pointerEvents: 'none',
        width: '100vw',
      }}
    >
      <RecordingPill
        phase={phase}
        sidecarState={sidecarState}
        timer={<SessionTimer phase={phase} audioMs={audioMs} startedAtMs={startedAtMs} />}
        waveform={<Waveform active={phase !== 'idle'} level={waveLevel} />}
      />
    </div>
  );
}
