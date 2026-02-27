import { useCallback, useState } from 'react';
import { useAppStore } from '../../store';
import type { AppState, TranscriptEntry } from '../../types';
import { ModelBadge } from './ModelBadge';
import { SidecarBadge } from './SidecarBadge';

type BadgeConfig = {
  label: string;
  dotClass: string;
  animate: boolean;
};

function stateBadgeConfig(appState: AppState, enabled: boolean): BadgeConfig {
  if (!enabled) {
    return {
      label: 'Paused',
      dotClass: 'bg-slate-400 dark:bg-slate-500',
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
      return { label: 'Idle', dotClass: 'bg-emerald-400', animate: false };
  }
}

function formatMode(mode?: string): string {
  if (mode === 'toggle') return 'Toggle';
  return 'Push-to-Talk';
}

function truncatePreview(text: string, maxChars = 140): string {
  const trimmed = text.trim();
  if (trimmed.length <= maxChars) {
    return trimmed;
  }
  return `${trimmed.slice(0, maxChars - 3)}...`;
}

function formatTranscriptTimestamp(timestamp: string): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return 'Unknown time';
  }

  const diffMs = Date.now() - date.getTime();
  if (diffMs < 60_000) return 'Just now';
  if (diffMs < 60 * 60_000) return `${Math.floor(diffMs / 60_000)}m ago`;
  if (diffMs < 24 * 60 * 60_000) return `${Math.floor(diffMs / (60 * 60_000))}h ago`;
  return date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
}

function formatAudioDuration(audioDurationMs: number): string {
  if (audioDurationMs < 1000) return `${audioDurationMs}ms`;
  return `${(audioDurationMs / 1000).toFixed(1)}s`;
}

function injectionStatusLabel(entry: TranscriptEntry): string {
  switch (entry.injection_result.status) {
    case 'injected':
      return 'Injected';
    case 'clipboard_only':
      return 'Clipboard';
    case 'error':
      return 'Injection Error';
    default:
      return 'Unknown';
  }
}

function SectionCard({ title, children }: { title: string; children: React.ReactNode }) {
  const headingId = `${title.toLowerCase().replace(/\s+/g, '-')}-section-title`;
  return (
    <section
      className="rounded-lg border border-gray-200 dark:border-gray-700 bg-white/80 dark:bg-gray-800/80 p-4"
      aria-labelledby={headingId}
    >
      <h3 id={headingId} className="mb-3 text-sm font-semibold uppercase tracking-wide text-gray-600 dark:text-gray-400">{title}</h3>
      {children}
    </section>
  );
}

type StatusDashboardProps = {
  onNavigateSettings?: () => void;
};

export function StatusDashboard({ onNavigateSettings }: StatusDashboardProps = {}) {
  const appState = useAppStore((state) => state.appState);
  const enabled = useAppStore((state) => state.enabled);
  const errorDetail = useAppStore((state) => state.errorDetail);
  const config = useAppStore((state) => state.config);
  const history = useAppStore((state) => state.history);
  const startRecording = useAppStore((state) => state.startRecording);
  const stopRecording = useAppStore((state) => state.stopRecording);
  const cancelRecording = useAppStore((state) => state.cancelRecording);
  const [pendingAction, setPendingAction] = useState<'start' | 'stop' | 'cancel' | null>(null);

  const badge = stateBadgeConfig(appState, enabled);
  const hotkey = config?.hotkeys.primary?.trim() ?? '';
  const hasHotkey = hotkey.length > 0;
  const mode = formatMode(config?.hotkeys.mode);

  const latestTranscript = history[0];
  const transcriptPreview = latestTranscript
    ? truncatePreview(latestTranscript.text, 100)
    : 'No transcripts yet.';
  const transcriptTimestamp = latestTranscript
    ? formatTranscriptTimestamp(latestTranscript.timestamp)
    : null;
  const transcriptAudio = latestTranscript
    ? formatAudioDuration(latestTranscript.audio_duration_ms)
    : null;
  const transcriptInjection = latestTranscript
    ? injectionStatusLabel(latestTranscript)
    : null;

  const canStart = enabled && appState === 'idle' && pendingAction === null;
  const canStop = appState === 'recording' && pendingAction === null;
  const canCancel = appState === 'recording' && pendingAction === null;

  const runRecordingAction = useCallback(
    async (action: 'start' | 'stop' | 'cancel', command: () => Promise<void>) => {
      setPendingAction(action);
      try {
        await command();
      } catch (error) {
        console.error(`Recording action '${action}' failed`, error);
      } finally {
        setPendingAction(null);
      }
    },
    []
  );

  return (
    <div className="grid gap-3 sm:gap-4" data-testid="status-dashboard">
      <SectionCard title="App State">
        <div className="flex items-center justify-between gap-3">
          <span className="text-sm text-gray-700 dark:text-gray-300">Current State</span>
          <span
            role="status"
            aria-live={appState === 'error' ? 'assertive' : 'polite'}
            aria-atomic="true"
            className="inline-flex items-center gap-2 rounded-full bg-gray-100 dark:bg-gray-700 px-3 py-1 text-xs font-semibold text-gray-900 dark:text-white"
          >
            <span
              className={`h-2.5 w-2.5 rounded-full ${badge.dotClass} ${badge.animate ? 'animate-pulse' : ''}`}
              aria-hidden="true"
            />
            <span>{badge.label}</span>
          </span>
        </div>
        <div className="mt-3 flex flex-wrap items-center gap-2">
          {appState === 'recording' ? (
            <>
              <button
                type="button"
                data-testid="recording-stop-button"
                className="rounded-md bg-red-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-red-500 disabled:cursor-not-allowed disabled:opacity-60"
                disabled={!canStop}
                onClick={() => void runRecordingAction('stop', stopRecording)}
              >
                Stop Recording
              </button>
              <button
                type="button"
                data-testid="recording-cancel-button"
                className="rounded-md border border-gray-300 dark:border-gray-600 px-3 py-1.5 text-xs font-semibold text-gray-900 dark:text-gray-100 hover:bg-gray-50 dark:hover:bg-gray-700 disabled:cursor-not-allowed disabled:opacity-60"
                disabled={!canCancel}
                onClick={() => void runRecordingAction('cancel', cancelRecording)}
              >
                Cancel
              </button>
            </>
          ) : (
            <button
              type="button"
              data-testid="recording-start-button"
              className="rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-emerald-500 disabled:cursor-not-allowed disabled:opacity-60"
              disabled={!canStart}
              onClick={() => void runRecordingAction('start', startRecording)}
            >
              Start Recording
            </button>
          )}
        </div>
        {appState === 'recording' && config?.audio.vad_enabled ? (
          <p
            className="mt-2 text-xs text-purple-600 dark:text-purple-300"
            data-testid="vad-active-badge"
          >
            <span aria-hidden="true">&#9679; </span>
            Auto-stop enabled &mdash; will stop after{' '}
            {(config.audio.vad_silence_ms / 1000).toFixed(1)}s of silence
          </p>
        ) : null}
        {appState === 'error' && errorDetail ? (
          <p
            role="alert"
            className="mt-3 text-xs text-orange-600 dark:text-orange-200"
            data-testid="app-state-error-detail"
          >
            {errorDetail}
          </p>
        ) : null}
      </SectionCard>

      <SectionCard title="Hotkey">
        <div className="space-y-1 text-sm text-gray-800 dark:text-gray-200">
          {hasHotkey ? (
            <p>
              <span className="text-gray-600 dark:text-gray-400">Hotkey:</span>{' '}
              <code className="rounded bg-gray-200 dark:bg-gray-700 px-1.5 py-0.5 text-xs text-sky-600 dark:text-sky-300">{hotkey}</code>{' '}
              <span className="text-gray-700 dark:text-gray-300">({mode})</span>
            </p>
          ) : (
            <p>
              <span className="text-gray-600 dark:text-gray-400">Hotkey:</span>{' '}
              <span className="text-amber-600 dark:text-amber-300">No hotkey configured.</span>{' '}
              {onNavigateSettings ? (
                <button
                  type="button"
                  data-testid="hotkey-settings-link"
                  className="text-sky-600 dark:text-sky-400 underline hover:text-sky-500 dark:hover:text-sky-300"
                  onClick={onNavigateSettings}
                >
                  Configure it in Settings.
                </button>
              ) : (
                <span className="text-gray-600 dark:text-gray-400">Configure it in Settings.</span>
              )}
            </p>
          )}
        </div>
      </SectionCard>

      <SectionCard title="Last Transcript">
        <p className="text-sm leading-relaxed text-gray-900 dark:text-gray-100" aria-live="polite">
          {transcriptPreview}
        </p>
        {latestTranscript ? (
          <p className="mt-2 text-xs text-gray-400">
            {transcriptTimestamp} · {transcriptAudio} · {transcriptInjection}
          </p>
        ) : null}
      </SectionCard>

      <SectionCard title="Model">
        <ModelBadge />
      </SectionCard>

      <SectionCard title="Sidecar">
        <SidecarBadge />
      </SectionCard>
    </div>
  );
}

export default StatusDashboard;
