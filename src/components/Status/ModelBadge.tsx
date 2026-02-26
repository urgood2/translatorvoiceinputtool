import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../store';
import type { ModelState, Progress } from '../../types';

type ModelCatalogEntry = {
  model_id: string;
  display_name: string;
};

type StatusAppearance = {
  label: string;
  badgeClass: string;
  showSpinner: boolean;
  canDownload: boolean;
  showError: boolean;
  showProgress: boolean;
};

function formatProgress(progress: Progress): string {
  if (typeof progress.total === 'number' && progress.total > 0) {
    const percent = Math.max(0, Math.min(100, Math.round((progress.current / progress.total) * 100)));
    return `${percent}%`;
  }

  if (progress.unit.length > 0) {
    return `${progress.current} ${progress.unit}`;
  }

  return String(progress.current);
}

function statusAppearance(status: ModelState | 'not_downloaded'): StatusAppearance {
  if (status === 'ready') {
    return {
      label: 'Ready',
      badgeClass: 'bg-emerald-500/15 text-emerald-200 ring-emerald-500/40',
      showSpinner: false,
      canDownload: false,
      showError: false,
      showProgress: false,
    };
  }

  if (status === 'error') {
    return {
      label: 'Error',
      badgeClass: 'bg-rose-500/15 text-rose-200 ring-rose-500/40',
      showSpinner: false,
      canDownload: false,
      showError: true,
      showProgress: false,
    };
  }

  if (
    status === 'loading'
    || status === 'downloading'
    || status === 'verifying'
  ) {
    return {
      label: 'Loading',
      badgeClass: 'bg-amber-500/15 text-amber-200 ring-amber-500/40',
      showSpinner: true,
      canDownload: false,
      showError: false,
      showProgress: true,
    };
  }

  return {
    label: 'Not Installed',
    badgeClass: 'bg-slate-500/15 text-slate-200 ring-slate-500/40',
    showSpinner: false,
    canDownload: true,
    showError: false,
    showProgress: false,
  };
}

export function ModelBadge() {
  const modelStatus = useAppStore((state) => state.modelStatus);
  const config = useAppStore((state) => state.config);
  const downloadProgress = useAppStore((state) => state.downloadProgress);
  const downloadModel = useAppStore((state) => state.downloadModel);

  const [catalog, setCatalog] = useState<Record<string, string>>({});
  const [isDownloading, setIsDownloading] = useState(false);
  const [downloadError, setDownloadError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;

    const loadCatalog = async () => {
      try {
        const entries = await invoke<unknown>('get_model_catalog');
        if (!active || !Array.isArray(entries)) {
          return;
        }

        const nextCatalog: Record<string, string> = {};
        for (const entry of entries) {
          if (
            typeof entry === 'object'
            && entry !== null
            && 'model_id' in entry
            && 'display_name' in entry
            && typeof (entry as ModelCatalogEntry).model_id === 'string'
            && typeof (entry as ModelCatalogEntry).display_name === 'string'
          ) {
            const typedEntry = entry as ModelCatalogEntry;
            nextCatalog[typedEntry.model_id] = typedEntry.display_name;
          }
        }

        setCatalog(nextCatalog);
      } catch (error) {
        console.warn('Failed to load model catalog for dashboard badge', error);
      }
    };

    void loadCatalog();
    return () => {
      active = false;
    };
  }, []);

  const modelId = modelStatus?.model_id ?? config?.model?.model_id ?? null;
  const hasModelInfo = Boolean(modelId) || modelStatus !== null;

  const displayName = useMemo(() => {
    if (!modelId) {
      return 'No model';
    }

    return catalog[modelId] ?? modelId;
  }, [catalog, modelId]);

  if (!hasModelInfo) {
    return (
      <div className="space-y-1 text-sm" role="status" aria-live="polite" aria-atomic="true">
        <p className="text-gray-100" data-testid="model-badge-empty">No model</p>
        <p className="text-xs text-gray-400">Select or download a model in Settings.</p>
      </div>
    );
  }

  const rawStatus = modelStatus?.status ?? 'unknown';
  const normalizedStatus = rawStatus === 'unknown' ? 'not_downloaded' : rawStatus;
  const appearance = statusAppearance(normalizedStatus);
  const progress = modelStatus?.progress ?? downloadProgress;
  const progressLabel = progress ? formatProgress(progress) : null;

  const onDownload = async () => {
    setDownloadError(null);
    setIsDownloading(true);
    try {
      await downloadModel();
    } catch (error) {
      setDownloadError(error instanceof Error ? error.message : 'Failed to start model download.');
    } finally {
      setIsDownloading(false);
    }
  };

  return (
    <div className="space-y-2 text-sm" role="status" aria-live="polite" aria-atomic="true">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="text-gray-100" data-testid="model-badge-name">{displayName}</span>
        <span
          className={`inline-flex items-center gap-2 rounded-full px-2.5 py-1 text-xs font-semibold ring-1 ${appearance.badgeClass}`}
          data-testid="model-badge-status"
          aria-label={`Model status: ${appearance.label}`}
        >
          {appearance.showSpinner ? (
            <span
              className="h-3 w-3 animate-spin rounded-full border border-amber-300 border-t-transparent"
              aria-hidden="true"
            />
          ) : null}
          <span>{appearance.label}</span>
        </span>
      </div>

      {appearance.showProgress && progressLabel ? (
        <p className="text-xs text-amber-200" data-testid="model-badge-progress" aria-live="polite">
          Progress: {progressLabel}
        </p>
      ) : null}

      {appearance.showError && modelStatus?.error ? (
        <p className="text-xs text-rose-200" data-testid="model-badge-error" aria-live="assertive">
          {modelStatus.error}
        </p>
      ) : null}

      {appearance.canDownload ? (
        <button
          type="button"
          data-testid="model-badge-download"
          className="rounded-md border border-slate-500/60 px-2.5 py-1 text-xs font-semibold text-slate-100 hover:bg-slate-700/60 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={() => void onDownload()}
          disabled={isDownloading}
          aria-label={isDownloading ? 'Starting model download' : 'Download model'}
        >
          {isDownloading ? 'Starting...' : 'Download'}
        </button>
      ) : null}

      {downloadError ? (
        <p role="alert" className="text-xs text-rose-200">{downloadError}</p>
      ) : null}
    </div>
  );
}

export default ModelBadge;
