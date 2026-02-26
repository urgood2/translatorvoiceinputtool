/**
 * Model settings component for ASR model management.
 *
 * Features:
 * - Model status display (missing, downloading, verifying, ready, error, unknown)
 * - Download progress with visual progress bar
 * - "Download now" and "Purge cache" actions
 * - Model info (ID, revision, size)
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { AppConfig, ModelCatalogEntry, ModelStatus, ModelState, Progress } from '../../types';

interface ModelSettingsProps {
  status: ModelStatus | null;
  onDownload: () => Promise<void>;
  onPurgeCache: () => Promise<void>;
  onSelectModel?: (modelId: string) => Promise<void>;
  isLoading?: boolean;
}

function parseCatalogEntries(payload: unknown): ModelCatalogEntry[] {
  if (!Array.isArray(payload)) {
    return [];
  }

  return payload.filter((entry): entry is ModelCatalogEntry => {
    return (
      typeof entry === 'object'
      && entry !== null
      && 'model_id' in entry
      && typeof (entry as ModelCatalogEntry).model_id === 'string'
      && 'family' in entry
      && typeof (entry as ModelCatalogEntry).family === 'string'
      && 'display_name' in entry
      && typeof (entry as ModelCatalogEntry).display_name === 'string'
      && 'description' in entry
      && typeof (entry as ModelCatalogEntry).description === 'string'
      && 'supported_languages' in entry
      && Array.isArray((entry as ModelCatalogEntry).supported_languages)
      && 'default_language' in entry
      && typeof (entry as ModelCatalogEntry).default_language === 'string'
      && 'size_bytes' in entry
      && typeof (entry as ModelCatalogEntry).size_bytes === 'number'
      && 'manifest_path' in entry
      && typeof (entry as ModelCatalogEntry).manifest_path === 'string'
    );
  });
}

/** Format bytes to human-readable size. */
function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

function formatProgressValue(value: number, unit: string): string {
  const normalizedUnit = unit.trim().toLowerCase();
  if (normalizedUnit === 'bytes' || normalizedUnit === 'byte') {
    return formatBytes(value);
  }

  return unit.length > 0 ? `${value} ${unit}` : String(value);
}

function formatEta(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) {
    return '<1s';
  }

  const rounded = Math.round(seconds);
  if (rounded < 60) return `${rounded}s`;

  const minutes = Math.floor(rounded / 60);
  const remainingSeconds = rounded % 60;
  if (minutes < 60) {
    return remainingSeconds === 0 ? `${minutes}m` : `${minutes}m ${remainingSeconds}s`;
  }

  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  return remainingMinutes === 0 ? `${hours}h` : `${hours}h ${remainingMinutes}m`;
}

/** Get status configuration for display. */
function getStatusConfig(state: ModelState): { color: string; icon: string; label: string } {
  switch (state) {
    case 'missing':
      return { color: 'text-yellow-600 dark:text-yellow-400', icon: '⚠️', label: 'Available' };
    case 'loading':
      return { color: 'text-blue-600 dark:text-blue-400', icon: '⚙️', label: 'Installing...' };
    case 'downloading':
      return { color: 'text-blue-600 dark:text-blue-400', icon: '⬇️', label: 'Installing...' };
    case 'verifying':
      return { color: 'text-blue-600 dark:text-blue-400', icon: '🔍', label: 'Installing...' };
    case 'ready':
      return { color: 'text-green-600 dark:text-green-400', icon: '✅', label: 'Ready' };
    case 'error':
      return { color: 'text-red-600 dark:text-red-400', icon: '❌', label: 'Error' };
    case 'unknown':
      return { color: 'text-gray-600 dark:text-gray-400', icon: '❔', label: 'Available' };
  }

  // Exhaustive fallback for future enum extensions.
  return { color: 'text-gray-600 dark:text-gray-400', icon: 'ℹ️', label: state };
}

/** Progress bar component. */
function ProgressBar({ progress }: { progress: Progress }) {
  const sampleRef = useRef<{ bytes: number; atMs: number } | null>(null);
  const [bytesPerSecond, setBytesPerSecond] = useState<number | null>(null);
  const percentage = progress.total
    ? Math.round((progress.current / progress.total) * 100)
    : 0;
  const normalizedUnit = progress.unit.trim().toLowerCase();
  const isByteProgress = normalizedUnit === 'bytes' || normalizedUnit === 'byte';

  useEffect(() => {
    if (!isByteProgress) {
      sampleRef.current = null;
      setBytesPerSecond(null);
      return;
    }

    const nowMs = Date.now();
    const previous = sampleRef.current;
    if (previous) {
      if (progress.current < previous.bytes) {
        // New transfer session or reset; discard prior throughput sample.
        setBytesPerSecond(null);
      } else if (progress.current > previous.bytes && nowMs > previous.atMs) {
        const deltaBytes = progress.current - previous.bytes;
        const deltaSeconds = (nowMs - previous.atMs) / 1000;
        if (deltaBytes > 0 && deltaSeconds > 0) {
          const instantaneous = deltaBytes / deltaSeconds;
          setBytesPerSecond((prior) => (prior && prior > 0 ? (prior * 0.7) + (instantaneous * 0.3) : instantaneous));
        }
      }
    }

    sampleRef.current = { bytes: progress.current, atMs: nowMs };
  }, [isByteProgress, progress.current]);

  const etaSeconds = useMemo(() => {
    if (
      !isByteProgress
      || !bytesPerSecond
      || bytesPerSecond <= 0
      || typeof progress.total !== 'number'
      || progress.total <= progress.current
    ) {
      return null;
    }

    return (progress.total - progress.current) / bytesPerSecond;
  }, [bytesPerSecond, isByteProgress, progress.current, progress.total]);

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
          {formatProgressValue(progress.current, progress.unit)}
          {progress.total ? ` / ${formatProgressValue(progress.total, progress.unit)}` : ''}
        </span>
        <span>{percentage}%</span>
      </div>
      {(bytesPerSecond || etaSeconds) && (
        <div className="flex justify-between text-xs text-gray-500 dark:text-gray-400">
          <span>
            {bytesPerSecond ? `Speed: ${formatBytes(bytesPerSecond)}/s` : ''}
          </span>
          <span>
            {etaSeconds ? `ETA: ${formatEta(etaSeconds)}` : ''}
          </span>
        </div>
      )}
    </div>
  );
}

export function ModelSettings({
  status,
  onDownload,
  onPurgeCache,
  onSelectModel,
  isLoading,
}: ModelSettingsProps) {
  const [actionInProgress, setActionInProgress] = useState<'download' | 'purge' | 'select' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showPurgeConfirm, setShowPurgeConfirm] = useState(false);
  const [catalogEntries, setCatalogEntries] = useState<ModelCatalogEntry[]>([]);
  const [isCatalogLoading, setIsCatalogLoading] = useState(false);
  const [activeModelId, setActiveModelId] = useState<string | null>(status?.model_id ?? null);
  const [selectedLanguage, setSelectedLanguage] = useState<string>('auto');

  useEffect(() => {
    if (typeof status?.model_id === 'string' && status.model_id.length > 0) {
      setActiveModelId(status.model_id);
    }
  }, [status?.model_id]);

  useEffect(() => {
    let active = true;
    const loadCatalog = async () => {
      setIsCatalogLoading(true);
      try {
        const payload = await invoke<unknown>('get_model_catalog');
        if (!active) return;
        setCatalogEntries(parseCatalogEntries(payload));
      } catch (loadError) {
        if (active) {
          console.warn('Failed to load model catalog in ModelSettings', loadError);
        }
      } finally {
        if (active) {
          setIsCatalogLoading(false);
        }
      }
    };

    void loadCatalog();
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let active = true;
    const loadLanguageConfig = async () => {
      try {
        const config = await invoke<unknown>('get_config');
        if (!active || typeof config !== 'object' || config === null) return;

        const modelConfig = (config as AppConfig).model;
        if (modelConfig?.model_id) {
          setActiveModelId(modelConfig.model_id);
        }
        if (typeof modelConfig?.language === 'string' && modelConfig.language.length > 0) {
          setSelectedLanguage(modelConfig.language);
        } else {
          setSelectedLanguage('auto');
        }
      } catch {
        // Best-effort load; keep defaults.
      }
    };

    void loadLanguageConfig();
    return () => {
      active = false;
    };
  }, []);

  const selectedCatalogEntry = useMemo(() => {
    const currentModelId = activeModelId ?? status?.model_id;
    if (!currentModelId) return null;
    return catalogEntries.find((entry) => entry.model_id === currentModelId) ?? null;
  }, [activeModelId, catalogEntries, status?.model_id]);

  useEffect(() => {
    if (!selectedCatalogEntry || selectedCatalogEntry.family !== 'whisper') {
      return;
    }
    const available = new Set([
      'auto',
      ...selectedCatalogEntry.supported_languages.map((language) => language.toLowerCase()),
    ]);
    if (!available.has(selectedLanguage.toLowerCase())) {
      setSelectedLanguage('auto');
    }
  }, [selectedCatalogEntry, selectedLanguage]);

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

  const defaultSelectModel = async (modelId: string) => {
    const config = await invoke<AppConfig>('get_config');
    const nextConfig: AppConfig = {
      ...config,
      model: {
        model_id: modelId,
        device: config.model?.device ?? null,
        preferred_device: config.model?.preferred_device ?? 'auto',
        language: config.model?.language ?? null,
      },
    };
    await invoke('update_config', { config: nextConfig });
  };

  const handleSelectModel = async (modelId: string) => {
    setError(null);
    setActionInProgress('select');
    try {
      if (onSelectModel) {
        await onSelectModel(modelId);
      } else {
        await defaultSelectModel(modelId);
      }
      setActiveModelId(modelId);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to select model');
    } finally {
      setActionInProgress(null);
    }
  };

  const handleLanguageChange = async (language: string) => {
    setError(null);
    setSelectedLanguage(language);
    setActionInProgress('select');
    try {
      const config = await invoke<AppConfig>('get_config');
      const nextConfig: AppConfig = {
        ...config,
        model: {
          model_id: activeModelId ?? config.model?.model_id ?? null,
          device: config.model?.device ?? null,
          preferred_device: config.model?.preferred_device ?? 'auto',
          language,
        },
      };
      await invoke('update_config', { config: nextConfig });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to update model language');
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
  const isInstalling =
    status.status === 'loading'
    || status.status === 'downloading'
    || status.status === 'verifying';
  const canInstall =
    status.status === 'missing'
    || status.status === 'error'
    || status.status === 'unknown';
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
              {selectedCatalogEntry?.description ?? 'NVIDIA Parakeet TDT 0.6B - High-quality speech recognition'}
            </p>
          </div>
          <div className={`flex items-center gap-1 ${statusConfig.color}`}>
            <span>{statusConfig.icon}</span>
            <span className="text-sm font-medium">{statusConfig.label}</span>
          </div>
        </div>
      </div>

      {/* Catalog selector */}
      <div className="space-y-2">
        <h4 className="text-sm font-semibold text-gray-900 dark:text-gray-100">
          Available Models
        </h4>
        {isCatalogLoading && (
          <p className="text-xs text-gray-500 dark:text-gray-400" role="status" aria-live="polite">
            Loading model catalog...
          </p>
        )}
        {catalogEntries.length > 0 && (
          <div className="space-y-2">
            {catalogEntries.map((entry) => {
              const isActive = (activeModelId ?? status.model_id) === entry.model_id;
              const cardStatus =
                status.model_id === entry.model_id ? status.status : ('missing' as ModelState);
              const cardStatusConfig = getStatusConfig(cardStatus);

              return (
                <div
                  key={entry.model_id}
                  className={`rounded-lg border p-3 ${
                    isActive
                      ? 'border-blue-500 bg-blue-50/50 dark:bg-blue-900/10'
                      : 'border-gray-200 dark:border-gray-700'
                  }`}
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <p className="text-sm font-medium text-gray-900 dark:text-gray-100">{entry.display_name}</p>
                      <p className="text-xs text-gray-500 dark:text-gray-400 font-mono break-all">{entry.model_id}</p>
                      <p className="text-xs text-gray-600 dark:text-gray-300 mt-1">{entry.family} • {formatBytes(entry.size_bytes)}</p>
                    </div>
                    <div className="flex items-center gap-2">
                      <span className={`text-xs ${cardStatusConfig.color}`}>{cardStatusConfig.label}</span>
                      <button
                        type="button"
                        onClick={() => void handleSelectModel(entry.model_id)}
                        disabled={isLoading || actionInProgress !== null || isActive}
                        className="px-3 py-1 rounded text-xs font-medium bg-gray-100 dark:bg-gray-700
                                   text-gray-800 dark:text-gray-200 hover:bg-gray-200 dark:hover:bg-gray-600
                                   disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        {isActive && status.status === 'ready' ? 'Active' : isActive ? 'Selected' : 'Select'}
                      </button>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}

        {selectedCatalogEntry?.family === 'whisper' && (
          <div className="space-y-1">
            <label
              htmlFor="whisper-language"
              className="text-xs font-medium text-gray-700 dark:text-gray-300"
            >
              Language
            </label>
            <select
              id="whisper-language"
              aria-label="Language"
              value={selectedLanguage}
              onChange={(event) => void handleLanguageChange(event.target.value)}
              disabled={isLoading || actionInProgress !== null}
              className="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800
                         text-sm text-gray-900 dark:text-gray-100 px-3 py-2
                         disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <option value="auto">Auto detect</option>
              {selectedCatalogEntry.supported_languages
                .map((language) => language.toLowerCase())
                .filter((language, index, values) => values.indexOf(language) === index)
                .filter((language) => language !== 'auto')
                .map((language) => (
                  <option key={language} value={language}>
                    {language.toUpperCase()}
                  </option>
                ))}
            </select>
          </div>
        )}
      </div>

      {/* Download progress */}
      {isInstalling && status.progress && (
        <ProgressBar progress={status.progress} />
      )}

      {/* Missing state */}
      {status.status === 'missing' && (
        <div className="p-4 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-lg">
          <p className="text-sm text-yellow-700 dark:text-yellow-300">
            This model is available but not installed yet.
            Install it to enable voice transcription with this model.
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
        {canInstall && (
          <button
            type="button"
            onClick={handleDownload}
            disabled={isLoading || actionInProgress !== null}
            className="px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-md text-sm font-medium
                       transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {actionInProgress === 'download' ? 'Starting...' : status.status === 'error' ? 'Retry Install' : 'Install Model'}
          </button>
        )}

        {/* Installing indicator */}
        {isInstalling && (
          <button
            type="button"
            disabled
            className="px-4 py-2 bg-gray-100 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded-md text-sm font-medium cursor-not-allowed"
          >
            Installing...
          </button>
        )}

        {/* Purge cache button */}
        {canPurge && !showPurgeConfirm && (
          <button
            type="button"
            onClick={() => setShowPurgeConfirm(true)}
            disabled={isLoading || actionInProgress !== null}
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
