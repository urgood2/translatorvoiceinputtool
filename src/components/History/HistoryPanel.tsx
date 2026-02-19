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

import { useEffect, useMemo, useState } from 'react';
import type { TranscriptEntry } from '../../types';
import { HistoryEntry } from './HistoryEntry';

export interface HistoryPanelProps {
  entries: TranscriptEntry[];
  onCopy: (id: string) => Promise<void>;
  onClearAll?: () => Promise<void>;
}

export function HistoryPanel({ entries, onCopy, onClearAll }: HistoryPanelProps) {
  const [searchInput, setSearchInput] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const [isClearing, setIsClearing] = useState(false);
  const [clearError, setClearError] = useState<string | null>(null);

  useEffect(() => {
    const timeoutId = window.setTimeout(() => {
      setDebouncedQuery(searchInput.trim().toLowerCase());
    }, 300);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [searchInput]);

  const filteredEntries = useMemo(() => {
    if (debouncedQuery.length === 0) {
      return entries;
    }

    return entries.filter((entry) => {
      const text = entry.text.toLowerCase();
      const finalText = (entry.final_text ?? '').toLowerCase();
      const language = (entry.language ?? '').toLowerCase();

      return (
        text.includes(debouncedQuery)
        || finalText.includes(debouncedQuery)
        || language.includes(debouncedQuery)
      );
    });
  }, [debouncedQuery, entries]);

  const showingFilteredResults = debouncedQuery.length > 0;
  const summaryLabel = showingFilteredResults
    ? `Showing ${filteredEntries.length} of ${entries.length} entries`
    : `${entries.length} ${entries.length === 1 ? 'entry' : 'entries'}`;

  const clearDisabled = entries.length === 0 || isClearing;

  const onConfirmClear = async () => {
    if (!onClearAll) {
      setShowClearConfirm(false);
      return;
    }

    setClearError(null);
    setIsClearing(true);
    try {
      await onClearAll();
      setShowClearConfirm(false);
    } catch (error) {
      setClearError(error instanceof Error ? error.message : 'Failed to clear history');
    } finally {
      setIsClearing(false);
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col space-y-4">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
          Recent Transcripts
        </h3>
        <div className="flex items-center gap-2">
          <span className="text-sm text-gray-500 dark:text-gray-400">
            {summaryLabel}
          </span>
          <button
            type="button"
            data-testid="history-clear-all-button"
            disabled={clearDisabled}
            onClick={() => setShowClearConfirm(true)}
            className="rounded-md border border-gray-300 px-2.5 py-1.5 text-xs font-semibold text-gray-700 hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
          >
            Clear All
          </button>
        </div>
      </div>

      <div className="flex items-center gap-2">
        <input
          type="search"
          value={searchInput}
          data-testid="history-search-input"
          placeholder="Search transcripts or language…"
          onChange={(event) => setSearchInput(event.target.value)}
          className="w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 focus:border-blue-400 focus:outline-none focus:ring-2 focus:ring-blue-200 dark:border-gray-600 dark:bg-gray-900 dark:text-gray-100 dark:placeholder:text-gray-500 dark:focus:border-blue-500 dark:focus:ring-blue-900/50"
        />
        {searchInput.length > 0 ? (
          <button
            type="button"
            data-testid="history-search-clear"
            onClick={() => setSearchInput('')}
            className="rounded-md border border-gray-300 px-2.5 py-2 text-xs font-semibold text-gray-700 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
          >
            Clear
          </button>
        ) : null}
      </div>

      {entries.length === 0 ? (
        <div className="flex h-full min-h-0 items-center justify-center rounded-lg bg-gray-50 p-8 text-center text-gray-500 dark:bg-gray-800 dark:text-gray-400">
          <div>
            <div className="mb-3 text-4xl">🎤</div>
            <p className="font-medium">No transcripts yet</p>
            <p className="mt-1 text-sm">Press the hotkey to start recording.</p>
          </div>
        </div>
      ) : filteredEntries.length === 0 ? (
        <div className="flex h-full min-h-0 items-center justify-center rounded-lg border border-dashed border-gray-500/40 p-8 text-center text-gray-400">
          <div>
            <p className="font-medium text-gray-200">No matching transcripts</p>
            <p className="mt-1 text-sm">Try a different search term.</p>
          </div>
        </div>
      ) : (
        <div className="min-h-0 flex-1 space-y-2 overflow-y-auto pr-1" data-testid="history-scroll-region">
          {filteredEntries.map((entry) => (
            <HistoryEntry
              key={entry.id}
              entry={entry}
              onCopy={() => onCopy(entry.id)}
            />
          ))}
        </div>
      )}

      {showClearConfirm ? (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="history-clear-title"
            className="w-full max-w-md rounded-lg border border-gray-600 bg-gray-800 p-4 shadow-xl"
          >
            <h4 id="history-clear-title" className="text-base font-semibold text-gray-100">
              Clear all transcript history?
            </h4>
            <p className="mt-2 text-sm text-gray-300">
              This action cannot be undone.
            </p>

            {clearError ? (
              <p className="mt-2 text-xs text-rose-300">{clearError}</p>
            ) : null}

            <div className="mt-4 flex justify-end gap-2">
              <button
                type="button"
                data-testid="history-clear-cancel"
                onClick={() => {
                  if (!isClearing) {
                    setShowClearConfirm(false);
                    setClearError(null);
                  }
                }}
                className="rounded-md border border-gray-500 px-3 py-1.5 text-sm text-gray-100 hover:bg-gray-700 disabled:cursor-not-allowed disabled:opacity-60"
                disabled={isClearing}
              >
                Cancel
              </button>
              <button
                type="button"
                data-testid="history-clear-confirm"
                onClick={() => void onConfirmClear()}
                className="rounded-md bg-red-600 px-3 py-1.5 text-sm font-semibold text-white hover:bg-red-500 disabled:cursor-not-allowed disabled:opacity-60"
                disabled={isClearing}
              >
                {isClearing ? 'Clearing...' : 'Clear All'}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

export default HistoryPanel;
