/**
 * Transcript history panel showing recent transcriptions.
 *
 * Features:
 * - Shows recent transcripts with newest first
 * - Copy action for each transcript
 * - Shows injection status (injected, clipboard-only, error)
 * - Relative timestamps ("2 minutes ago")
 * - Audio duration display
 */

import { useState } from 'react';
import type { TranscriptEntry, InjectionResult } from '../../types';

interface HistoryPanelProps {
  entries: TranscriptEntry[];
  onCopy: (id: string) => Promise<void>;
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

interface TranscriptCardProps {
  entry: TranscriptEntry;
  onCopy: () => Promise<void>;
}

function TranscriptCard({ entry, onCopy }: TranscriptCardProps) {
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState<string | null>(null);
  const badge = getInjectionBadge(entry.injection_result);

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
    <div className="p-3 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 hover:shadow-sm transition-shadow">
      {/* Header with timestamp and badge */}
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2 text-sm text-gray-500 dark:text-gray-400">
          <span>{formatRelativeTime(entry.timestamp)}</span>
          <span>•</span>
          <span>{formatDuration(entry.audio_duration_ms)} audio</span>
        </div>

        {/* Injection status badge */}
        <div
          className={`flex items-center gap-1 text-sm ${badge.color}`}
          title={badge.tooltip}
        >
          <span>{badge.icon}</span>
          <span className="hidden sm:inline">{badge.label}</span>
        </div>
      </div>

      {/* Transcript text */}
      <p className="text-gray-900 dark:text-gray-100 line-clamp-3 mb-3">
        {entry.text}
      </p>

      {/* Actions */}
      <div className="flex justify-end">
        <button
          onClick={handleCopy}
          className="px-3 py-1 text-sm text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded transition-colors"
        >
          {copied ? '✓ Copied' : 'Copy'}
        </button>
      </div>

      {copyError && (
        <p className="mt-2 text-sm text-red-600 dark:text-red-400">{copyError}</p>
      )}
    </div>
  );
}

export function HistoryPanel({ entries, onCopy }: HistoryPanelProps) {
  // Empty state
  if (entries.length === 0) {
    return (
      <div className="p-8 text-center text-gray-500 dark:text-gray-400 bg-gray-50 dark:bg-gray-800 rounded-lg">
        <div className="text-4xl mb-3">🎤</div>
        <p className="font-medium">No recent transcripts</p>
        <p className="text-sm mt-1">Press the hotkey to start recording.</p>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
          Recent Transcripts
        </h3>
        <span className="text-sm text-gray-500 dark:text-gray-400">
          {entries.length} {entries.length === 1 ? 'entry' : 'entries'}
        </span>
      </div>

      {/* Transcript list (newest first - entries should already be sorted) */}
      <div className="space-y-2">
        {entries.map((entry) => (
          <TranscriptCard
            key={entry.id}
            entry={entry}
            onCopy={() => onCopy(entry.id)}
          />
        ))}
      </div>
    </div>
  );
}

export default HistoryPanel;
