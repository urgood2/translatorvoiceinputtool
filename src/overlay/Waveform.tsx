const BAR_PATTERN = [0.22, 0.36, 0.52, 0.68, 0.84, 1.0, 1.0, 0.84, 0.68, 0.52, 0.36, 0.22];

export interface WaveformProps {
  active: boolean;
  level: number;
}

function clampUnit(value: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.min(1, value));
}

export function Waveform({ active, level }: WaveformProps) {
  const normalized = clampUnit(level);

  return (
    <div
      aria-hidden="true"
      style={{
        alignItems: 'center',
        display: 'flex',
        gap: 2,
        height: 16,
      }}
    >
      {BAR_PATTERN.map((factor, index) => {
        const targetHeight = active ? 0.2 + factor * normalized : 0.15;
        return (
          <span
            key={`bar-${index}`}
            style={{
              backgroundColor: active ? 'rgba(164, 244, 194, 0.95)' : 'rgba(140, 140, 140, 0.6)',
              borderRadius: 999,
              display: 'inline-block',
              height: `${Math.max(2, Math.round(14 * targetHeight))}px`,
              transition: 'height 66ms linear, background-color 160ms ease',
              width: '3px',
            }}
          />
        );
      })}
    </div>
  );
}
