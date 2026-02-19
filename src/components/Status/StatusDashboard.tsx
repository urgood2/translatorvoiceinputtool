import { useAppStore } from '../../store';
import type { AppState } from '../../types';

type BadgeConfig = {
  label: string;
  dotClass: string;
  animate: boolean;
};

function stateBadgeConfig(appState: AppState, enabled: boolean): BadgeConfig {
  if (!enabled) {
    return {
      label: 'Paused',
      dotClass: 'bg-slate-400',
      animate: false,
    };
  }

  switch (appState) {
    case 'recording':
      return { label: 'Recording', dotClass: 'bg-red-400', animate: true };
    case 'transcribing':
      return { label: 'Transcribing', dotClass: 'bg-sky-400', animate: true };
    case 'loading_model':
      return { label: 'Loading Model', dotClass: 'bg-amber-400', animate: true };
    case 'error':
      return { label: 'Error', dotClass: 'bg-orange-400', animate: false };
    case 'idle':
    default:
      return { label: 'Ready', dotClass: 'bg-emerald-400', animate: false };
  }
}

function formatMode(mode?: string): string {
  if (mode === 'toggle') return 'Toggle';
  return 'Hold';
}

function truncatePreview(text: string, maxChars = 140): string {
  const trimmed = text.trim();
  if (trimmed.length <= maxChars) {
    return trimmed;
  }
  return `${trimmed.slice(0, maxChars - 3)}...`;
}

function SectionCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-lg border border-gray-700 bg-gray-800/80 p-4" aria-label={title}>
      <h3 className="mb-3 text-sm font-semibold uppercase tracking-wide text-gray-400">{title}</h3>
      {children}
    </section>
  );
}

export function StatusDashboard() {
  const appState = useAppStore((state) => state.appState);
  const enabled = useAppStore((state) => state.enabled);
  const config = useAppStore((state) => state.config);
  const history = useAppStore((state) => state.history);
  const modelStatus = useAppStore((state) => state.modelStatus);
  const sidecarStatus = useAppStore((state) => state.sidecarStatus);

  const badge = stateBadgeConfig(appState, enabled);
  const hotkey = config?.hotkeys.primary ?? 'Not configured';
  const mode = formatMode(config?.hotkeys.mode);

  const lastTranscript = history[0]?.text;
  const transcriptPreview = lastTranscript
    ? truncatePreview(lastTranscript)
    : 'No transcript available yet.';

  const modelName = modelStatus?.model_id ?? config?.model?.model_id ?? 'Default';
  const modelState = modelStatus?.status ?? 'unknown';

  const sidecarState = sidecarStatus?.state ?? 'unknown';
  const restartCount = sidecarStatus?.restart_count ?? 0;

  return (
    <div className="grid gap-3 sm:gap-4" data-testid="status-dashboard">
      <SectionCard title="App State">
        <div className="flex items-center justify-between gap-3">
          <span className="text-sm text-gray-300">Current State</span>
          <span className="inline-flex items-center gap-2 rounded-full bg-gray-700 px-3 py-1 text-xs font-semibold text-white">
            <span
              className={`h-2.5 w-2.5 rounded-full ${badge.dotClass} ${badge.animate ? 'animate-pulse' : ''}`}
              aria-hidden="true"
            />
            <span>{badge.label}</span>
          </span>
        </div>
      </SectionCard>

      <SectionCard title="Hotkey">
        <div className="space-y-1 text-sm text-gray-200">
          <p>
            <span className="text-gray-400">Primary:</span>{' '}
            <code className="rounded bg-gray-700 px-1.5 py-0.5 text-xs text-sky-300">{hotkey}</code>
          </p>
          <p>
            <span className="text-gray-400">Mode:</span> {mode}
          </p>
        </div>
      </SectionCard>

      <SectionCard title="Last Transcript">
        <p className="text-sm leading-relaxed text-gray-100">{transcriptPreview}</p>
      </SectionCard>

      <SectionCard title="Model">
        <div className="space-y-1 text-sm text-gray-200">
          <p>
            <span className="text-gray-400">Name:</span> {modelName}
          </p>
          <p>
            <span className="text-gray-400">Status:</span> {modelState}
          </p>
        </div>
      </SectionCard>

      <SectionCard title="Sidecar">
        <div className="space-y-1 text-sm text-gray-200">
          <p>
            <span className="text-gray-400">State:</span> {sidecarState}
          </p>
          <p>
            <span className="text-gray-400">Restarts:</span> {restartCount}
          </p>
        </div>
      </SectionCard>
    </div>
  );
}

export default StatusDashboard;
