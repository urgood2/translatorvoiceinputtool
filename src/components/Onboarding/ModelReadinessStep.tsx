/**
 * Model readiness step for onboarding wizard.
 *
 * Checks model download status, initiates download if needed,
 * and shows progress until the model is ready.
 */

import { useEffect, useCallback, useState } from 'react';
import { useAppStore } from '../../store/appStore';
import type { ModelState } from '../../types';

export interface ModelReadinessStepProps {
  onReady: () => void;
}

export function ModelReadinessStep({ onReady }: ModelReadinessStepProps) {
  const modelStatus = useAppStore((s) => s.modelStatus);
  const downloadProgress = useAppStore((s) => s.downloadProgress);
  const refreshModelStatus = useAppStore((s) => s.refreshModelStatus);
  const downloadModel = useAppStore((s) => s.downloadModel);

  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const state: ModelState = modelStatus?.status ?? 'unknown';

  // Check model status on mount
  useEffect(() => {
    refreshModelStatus();
  }, [refreshModelStatus]);

  // Auto-advance when model becomes ready
  useEffect(() => {
    if (state === 'ready') {
      onReady();
    }
  }, [state, onReady]);

  const handleDownload = useCallback(async () => {
    setDownloading(true);
    setError(null);
    try {
      await downloadModel();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Download failed');
    } finally {
      setDownloading(false);
    }
  }, [downloadModel]);

  const progressPercent =
    downloadProgress?.total && downloadProgress.total > 0
      ? Math.round((downloadProgress.current / downloadProgress.total) * 100)
      : 0;

  return (
    <div>
      <h2 className="text-2xl font-bold mb-4">Speech Recognition Model</h2>

      {state === 'ready' && (
        <p className="text-green-600 dark:text-green-400 mb-8">
          Model is ready. You&rsquo;re all set!
        </p>
      )}

      {state === 'missing' && !downloading && (
        <>
          <p className="text-gray-600 dark:text-gray-400 mb-4">
            The speech recognition model needs to be downloaded before you can use voice input.
          </p>
          <button
            type="button"
            onClick={handleDownload}
            className="px-6 py-2 text-sm rounded bg-blue-600 text-white hover:bg-blue-700 mb-4"
          >
            Download Model
          </button>
        </>
      )}

      {(state === 'downloading' || downloading) && (
        <>
          <p className="text-blue-600 dark:text-blue-400 mb-4">Downloading model...</p>
          <div className="w-full h-3 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden mb-2">
            <div
              className="h-full bg-blue-600 rounded-full transition-all duration-300"
              style={{ width: `${progressPercent}%` }}
              role="progressbar"
              aria-valuenow={progressPercent}
              aria-valuemin={0}
              aria-valuemax={100}
            />
          </div>
          {downloadProgress?.total && downloadProgress.total > 0 && (
            <p className="text-sm text-gray-500">{progressPercent}%</p>
          )}
        </>
      )}

      {state === 'verifying' && (
        <p className="text-blue-600 dark:text-blue-400 mb-8">Verifying model integrity...</p>
      )}

      {state === 'unknown' && !downloading && (
        <p className="text-gray-600 dark:text-gray-400 mb-8">Checking model status...</p>
      )}

      {(state === 'error' || error) && (
        <div className="mb-4">
          <p className="text-red-600 dark:text-red-400 mb-2">
            {error || modelStatus?.error || 'An error occurred'}
          </p>
          <button
            type="button"
            onClick={handleDownload}
            className="px-4 py-2 text-sm rounded bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600"
          >
            Retry Download
          </button>
        </div>
      )}
    </div>
  );
}
