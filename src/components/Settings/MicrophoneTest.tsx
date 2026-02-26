/**
 * Microphone test component with real-time level meter.
 *
 * Features:
 * - Real-time audio level visualization (RMS + peak)
 * - Color-coded levels (green/yellow/red)
 * - No-signal detection with warning
 * - Smooth animations with decay
 * - Start/stop test controls
 */

import { useEffect, useState, useRef, useCallback } from 'react';

interface AudioLevel {
  rms: number;
  peak: number;
}

interface MicrophoneTestProps {
  deviceUid: string | undefined;
  onStartTest: () => Promise<void>;
  onStopTest: () => Promise<void>;
  audioLevel: AudioLevel | null;
  isRunning?: boolean;
}

/** Threshold for detecting no signal. */
const NO_SIGNAL_THRESHOLD = 0.01;
const NO_SIGNAL_TIMEOUT_MS = 3000;

/** Level color thresholds. */
const LEVEL_THRESHOLDS = {
  green: 0.5,
  yellow: 0.8,
};

/** Get color class based on level. */
function getLevelColor(level: number): string {
  if (level > LEVEL_THRESHOLDS.yellow) return 'bg-red-500';
  if (level > LEVEL_THRESHOLDS.green) return 'bg-yellow-500';
  return 'bg-green-500';
}

export function MicrophoneTest({
  deviceUid,
  onStartTest,
  onStopTest,
  audioLevel,
  isRunning = false,
}: MicrophoneTestProps) {
  const [displayLevel, setDisplayLevel] = useState({ rms: 0, peak: 0 });
  const [peakHold, setPeakHold] = useState(0);
  const [noSignal, setNoSignal] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isStarting, setIsStarting] = useState(false);

  const lastActivityRef = useRef(Date.now());
  const animationFrameRef = useRef<number>();
  const peakHoldTimeoutRef = useRef<NodeJS.Timeout>();

  // Smooth level animation with decay
  useEffect(() => {
    if (!audioLevel) return;

    const targetLevel = audioLevel;

    // Update last activity if we have signal
    if (targetLevel.peak > NO_SIGNAL_THRESHOLD) {
      lastActivityRef.current = Date.now();
      setNoSignal(false);
    }

    // Update peak hold
    if (targetLevel.peak > peakHold) {
      setPeakHold(targetLevel.peak);
      if (peakHoldTimeoutRef.current) {
        clearTimeout(peakHoldTimeoutRef.current);
      }
      peakHoldTimeoutRef.current = setTimeout(() => {
        setPeakHold(0);
      }, 1000);
    }

    // Animate to target level
    const animate = () => {
      setDisplayLevel(prev => {
        const decayRate = 0.15;
        const attackRate = 0.5;

        return {
          rms: prev.rms < targetLevel.rms
            ? prev.rms + (targetLevel.rms - prev.rms) * attackRate
            : prev.rms - (prev.rms - targetLevel.rms) * decayRate,
          peak: prev.peak < targetLevel.peak
            ? prev.peak + (targetLevel.peak - prev.peak) * attackRate
            : prev.peak - (prev.peak - targetLevel.peak) * decayRate,
        };
      });
    };

    animationFrameRef.current = requestAnimationFrame(animate);

    return () => {
      if (animationFrameRef.current) {
        cancelAnimationFrame(animationFrameRef.current);
      }
    };
  }, [audioLevel, peakHold]);

  // Check for no signal
  useEffect(() => {
    if (!isRunning) {
      setNoSignal(false);
      return;
    }

    const checkSignal = setInterval(() => {
      if (Date.now() - lastActivityRef.current > NO_SIGNAL_TIMEOUT_MS) {
        setNoSignal(true);
      }
    }, 500);

    return () => clearInterval(checkSignal);
  }, [isRunning]);

  // Reset level when device changes
  useEffect(() => {
    setDisplayLevel({ rms: 0, peak: 0 });
    setPeakHold(0);
    setNoSignal(false);
  }, [deviceUid]);

  const handleStart = useCallback(async () => {
    setError(null);
    setIsStarting(true);
    try {
      await onStartTest();
      lastActivityRef.current = Date.now();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to start mic test');
    } finally {
      setIsStarting(false);
    }
  }, [onStartTest]);

  const handleStop = useCallback(async () => {
    setError(null);
    try {
      await onStopTest();
      setDisplayLevel({ rms: 0, peak: 0 });
      setPeakHold(0);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to stop mic test');
    }
  }, [onStopTest]);

  // Percentage for display
  const rmsPercent = Math.round(displayLevel.rms * 100);
  const peakPercent = Math.round(peakHold * 100);

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h4 className="font-medium text-gray-900 dark:text-gray-100">
          Microphone Test
        </h4>
        <button
          type="button"
          onClick={isRunning ? handleStop : handleStart}
          disabled={isStarting}
          aria-label={isRunning ? 'Stop microphone test' : 'Start microphone test'}
          className={`px-4 py-2 rounded-md text-sm font-medium transition-colors
                     ${isRunning
                       ? 'bg-red-500 hover:bg-red-600 text-white'
                       : 'bg-blue-500 hover:bg-blue-600 text-white'}
                     disabled:opacity-50 disabled:cursor-not-allowed`}
        >
          {isStarting ? 'Starting...' : isRunning ? 'Stop Test' : 'Start Test'}
        </button>
      </div>

      {/* Level meter */}
      <div className="space-y-2">
        <div
          role="progressbar"
          aria-label="Microphone input level"
          aria-valuenow={rmsPercent}
          aria-valuemin={0}
          aria-valuemax={100}
          className="relative h-6 bg-gray-200 dark:bg-gray-700 rounded overflow-hidden"
        >
          {/* RMS level bar */}
          <div
            className={`absolute inset-y-0 left-0 transition-all duration-75 ${getLevelColor(displayLevel.rms)}`}
            style={{ width: `${rmsPercent}%` }}
          />

          {/* Peak hold indicator */}
          {peakHold > 0 && (
            <div
              className="absolute inset-y-0 w-1 bg-white opacity-80"
              style={{ left: `${peakPercent}%`, transform: 'translateX(-50%)' }}
            />
          )}

          {/* Threshold markers */}
          <div
            className="absolute inset-y-0 w-px bg-yellow-600 opacity-50"
            style={{ left: '50%' }}
            title="Normal speech"
          />
          <div
            className="absolute inset-y-0 w-px bg-red-600 opacity-50"
            style={{ left: '80%' }}
            title="Loud"
          />

          {/* Level readout */}
          <div className="absolute inset-0 flex items-center justify-end pr-2">
            <span className="text-xs font-mono text-gray-600 dark:text-gray-300 bg-white/50 dark:bg-black/30 px-1 rounded" aria-live="polite">
              {rmsPercent}%
            </span>
          </div>
        </div>

        {/* Legend */}
        <div className="flex items-center justify-between text-xs text-gray-500 dark:text-gray-400">
          <span>Silent</span>
          <span>Normal</span>
          <span>Loud</span>
          <span>Clipping</span>
        </div>
      </div>

      {/* No signal warning */}
      {isRunning && noSignal && (
        <div role="alert" className="p-3 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-md">
          <div className="flex items-start gap-2">
            <span className="text-yellow-500">⚠️</span>
            <div>
              <p className="text-sm font-medium text-yellow-700 dark:text-yellow-300">
                No audio detected
              </p>
              <p className="text-xs text-yellow-600 dark:text-yellow-400 mt-1">
                Check your microphone connection and permissions. Make sure the correct input device is selected.
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Idle state hint */}
      {!isRunning && !error && (
        <p className="text-sm text-gray-500 dark:text-gray-400">
          Click "Start Test" to check if your microphone is working correctly.
        </p>
      )}

      {/* Error display */}
      {error && (
        <div role="alert" className="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md">
          <p className="text-sm text-red-600 dark:text-red-400">{error}</p>
        </div>
      )}
    </div>
  );
}

export default MicrophoneTest;
