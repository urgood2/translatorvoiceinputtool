import { useState } from 'react';
import { useAppStore } from '../../store';

type SidecarAppearance = {
  label: string;
  badgeClass: string;
  showSpinner: boolean;
  showRestart: boolean;
};

function sidecarAppearance(state: string): SidecarAppearance {
  switch (state) {
    case 'ready':
      return {
        label: 'Ready',
        badgeClass: 'bg-emerald-500/15 text-emerald-200 ring-emerald-500/40',
        showSpinner: false,
        showRestart: false,
      };
    case 'starting':
    case 'restarting':
      return {
        label: state === 'starting' ? 'Starting' : 'Restarting',
        badgeClass: 'bg-amber-500/15 text-amber-200 ring-amber-500/40',
        showSpinner: true,
        showRestart: false,
      };
    case 'failed':
      return {
        label: 'Failed',
        badgeClass: 'bg-rose-500/15 text-rose-200 ring-rose-500/40',
        showSpinner: false,
        showRestart: true,
      };
    case 'stopped':
      return {
        label: 'Stopped',
        badgeClass: 'bg-slate-500/15 text-slate-200 ring-slate-500/40',
        showSpinner: false,
        showRestart: true,
      };
    default:
      return {
        label: 'Unknown',
        badgeClass: 'bg-slate-500/15 text-slate-200 ring-slate-500/40',
        showSpinner: false,
        showRestart: false,
      };
  }
}

export function SidecarBadge() {
  const sidecarStatus = useAppStore((state) => state.sidecarStatus);
  const restartSidecar = useAppStore((state) => state.restartSidecar);

  const [isRestarting, setIsRestarting] = useState(false);
  const [restartError, setRestartError] = useState<string | null>(null);

  if (!sidecarStatus) {
    return (
      <div className="space-y-1 text-sm">
        <p className="text-gray-100" data-testid="sidecar-badge-empty">No sidecar status</p>
        <p className="text-xs text-gray-400">Start recording once to initialize sidecar telemetry.</p>
      </div>
    );
  }

  const appearance = sidecarAppearance(sidecarStatus.state);

  const onRestart = async () => {
    setRestartError(null);
    setIsRestarting(true);
    try {
      await restartSidecar();
    } catch (error) {
      setRestartError(error instanceof Error ? error.message : 'Failed to restart sidecar.');
    } finally {
      setIsRestarting(false);
    }
  };

  return (
    <div className="space-y-2 text-sm">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span
          className="text-gray-100"
          data-testid="sidecar-badge-restarts"
        >
          Restarts: {sidecarStatus.restart_count}
        </span>
        <span
          className={`inline-flex items-center gap-2 rounded-full px-2.5 py-1 text-xs font-semibold ring-1 ${appearance.badgeClass}`}
          data-testid="sidecar-badge-state"
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

      {sidecarStatus.message ? (
        <p className="text-xs text-gray-300" data-testid="sidecar-badge-message">
          {sidecarStatus.message}
        </p>
      ) : null}

      {appearance.showRestart ? (
        <button
          type="button"
          data-testid="sidecar-badge-restart"
          className="rounded-md border border-slate-500/60 px-2.5 py-1 text-xs font-semibold text-slate-100 hover:bg-slate-700/60 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={() => void onRestart()}
          disabled={isRestarting}
        >
          {isRestarting ? 'Restarting...' : 'Restart'}
        </button>
      ) : null}

      {restartError ? (
        <p className="text-xs text-rose-200">{restartError}</p>
      ) : null}
    </div>
  );
}

export default SidecarBadge;
