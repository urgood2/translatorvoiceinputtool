/**
 * Status indicator component showing current app state.
 *
 * Visual states:
 * - Idle: green dot, "Ready"
 * - Recording: red pulsing dot, "Recording..."
 * - Transcribing: yellow spinner, "Transcribing..."
 * - LoadingModel: blue spinner + progress, "Loading model..."
 * - Error: red exclamation, error message
 */

import type { AppState } from '../types';

interface StatusIndicatorProps {
  state: AppState;
  enabled: boolean;
  detail?: string;
  progress?: { current: number; total?: number };
}

/** Status configuration for each state. */
const STATUS_CONFIG: Record<AppState, { color: string; label: string; animate?: boolean }> = {
  idle: { color: 'bg-green-500', label: 'Ready' },
  recording: { color: 'bg-red-500', label: 'Recording...', animate: true },
  transcribing: { color: 'bg-yellow-500', label: 'Transcribing...', animate: true },
  loading_model: { color: 'bg-blue-500', label: 'Loading model...', animate: true },
  error: { color: 'bg-red-500', label: 'Error' },
};

export function StatusIndicator({ state, enabled, detail, progress }: StatusIndicatorProps) {
  const config = STATUS_CONFIG[state];
  const isDisabled = !enabled;

  // Disabled state overrides
  const displayColor = isDisabled ? 'bg-gray-400' : config.color;
  const displayLabel = isDisabled ? 'Paused' : config.label;
  const shouldAnimate = !isDisabled && config.animate;

  return (
    <div className="flex items-center gap-3 p-4 bg-gray-50 dark:bg-gray-800 rounded-lg">
      {/* Status dot/indicator */}
      <div className="relative">
        <div
          className={`w-4 h-4 rounded-full ${displayColor} ${
            shouldAnimate ? 'animate-pulse' : ''
          }`}
        />
        {/* Error icon overlay */}
        {state === 'error' && !isDisabled && (
          <span className="absolute inset-0 flex items-center justify-center text-white text-xs font-bold">
            !
          </span>
        )}
      </div>

      {/* Status text and details */}
      <div className="flex-1 min-w-0">
        <div className="font-medium text-gray-900 dark:text-gray-100">
          {displayLabel}
        </div>

        {/* Detail/error message */}
        {detail && (
          <div className={`text-sm truncate ${
            state === 'error' ? 'text-red-600 dark:text-red-400' : 'text-gray-500 dark:text-gray-400'
          }`}>
            {detail}
          </div>
        )}

        {/* Progress bar for loading_model */}
        {state === 'loading_model' && progress && progress.total && (
          <div className="mt-2">
            <div className="h-2 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
              <div
                className="h-full bg-blue-500 transition-all duration-300"
                style={{ width: `${Math.round((progress.current / progress.total) * 100)}%` }}
              />
            </div>
            <div className="text-xs text-gray-500 dark:text-gray-400 mt-1">
              {Math.round((progress.current / progress.total) * 100)}%
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

export default StatusIndicator;
