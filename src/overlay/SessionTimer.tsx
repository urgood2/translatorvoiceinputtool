import { useEffect, useMemo, useState } from 'react';

type RecordingPhase = 'idle' | 'recording' | 'transcribing';

export interface SessionTimerProps {
  phase: RecordingPhase;
  audioMs?: number;
  startedAtMs?: number | null;
}

function formatTimer(totalMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(totalMs / 1000));
  const minutes = Math.floor(totalSeconds / 60)
    .toString()
    .padStart(2, '0');
  const seconds = (totalSeconds % 60).toString().padStart(2, '0');
  return `${minutes}:${seconds}`;
}

export function SessionTimer({ phase, audioMs = 0, startedAtMs = null }: SessionTimerProps) {
  const isActive = phase === 'recording' && typeof startedAtMs === 'number';
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    if (!isActive) {
      return;
    }

    const timerId = window.setInterval(() => {
      setNowMs(Date.now());
    }, 500); // 2Hz max timer updates.

    return () => {
      window.clearInterval(timerId);
    };
  }, [isActive, startedAtMs]);

  const elapsedMs = useMemo(() => {
    if (isActive && typeof startedAtMs === 'number') {
      return Math.max(audioMs, nowMs - startedAtMs);
    }
    return Math.max(0, audioMs);
  }, [audioMs, isActive, nowMs, startedAtMs]);

  return (
    <span
      style={{
        fontVariantNumeric: 'tabular-nums',
        fontSize: 12,
        opacity: 0.95,
      }}
    >
      {formatTimer(elapsedMs)}
    </span>
  );
}
