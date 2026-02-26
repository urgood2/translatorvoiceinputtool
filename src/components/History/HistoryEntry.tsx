import { useState } from 'react';
import type { InjectionResult, TranscriptEntry } from '../../types';

interface HistoryEntryProps {
  entry: TranscriptEntry;
  onCopy: () => Promise<void>;
}

/** Format a timestamp as relative time. */
function formatRelativeTime(timestamp: string): string {
  const date = new Date(timestamp);
  const now = new Date();
  const seconds = Math.floor((now.getTime() - date.getTime()) / 1000);

  if (seconds < 60) return 'Just now';
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)} hr ago`;
  return date.toLocaleDateString();
}

/** Format duration in milliseconds to human-readable. */
function formatDuration(ms: number): string {
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)}s`;
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}m ${remainingSeconds.toFixed(0)}s`;
}

function formatConfidence(confidence: number): string {
  return `${Math.round(confidence * 100)}%`;
}

function abbreviateSessionId(sessionId: string): string {
  if (sessionId.length <= 14) {
    return sessionId;
  }
  return `${sessionId.slice(0, 8)}…${sessionId.slice(-4)}`;
}

/** Get badge info for injection result. */
function getInjectionBadge(result: InjectionResult): { icon: string; label: string; color: string; tooltip?: string } {
  switch (result.status) {
    case 'injected':
      return { icon: '✅', label: 'Injected', color: 'text-green-600 dark:text-green-400' };
    case 'clipboard_only':
      return {
        icon: '📋',
        label: 'Clipboard',
        color: 'text-yellow-600 dark:text-yellow-400',
        tooltip: result.reason,
      };
    case 'error':
      return {
        icon: '⚠️',
        label: 'Error',
        color: 'text-red-600 dark:text-red-400',
        tooltip: result.message,
      };
  }
}

function timingPairs(entry: TranscriptEntry): Array<{ label: string; value: number }> {
  if (!entry.timings) {
    return [];
  }

  const timings: Array<{ label: string; value: number | undefined }> = [
    { label: 'IPC', value: entry.timings.ipc_ms },
    { label: 'Transcribe', value: entry.timings.transcribe_ms },
    { label: 'Post', value: entry.timings.postprocess_ms },
    { label: 'Inject', value: entry.timings.inject_ms },
    { label: 'Total', value: entry.timings.total_ms },
  ];

  return timings
    .filter((timing) => typeof timing.value === 'number')
    .map((timing) => ({ label: timing.label, value: timing.value as number }));
}

export function HistoryEntry({ entry, onCopy }: HistoryEntryProps) {
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState<string | null>(null);
  const [showRawText, setShowRawText] = useState(false);
  const badge = getInjectionBadge(entry.injection_result);
  const hasRawFinalDiff =
    typeof entry.raw_text === 'string'
    && typeof entry.final_text === 'string'
    && entry.raw_text !== entry.final_text;
  const displayText = hasRawFinalDiff && showRawText
    ? (entry.raw_text as string)
    : (entry.final_text ?? entry.text);
  const hasMetadata =
    typeof entry.language === 'string'
    || typeof entry.confidence === 'number'
    || typeof entry.session_id === 'string';
  const timings = timingPairs(entry);

  const handleCopy = async () => {
    setCopyError(null);
    try {
      await onCopy();
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      setCopied(false);
      setCopyError(e instanceof Error ? e.message : 'Failed to copy transcript');
    }
  };

  return (
    <div className="rounded-lg border border-gray-200 bg-white p-3 transition-shadow hover:shadow-sm dark:border-gray-700 dark:bg-gray-800">
      <div className="mb-2 flex items-center justify-between">
        <div className="flex items-center gap-2 text-sm text-gray-500 dark:text-gray-400">
          <span>{formatRelativeTime(entry.timestamp)}</span>
          <span>•</span>
          <span>{formatDuration(entry.audio_duration_ms)} audio</span>
        </div>

        <div className={`flex items-center gap-1 text-sm ${badge.color}`} title={badge.tooltip}>
          <span>{badge.icon}</span>
          <span className="hidden sm:inline">{badge.label}</span>
        </div>
      </div>

      <p className="mb-2 line-clamp-3 text-gray-900 dark:text-gray-100">
        {displayText}
      </p>

      {hasRawFinalDiff ? (
        <button
          type="button"
          data-testid={`history-entry-toggle-${entry.id}`}
          onClick={() => setShowRawText((value) => !value)}
          aria-pressed={showRawText}
          aria-label={`${showRawText ? 'Show final text' : 'Show raw text'} for transcript`}
          className="mb-2 rounded border border-gray-300 px-2 py-0.5 text-xs text-gray-600 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
        >
          {showRawText ? 'Show final text' : 'Show raw text'}
        </button>
      ) : null}

      {hasMetadata ? (
        <div className="mb-2 flex flex-wrap gap-2 text-xs text-gray-500 dark:text-gray-400">
          {entry.language && <span>Language: {entry.language}</span>}
          {typeof entry.confidence === 'number' && (
            <span>Confidence: {formatConfidence(entry.confidence)}</span>
          )}
          {entry.session_id && (
            <span
              title={entry.session_id}
              data-testid={`history-entry-session-${entry.id}`}
            >
              Session: {abbreviateSessionId(entry.session_id)}
            </span>
          )}
        </div>
      ) : null}

      {timings.length > 0 ? (
        <div className="mb-3 flex flex-wrap gap-2 text-xs text-gray-500 dark:text-gray-400">
          {timings.map((timing) => (
            <span
              key={timing.label}
              data-testid={`history-entry-timing-${entry.id}-${timing.label.toLowerCase()}`}
            >
              {timing.label}: {Math.round(timing.value)}ms
            </span>
          ))}
        </div>
      ) : null}

      <div className="flex justify-end">
        <button
          type="button"
          onClick={() => void handleCopy()}
          aria-label={copied ? 'Transcript copied' : 'Copy transcript'}
          className="rounded px-3 py-1 text-sm text-gray-600 transition-colors hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-700"
        >
          {copied ? '✓ Copied' : 'Copy'}
        </button>
      </div>

      {copyError ? (
        <p role="alert" className="mt-2 text-sm text-red-600 dark:text-red-400">{copyError}</p>
      ) : null}
    </div>
  );
}

export default HistoryEntry;
