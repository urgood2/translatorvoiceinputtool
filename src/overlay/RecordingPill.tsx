import type { ReactNode } from 'react';

type RecordingPhase = 'idle' | 'recording' | 'transcribing';
type SidecarState = 'starting' | 'ready' | 'failed' | 'restarting' | 'stopped' | 'unknown';

interface Palette {
  bg: string;
  border: string;
  dot: string;
  text: string;
}

const PHASE_PALETTE: Record<RecordingPhase, Palette> = {
  idle: {
    bg: 'rgba(26, 26, 26, 0.78)',
    border: 'rgba(129, 129, 129, 0.7)',
    dot: '#a6a6a6',
    text: '#f0f0f0',
  },
  recording: {
    bg: 'rgba(11, 53, 24, 0.88)',
    border: 'rgba(72, 179, 118, 0.78)',
    dot: '#5cff8f',
    text: '#e5ffed',
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

export interface RecordingPillProps {
  phase: RecordingPhase;
  sidecarState: SidecarState;
  timer: ReactNode;
  waveform: ReactNode;
}

export function RecordingPill({ phase, sidecarState, timer, waveform }: RecordingPillProps) {
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
      <span
        aria-hidden="true"
        style={{
          backgroundColor: palette.dot,
          borderRadius: 999,
          boxShadow: phase === 'recording' ? `0 0 8px ${palette.dot}` : 'none',
          display: 'inline-block',
          height: 8,
          width: 8,
        }}
      />
      <span style={{ fontSize: 13, fontWeight: 600, minWidth: 84 }}>{phaseLabel(phase)}</span>
      {timer}
      <div style={{ opacity: phase === 'recording' || phase === 'transcribing' ? 1 : 0.75 }}>
        {waveform}
      </div>
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
