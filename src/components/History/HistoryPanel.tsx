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

import { useEffect, useMemo, useRef, useState } from 'react';
import type { TranscriptEntry } from '../../types';
import { HistoryEntry } from './HistoryEntry';

export type ExportFormat = 'markdown' | 'csv';

export interface HistoryPanelProps {
  entries: TranscriptEntry[];
  onCopy: (id: string) => Promise<void>;
  onClearAll?: () => Promise<void>;
  onExport?: (format: ExportFormat) => Promise<string>;
}

function getFocusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(
    container.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
    )
  );
}

export function HistoryPanel({ entries, onCopy, onClearAll, onExport }: HistoryPanelProps) {
  const [searchInput, setSearchInput] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const [isClearing, setIsClearing] = useState(false);
  const [clearError, setClearError] = useState<string | null>(null);
  const [isExporting, setIsExporting] = useState(false);
  const [exportResult, setExportResult] = useState<{ path: string } | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);
  const clearAllButtonRef = useRef<HTMLButtonElement | null>(null);
  const clearDialogRef = useRef<HTMLDivElement | null>(null);
  const wasDialogOpenRef = useRef(false);

  useEffect(() => {
    const timeoutId = window.setTimeout(() => {
      setDebouncedQuery(searchInput.trim().toLowerCase());
    }, 300);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [searchInput]);

  useEffect(() => {
    if (!showClearConfirm) {
      return;
    }

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || isClearing) {
        return;
      }
      event.preventDefault();
      setShowClearConfirm(false);
      setClearError(null);
    };

    window.addEventListener('keydown', handleEscape);
    return () => window.removeEventListener('keydown', handleEscape);
  }, [showClearConfirm, isClearing]);

  useEffect(() => {
    if (!showClearConfirm) {
      if (wasDialogOpenRef.current) {
        wasDialogOpenRef.current = false;
        clearAllButtonRef.current?.focus();
      }
      return;
    }

    wasDialogOpenRef.current = true;
    const dialog = clearDialogRef.current;
    if (!dialog) {
      return;
    }

    const focusable = getFocusableElements(dialog);
    focusable[0]?.focus();

    const handleTab = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') {
        return;
      }

      const targets = getFocusableElements(dialog);
      if (targets.length === 0) {
        return;
      }

      const first = targets[0];
      const last = targets[targets.length - 1];
      const active = document.activeElement;

      if (event.shiftKey) {
        if (active === first || !dialog.contains(active)) {
          event.preventDefault();
          last.focus();
        }
        return;
      }

      if (active === last) {
        event.preventDefault();
        first.focus();
      }
    };

    dialog.addEventListener('keydown', handleTab);
    return () => {
      dialog.removeEventListener('keydown', handleTab);
    };
  }, [showClearConfirm]);

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
  const exportDisabled = entries.length === 0 || isExporting;

  const handleExport = async (format: ExportFormat) => {
    if (!onExport) {
      return;
    }

    setExportError(null);
    setExportResult(null);
    setIsExporting(true);
    try {
      const path = await onExport(format);
      setExportResult({ path });
    } catch (error) {
      setExportError(error instanceof Error ? error.message : 'Export failed');
    } finally {
      setIsExporting(false);
    }
  };

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
          <span
            role="status"
            aria-live="polite"
            aria-atomic="true"
            className="text-sm text-gray-500 dark:text-gray-400"
          >
            {summaryLabel}
          </span>
          {onExport ? (
            <span className="inline-flex rounded-md shadow-sm" role="group" aria-label="Export history">
              <button
                type="button"
                data-testid="history-export-md-button"
                disabled={exportDisabled}
                onClick={() => void handleExport('markdown')}
                className="rounded-l-md border border-gray-300 px-2.5 py-1.5 text-xs font-semibold text-gray-700 hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
              >
                {isExporting ? 'Exporting…' : 'Export MD'}
              </button>
              <button
                type="button"
                data-testid="history-export-csv-button"
                disabled={exportDisabled}
                onClick={() => void handleExport('csv')}
                className="-ml-px rounded-r-md border border-gray-300 px-2.5 py-1.5 text-xs font-semibold text-gray-700 hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
              >
                CSV
              </button>
            </span>
          ) : null}
          <button
            type="button"
            data-testid="history-clear-all-button"
            ref={clearAllButtonRef}
            disabled={clearDisabled}
            onClick={() => setShowClearConfirm(true)}
            aria-label="Clear all transcript history"
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
          aria-label="Search transcript history"
          placeholder="Search transcripts or language…"
          onChange={(event) => setSearchInput(event.target.value)}
          className="w-full rounded-md border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 placeholder:text-gray-400 focus:border-blue-400 focus:outline-none focus:ring-2 focus:ring-blue-200 dark:border-gray-600 dark:bg-gray-900 dark:text-gray-100 dark:placeholder:text-gray-500 dark:focus:border-blue-500 dark:focus:ring-blue-900/50"
        />
        {searchInput.length > 0 ? (
          <button
            type="button"
            data-testid="history-search-clear"
            onClick={() => setSearchInput('')}
            aria-label="Clear history search query"
            className="rounded-md border border-gray-300 px-2.5 py-2 text-xs font-semibold text-gray-700 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
          >
            Clear
          </button>
        ) : null}
      </div>

      {exportResult ? (
        <div
          role="status"
          data-testid="history-export-success"
          className="rounded-md border border-emerald-600/40 bg-emerald-50 p-2.5 text-xs text-emerald-700 dark:border-emerald-500/30 dark:bg-emerald-900/20 dark:text-emerald-300"
        >
          Exported to{' '}
          <code className="break-all rounded bg-emerald-100 px-1 py-0.5 dark:bg-emerald-800/40">
            {exportResult.path}
          </code>
          <button
            type="button"
            data-testid="history-export-dismiss"
            onClick={() => setExportResult(null)}
            className="ml-2 text-emerald-600 underline hover:text-emerald-500 dark:text-emerald-400"
          >
            Dismiss
          </button>
        </div>
      ) : null}

      {exportError ? (
        <div
          role="alert"
          data-testid="history-export-error"
          className="rounded-md border border-red-600/40 bg-red-50 p-2.5 text-xs text-red-700 dark:border-red-500/30 dark:bg-red-900/20 dark:text-red-300"
        >
          Export failed: {exportError}
        </div>
      ) : null}

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
            ref={clearDialogRef}
            aria-modal="true"
            aria-labelledby="history-clear-title"
            aria-describedby="history-clear-description"
            className="w-full max-w-md rounded-lg border border-gray-600 bg-gray-800 p-4 shadow-xl"
          >
            <h4 id="history-clear-title" className="text-base font-semibold text-gray-100">
              Clear all transcript history?
            </h4>
            <p id="history-clear-description" className="mt-2 text-sm text-gray-300">
              This action cannot be undone.
            </p>

            {clearError ? (
              <p role="alert" className="mt-2 text-xs text-rose-300">{clearError}</p>
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
