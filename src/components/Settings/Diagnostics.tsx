/**
 * Diagnostics panel for bug reports.
 *
 * Features:
 * - Generates comprehensive diagnostics text blob
 * - Redacts sensitive paths and transcript contents
 * - Bounded size (truncates if too large)
 * - One-click copy to clipboard
 * - Shows recent logs (redacted)
 */

import { useState, useMemo, useCallback } from 'react';
import type { DiagnosticsReport, Capabilities, AppConfig, SelfCheckResult, LogEntry } from '../../types';

interface DiagnosticsProps {
  report: DiagnosticsReport | null;
  onRefresh: () => Promise<void>;
  isLoading?: boolean;
}

/** Max diagnostics output size in characters. */
const MAX_DIAGNOSTICS_SIZE = 50000;

/** Paths that should be redacted. */
const SENSITIVE_PATH_PATTERNS = [
  /\/Users\/[^/]+/g,        // macOS user paths
  /\/home\/[^/]+/g,         // Linux user paths
  /C:\\Users\\[^\\]+/g,     // Windows user paths
  /[A-Za-z]:\\Users\\[^\\]+/g,
];

/** Redact sensitive paths from text. */
function redactPaths(text: string): string {
  let result = text;
  for (const pattern of SENSITIVE_PATH_PATTERNS) {
    result = result.replace(pattern, '[REDACTED_PATH]');
  }
  return result;
}

/** Format capabilities for diagnostics. */
function formatCapabilities(caps: Capabilities): string {
  const lines: string[] = [];

  // Display server
  if (caps.display_server.type === 'wayland') {
    lines.push(`Display: Wayland (${caps.display_server.compositor ?? 'unknown compositor'})`);
  } else {
    lines.push(`Display: ${caps.display_server.type}`);
  }

  // Hotkey
  lines.push(`Hotkey Mode: ${caps.hotkey_mode.effective} (configured: ${caps.hotkey_mode.configured})`);
  if (caps.hotkey_mode.reason) {
    lines.push(`  Reason: ${caps.hotkey_mode.reason}`);
  }

  // Injection
  lines.push(`Injection: ${caps.injection_method.effective} (configured: ${caps.injection_method.configured})`);
  if (caps.injection_method.reason) {
    lines.push(`  Reason: ${caps.injection_method.reason}`);
  }

  // Permissions
  lines.push(`Microphone Permission: ${caps.permissions.microphone}`);
  if (caps.permissions.accessibility) {
    lines.push(`Accessibility Permission: ${caps.permissions.accessibility}`);
  }

  // Feature availability
  lines.push(`Hotkey Press: ${caps.hotkey_press_available ? 'available' : 'unavailable'}`);
  lines.push(`Hotkey Release: ${caps.hotkey_release_available ? 'available' : 'unavailable'}`);
  lines.push(`Keystroke Injection: ${caps.keystroke_injection_available ? 'available' : 'unavailable'}`);
  lines.push(`Clipboard: ${caps.clipboard_available ? 'available' : 'unavailable'}`);

  return lines.join('\n');
}

/** Format config for diagnostics (redacted). */
function formatConfig(config: AppConfig): string {
  const redacted = {
    ...config,
    // Don't include replacement patterns (may contain personal data)
    replacements: `[${config.replacements.length} rules]`,
    // Redact device UID
    audio: {
      ...config.audio,
      device_uid: config.audio.device_uid ? '[REDACTED]' : null,
    },
  };

  return JSON.stringify(redacted, null, 2);
}

/** Format self-check results. */
function formatSelfCheck(check: SelfCheckResult): string {
  const formatItem = (name: string, item: { status: string; message: string }) =>
    `${name}: [${item.status.toUpperCase()}] ${item.message}`;

  return [
    formatItem('Hotkey', check.hotkey),
    formatItem('Injection', check.injection),
    formatItem('Microphone', check.microphone),
    formatItem('Sidecar', check.sidecar),
    formatItem('Model', check.model),
  ].join('\n');
}

/** Generate full diagnostics text. */
function formatRecentLogs(logs: LogEntry[]): string {
  if (logs.length === 0) {
    return 'No recent log entries available.';
  }

  return logs
    .map((entry) => `${entry.timestamp} [${entry.level}] ${entry.target}: ${entry.message}`)
    .join('\n');
}

/** Generate full diagnostics text. */
function generateDiagnosticsText(report: DiagnosticsReport): string {
  const sections: string[] = [];

  // Header
  sections.push('=== OpenVoicy Diagnostics Report ===');
  sections.push(`Generated: ${new Date().toISOString()}`);
  sections.push(`Version: ${report.version}`);
  sections.push(`Platform: ${report.platform}`);
  sections.push('');

  // Self-check
  sections.push('--- Self Check ---');
  sections.push(formatSelfCheck(report.self_check));
  sections.push('');

  // Capabilities
  sections.push('--- Capabilities ---');
  sections.push(formatCapabilities(report.capabilities));
  sections.push('');

  // Config
  sections.push('--- Configuration (Redacted) ---');
  sections.push(formatConfig(report.config));
  sections.push('');

  // Raw diagnostics from capabilities
  if (report.capabilities.diagnostics) {
    sections.push('--- Platform Diagnostics ---');
    sections.push(redactPaths(report.capabilities.diagnostics));
    sections.push('');
  }

  // Recent logs (already redacted by backend logger)
  sections.push('--- Recent Logs ---');
  sections.push(redactPaths(formatRecentLogs(report.recent_logs)));
  sections.push('');

  // Footer
  sections.push('=== End of Report ===');

  let text = sections.join('\n');

  // Truncate if too large
  if (text.length > MAX_DIAGNOSTICS_SIZE) {
    text = text.substring(0, MAX_DIAGNOSTICS_SIZE) + '\n\n[TRUNCATED - Report exceeded maximum size]';
  }

  return redactPaths(text);
}

export function Diagnostics({ report, onRefresh, isLoading }: DiagnosticsProps) {
  const [copied, setCopied] = useState(false);
  const [refreshing, setRefreshing] = useState(false);

  const diagnosticsText = useMemo(() => {
    if (!report) return '';
    return generateDiagnosticsText(report);
  }, [report]);

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(diagnosticsText);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback for older browsers
      const textarea = document.createElement('textarea');
      textarea.value = diagnosticsText;
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand('copy');
      document.body.removeChild(textarea);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }, [diagnosticsText]);

  const handleRefresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await onRefresh();
    } finally {
      setRefreshing(false);
    }
  }, [onRefresh]);

  // Loading state
  if (!report || isLoading) {
    return (
      <div className="space-y-4">
        <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
          Diagnostics
        </h3>
        <div className="flex items-center gap-3 py-8 justify-center">
          <div className="animate-spin h-5 w-5 border-2 border-gray-300 border-t-blue-500 rounded-full" />
          <span className="text-gray-500 dark:text-gray-400">Gathering diagnostics...</span>
        </div>
      </div>
    );
  }

  const charCount = diagnosticsText.length;
  const lineCount = diagnosticsText.split('\n').length;

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
          Diagnostics
        </h3>
        <div className="flex items-center gap-2">
          <button
            onClick={handleRefresh}
            disabled={refreshing}
            className="px-3 py-1.5 text-sm text-gray-600 dark:text-gray-400
                       hover:bg-gray-100 dark:hover:bg-gray-700 rounded-md transition-colors
                       disabled:opacity-50"
          >
            {refreshing ? 'Refreshing...' : 'Refresh'}
          </button>
          <button
            onClick={handleCopy}
            className={`px-3 py-1.5 text-sm rounded-md transition-colors
                       ${copied
                         ? 'bg-green-500 text-white'
                         : 'bg-blue-500 hover:bg-blue-600 text-white'}`}
          >
            {copied ? 'Copied!' : 'Copy to Clipboard'}
          </button>
        </div>
      </div>

      {/* Description */}
      <p className="text-sm text-gray-600 dark:text-gray-400">
        This diagnostic report contains system information that can help troubleshoot issues.
        Personal paths and sensitive data are automatically redacted.
        Copy and include this when reporting bugs.
      </p>

      {/* Diagnostics output */}
      <div className="relative">
        <pre
          className="p-4 bg-gray-900 dark:bg-black text-green-400 text-xs font-mono
                     rounded-lg overflow-auto max-h-96 whitespace-pre-wrap break-words"
        >
          {diagnosticsText}
        </pre>

        {/* Stats overlay */}
        <div className="absolute bottom-2 right-2 px-2 py-1 bg-black/50 rounded text-xs text-gray-400">
          {lineCount} lines | {charCount.toLocaleString()} chars
        </div>
      </div>

      {/* Privacy notice */}
      <div className="p-3 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-md">
        <p className="text-sm text-blue-700 dark:text-blue-300">
          <strong>Privacy:</strong> This report does not include transcript text or personal data.
          Paths are automatically redacted. Review the content before sharing.
        </p>
      </div>
    </div>
  );
}

export default Diagnostics;
