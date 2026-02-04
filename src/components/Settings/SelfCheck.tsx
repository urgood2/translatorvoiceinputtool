/**
 * Self-check panel for system health status.
 *
 * Features:
 * - Quick health status for all subsystems
 * - Color-coded status indicators (ok/warning/error)
 * - Expandable details for each check
 * - Refresh button to re-run checks
 */

import { useState, useCallback } from 'react';
import type { SelfCheckResult, CheckItem, CheckStatus } from '../../types';

interface SelfCheckProps {
  result: SelfCheckResult | null;
  onRefresh: () => Promise<void>;
  isLoading?: boolean;
}

/** Status icon mapping. */
const STATUS_ICONS: Record<CheckStatus, { icon: string; color: string }> = {
  ok: { icon: '✓', color: 'text-green-500' },
  warning: { icon: '⚠', color: 'text-yellow-500' },
  error: { icon: '✕', color: 'text-red-500' },
};

/** Individual check item row. */
function CheckRow({ label, item }: { label: string; item: CheckItem }) {
  const [expanded, setExpanded] = useState(false);
  const statusConfig = STATUS_ICONS[item.status];

  return (
    <div className="border-b border-gray-100 dark:border-gray-700 last:border-0">
      <button
        onClick={() => item.detail && setExpanded(!expanded)}
        disabled={!item.detail}
        className={`w-full flex items-center gap-3 py-3 px-1 text-left
                   ${item.detail ? 'hover:bg-gray-50 dark:hover:bg-gray-800 cursor-pointer' : 'cursor-default'}`}
      >
        {/* Status icon */}
        <span className={`text-lg font-bold ${statusConfig.color}`}>
          {statusConfig.icon}
        </span>

        {/* Label */}
        <span className="flex-1 font-medium text-gray-900 dark:text-gray-100">
          {label}
        </span>

        {/* Message */}
        <span className="text-sm text-gray-600 dark:text-gray-400">
          {item.message}
        </span>

        {/* Expand indicator */}
        {item.detail && (
          <span className="text-gray-400">
            {expanded ? '▼' : '▶'}
          </span>
        )}
      </button>

      {/* Expanded detail */}
      {expanded && item.detail && (
        <div className="px-10 pb-3 text-sm text-gray-600 dark:text-gray-400 whitespace-pre-wrap">
          {item.detail}
        </div>
      )}
    </div>
  );
}

/** Summary badge showing overall status. */
function StatusSummary({ result }: { result: SelfCheckResult }) {
  const checks = [result.hotkey, result.injection, result.microphone, result.sidecar, result.model];
  const errorCount = checks.filter((c) => c.status === 'error').length;
  const warningCount = checks.filter((c) => c.status === 'warning').length;

  if (errorCount > 0) {
    return (
      <span className="px-3 py-1 rounded-full text-sm font-medium bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300">
        {errorCount} {errorCount === 1 ? 'issue' : 'issues'} found
      </span>
    );
  }

  if (warningCount > 0) {
    return (
      <span className="px-3 py-1 rounded-full text-sm font-medium bg-yellow-100 dark:bg-yellow-900/30 text-yellow-700 dark:text-yellow-300">
        {warningCount} {warningCount === 1 ? 'warning' : 'warnings'}
      </span>
    );
  }

  return (
    <span className="px-3 py-1 rounded-full text-sm font-medium bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300">
      All systems operational
    </span>
  );
}

export function SelfCheck({ result, onRefresh, isLoading }: SelfCheckProps) {
  const [refreshing, setRefreshing] = useState(false);

  const handleRefresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await onRefresh();
    } finally {
      setRefreshing(false);
    }
  }, [onRefresh]);

  // Loading state
  if (!result || isLoading) {
    return (
      <div className="space-y-4">
        <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
          System Health Check
        </h3>
        <div className="flex items-center gap-3 py-8 justify-center">
          <div className="animate-spin h-5 w-5 border-2 border-gray-300 border-t-blue-500 rounded-full" />
          <span className="text-gray-500 dark:text-gray-400">Running checks...</span>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
          System Health Check
        </h3>
        <div className="flex items-center gap-3">
          <StatusSummary result={result} />
          <button
            onClick={handleRefresh}
            disabled={refreshing}
            className="p-2 text-gray-500 hover:text-gray-700 dark:hover:text-gray-300
                       hover:bg-gray-100 dark:hover:bg-gray-700 rounded-md transition-colors
                       disabled:opacity-50"
            title="Refresh checks"
          >
            <svg
              className={`w-5 h-5 ${refreshing ? 'animate-spin' : ''}`}
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
              />
            </svg>
          </button>
        </div>
      </div>

      {/* Check results */}
      <div className="bg-gray-50 dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700">
        <CheckRow label="Hotkey" item={result.hotkey} />
        <CheckRow label="Injection" item={result.injection} />
        <CheckRow label="Microphone" item={result.microphone} />
        <CheckRow label="Sidecar" item={result.sidecar} />
        <CheckRow label="Model" item={result.model} />
      </div>

      {/* Help text */}
      <p className="text-sm text-gray-500 dark:text-gray-400">
        Click on any item with issues to see more details. Use "Refresh" to re-run all checks.
      </p>
    </div>
  );
}

export default SelfCheck;
