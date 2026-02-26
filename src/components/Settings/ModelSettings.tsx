/**
 * Model settings component for ASR model management.
 *
 * Features:
 * - Model status display (missing, downloading, verifying, ready, error, unknown)
 * - Download progress with visual progress bar
 * - "Download now" and "Purge cache" actions
 * - Model info (ID, revision, size)
 */

import { useState } from 'react';
import type { ModelStatus, ModelState, Progress } from '../../types';

interface ModelSettingsProps {
  status: ModelStatus | null;
  onDownload: () => Promise<void>;
  onPurgeCache: () => Promise<void>;
  isLoading?: boolean;
}

/** Format bytes to human-readable size. */
function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

/** Get status configuration for display. */
function getStatusConfig(state: ModelState): { color: string; icon: string; label: string } {
  switch (state) {
    case 'missing':
      return { color: 'text-yellow-600 dark:text-yellow-400', icon: '⚠️', label: 'Not Downloaded' };
    case 'downloading':
      return { color: 'text-blue-600 dark:text-blue-400', icon: '⬇️', label: 'Downloading...' };
    case 'verifying':
      return { color: 'text-blue-600 dark:text-blue-400', icon: '🔍', label: 'Verifying...' };
    case 'ready':
      return { color: 'text-green-600 dark:text-green-400', icon: '✅', label: 'Ready' };
    case 'error':
      return { color: 'text-red-600 dark:text-red-400', icon: '❌', label: 'Error' };
    case 'unknown':
      return { color: 'text-gray-600 dark:text-gray-400', icon: '❔', label: 'Unknown' };
  }

  // Exhaustive fallback for future enum extensions.
  return { color: 'text-gray-600 dark:text-gray-400', icon: 'ℹ️', label: state };
}

/** Progress bar component. */
function ProgressBar({ progress }: { progress: Progress }) {
  const percentage = progress.total
    ? Math.round((progress.current / progress.total) * 100)
    : 0;

  return (
    <div className="space-y-2" aria-live="polite">
      <div
        role="progressbar"
        aria-label="Model download progress"
        aria-valuenow={percentage}
        aria-valuemin={0}
        aria-valuemax={100}
        className="h-3 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden"
      >
        <div
          className="h-full bg-blue-500 transition-all duration-300"
          style={{ width: `${percentage}%` }}
        />
      </div>
      <div className="flex justify-between text-sm text-gray-600 dark:text-gray-400">
        <span>
          {formatBytes(progress.current)}
          {progress.total ? ` / ${formatBytes(progress.total)}` : ''}
        </span>
        <span>{percentage}%</span>
      </div>
    </div>
  );
}

export function ModelSettings({
  status,
  onDownload,
  onPurgeCache,
  isLoading,
}: ModelSettingsProps) {
  const [actionInProgress, setActionInProgress] = useState<'download' | 'purge' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showPurgeConfirm, setShowPurgeConfirm] = useState(false);

  const handleDownload = async () => {
    setError(null);
    setActionInProgress('download');
    try {
      await onDownload();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to start download');
    } finally {
      setActionInProgress(null);
    }
  };

  const handlePurge = async () => {
    setError(null);
    setShowPurgeConfirm(false);
    setActionInProgress('purge');
    try {
      await onPurgeCache();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to purge cache');
    } finally {
      setActionInProgress(null);
    }
  };

  // Loading state
  if (!status) {
    return (
      <div className="space-y-4">
        <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
          Speech Recognition Model
        </h3>
        <div className="flex items-center gap-2 text-gray-500 dark:text-gray-400" role="status" aria-live="polite">
          <div className="animate-spin h-4 w-4 border-2 border-gray-300 border-t-blue-500 rounded-full" aria-hidden="true" />
          <span>Loading model status...</span>
        </div>
      </div>
    );
  }

  const statusConfig = getStatusConfig(status.status);
  const isDownloading = status.status === 'downloading' || status.status === 'verifying';
  const canDownload = status.status === 'missing' || status.status === 'error';
  const canPurge = status.status === 'ready';

  return (
    <div className="space-y-4">
      <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
        Speech Recognition Model
      </h3>

      {/* Model info */}
      <div className="p-4 bg-gray-50 dark:bg-gray-800 rounded-lg" role="status" aria-live="polite" aria-atomic="true">
        <div className="flex items-start justify-between">
          <div>
            <div className="font-mono text-sm text-gray-700 dark:text-gray-300">
              {status.model_id}
            </div>
            <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">
              NVIDIA Parakeet TDT 0.6B - High-quality speech recognition
            </p>
          </div>
          <div className={`flex items-center gap-1 ${statusConfig.color}`}>
            <span>{statusConfig.icon}</span>
            <span className="text-sm font-medium">{statusConfig.label}</span>
          </div>
        </div>
      </div>

      {/* Download progress */}
      {isDownloading && status.progress && (
        <ProgressBar progress={status.progress} />
      )}

      {/* Missing state */}
      {status.status === 'missing' && (
        <div className="p-4 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-lg">
          <p className="text-sm text-yellow-700 dark:text-yellow-300">
            The speech recognition model needs to be downloaded before you can use voice transcription.
            Download size is approximately <strong>2.5 GB</strong>.
          </p>
        </div>
      )}

      {/* Error state */}
      {status.status === 'error' && status.error && (
        <div role="alert" className="p-4 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg">
          <p className="text-sm text-red-700 dark:text-red-300 font-medium">
            Model Error
          </p>
          <p className="text-sm text-red-600 dark:text-red-400 mt-1">
            {status.error}
          </p>
        </div>
      )}

      {/* Ready state */}
      {status.status === 'ready' && (
        <div className="p-4 bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-lg">
          <p className="text-sm text-green-700 dark:text-green-300">
            Model is ready for transcription.
          </p>
        </div>
      )}

      {/* Actions */}
      <div className="flex gap-3">
        {/* Download/Retry button */}
        {canDownload && (
          <button
            type="button"
            onClick={handleDownload}
            disabled={isLoading || actionInProgress !== null}
            aria-label={status.status === 'error' ? 'Retry model download' : 'Download speech recognition model'}
            className="px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-md text-sm font-medium
                       transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {actionInProgress === 'download' ? 'Starting...' : status.status === 'error' ? 'Retry Download' : 'Download Model'}
          </button>
        )}

        {/* Downloading indicator */}
        {isDownloading && (
          <button
            type="button"
            disabled
            className="px-4 py-2 bg-gray-100 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded-md text-sm font-medium cursor-not-allowed"
          >
            {status.status === 'verifying' ? 'Verifying...' : 'Downloading...'}
          </button>
        )}

        {/* Purge cache button */}
        {canPurge && !showPurgeConfirm && (
          <button
            type="button"
            onClick={() => setShowPurgeConfirm(true)}
            disabled={isLoading || actionInProgress !== null}
            aria-label="Purge cached speech recognition model"
            className="px-4 py-2 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20
                       rounded-md text-sm font-medium transition-colors
                       disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Purge Cache
          </button>
        )}

        {/* Purge confirmation */}
        {showPurgeConfirm && (
          <div className="flex items-center gap-2">
            <span className="text-sm text-gray-600 dark:text-gray-400">Delete model and redownload?</span>
            <button
              type="button"
              onClick={handlePurge}
              disabled={actionInProgress === 'purge'}
              className="px-3 py-1 bg-red-500 hover:bg-red-600 text-white rounded text-sm
                         disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {actionInProgress === 'purge' ? 'Deleting...' : 'Yes, Delete'}
            </button>
            <button
              type="button"
              onClick={() => setShowPurgeConfirm(false)}
              className="px-3 py-1 text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700 rounded text-sm"
            >
              Cancel
            </button>
          </div>
        )}
      </div>

      {/* Error from action */}
      {error && (
        <div role="alert" className="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md">
          <p className="text-sm text-red-600 dark:text-red-400">{error}</p>
        </div>
      )}
    </div>
  );
}

export default ModelSettings;
