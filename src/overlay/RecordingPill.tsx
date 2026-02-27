import { useEffect, useState, type ReactNode } from 'react';

type RecordingPhase = 'idle' | 'recording' | 'transcribing';
type SidecarState = 'starting' | 'ready' | 'failed' | 'restarting' | 'stopped' | 'unknown';

interface Palette {
  bg: string;
  border: string;
  dot: string;
  text: string;
}

const PHASE_PALETTE: Record<Exclude<RecordingPhase, 'idle'>, Palette> = {
  recording: {
    bg: 'rgba(53, 9, 9, 0.9)',
    border: 'rgba(235, 76, 76, 0.82)',
    dot: '#ff4d4d',
    text: '#ffeaea',
  },
  transcribing: {
    bg: 'rgba(77, 58, 8, 0.88)',
    border: 'rgba(232, 187, 56, 0.78)',
    dot: '#ffd659',
    text: '#fff8e1',
  },
};

function phaseLabel(phase: RecordingPhase): string {
  if (phase === 'recording') {
    return 'Recording';
  }
  if (phase === 'transcribing') {
    return 'Transcribing';
  }
  return 'Idle';
}

function sidecarLabel(state: SidecarState): string | null {
  if (state === 'failed' || state === 'stopped') {
    return 'Sidecar down';
  }
  if (state === 'restarting' || state === 'starting') {
    return 'Sidecar busy';
  }
  return null;
}

function prefersReducedMotion(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return false;
  }
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

function useReducedMotion(): boolean {
  const [reducedMotion, setReducedMotion] = useState(prefersReducedMotion);

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return;
    }

    const media = window.matchMedia('(prefers-reduced-motion: reduce)');
    const onChange = () => setReducedMotion(media.matches);

    if (typeof media.addEventListener === 'function') {
      media.addEventListener('change', onChange);
      return () => media.removeEventListener('change', onChange);
    }

    if (typeof media.addListener === 'function') {
      media.addListener(onChange);
      return () => media.removeListener(onChange);
    }

    return undefined;
  }, []);

  return reducedMotion;
}

export interface RecordingPillProps {
  phase: RecordingPhase;
  sidecarState: SidecarState;
  timer: ReactNode;
  waveform: ReactNode;
}

export function RecordingPill({ phase, sidecarState, timer, waveform }: RecordingPillProps) {
  const reducedMotion = useReducedMotion();

  if (phase === 'idle') {
    return null;
  }

  const palette = PHASE_PALETTE[phase];
  const sidecar = sidecarLabel(sidecarState);
  const sidecarWarning = sidecarState === 'failed' || sidecarState === 'stopped';

  return (
    <div
      style={{
        alignItems: 'center',
        backdropFilter: 'blur(4px)',
        background: palette.bg,
        border: `1px solid ${palette.border}`,
        borderRadius: 999,
        boxShadow: '0 12px 24px rgba(0, 0, 0, 0.35)',
        color: palette.text,
        display: 'inline-flex',
        gap: 10,
        minHeight: 38,
        padding: '8px 12px',
      }}
    >
      <style>{`
        @keyframes overlay-recording-pulse {
          0% { transform: scale(0.9); opacity: 0.72; }
          50% { transform: scale(1.18); opacity: 1; }
          100% { transform: scale(0.9); opacity: 0.72; }
        }
        @keyframes overlay-transcribing-spin {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }
      `}</style>
      {phase === 'recording' ? (
        <span
          aria-hidden="true"
          data-testid="recording-dot"
          style={{
            animation: reducedMotion ? 'none' : 'overlay-recording-pulse 1.05s ease-in-out infinite',
            backgroundColor: palette.dot,
            borderRadius: 999,
            boxShadow: `0 0 10px ${palette.dot}`,
            display: 'inline-block',
            height: 10,
            width: 10,
          }}
        />
      ) : (
        <span
          aria-hidden="true"
          data-testid="transcribing-spinner"
          style={{
            animation: reducedMotion ? 'none' : 'overlay-transcribing-spin 0.95s linear infinite',
            border: '2px solid rgba(255, 214, 89, 0.34)',
            borderRadius: 999,
            borderTopColor: palette.dot,
            display: 'inline-block',
            height: 12,
            width: 12,
          }}
        />
      )}
      <span style={{ fontSize: 13, fontWeight: 600, minWidth: 84 }}>{phaseLabel(phase)}</span>
      {timer}
      <div style={{ opacity: 1 }}>{waveform}</div>
      {sidecar ? (
        <span
          style={{
            color: sidecarWarning ? '#ffb4b4' : '#f7e7a6',
            fontSize: 11,
            fontWeight: 600,
            marginLeft: 4,
            textTransform: 'uppercase',
          }}
        >
          {sidecar}
        </span>
      ) : null}
    </div>
  );
}
