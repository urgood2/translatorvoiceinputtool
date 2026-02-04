/**
 * Preview component for testing replacement rules.
 *
 * Features:
 * - Input text area for testing
 * - Real-time preview of transformations
 * - Visual diff showing changes
 * - Support for testing individual rules or entire ruleset
 */

import { useState, useMemo } from 'react';
import type { ReplacementRule } from '../../types';

interface ReplacementPreviewProps {
  rules: ReplacementRule[];
}

/** Apply replacement rules to text (local mirror of sidecar logic). */
function applyReplacements(text: string, rules: ReplacementRule[]): string {
  let result = text;

  for (const rule of rules) {
    if (!rule.enabled) continue;

    try {
      if (rule.kind === 'literal') {
        // Build regex for literal match
        const escaped = rule.pattern.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        const boundary = rule.word_boundary ? '\\b' : '';
        const flags = rule.case_sensitive ? 'g' : 'gi';
        const regex = new RegExp(`${boundary}${escaped}${boundary}`, flags);
        result = result.replace(regex, rule.replacement);
      } else {
        // Regex match
        const flags = rule.case_sensitive ? 'g' : 'gi';
        const regex = new RegExp(rule.pattern, flags);
        result = result.replace(regex, rule.replacement);
      }
    } catch {
      // Skip invalid rules
    }
  }

  return result;
}

/** Expand macros in text. */
function expandMacros(text: string): string {
  const now = new Date();
  const date = now.toLocaleDateString();
  const time = now.toLocaleTimeString();
  const datetime = `${date} ${time}`;

  return text
    .replace(/\{\{date\}\}/g, date)
    .replace(/\{\{time\}\}/g, time)
    .replace(/\{\{datetime\}\}/g, datetime);
}

/** Highlight differences between original and result. */
function HighlightDiff({ original, result }: { original: string; result: string }) {
  if (original === result) {
    return (
      <span className="text-gray-500 dark:text-gray-400 italic">No changes</span>
    );
  }

  // Simple word-based diff for visualization
  const originalWords = original.split(/(\s+)/);
  const resultWords = result.split(/(\s+)/);

  // Find common prefix length
  let commonPrefix = 0;
  while (
    commonPrefix < originalWords.length &&
    commonPrefix < resultWords.length &&
    originalWords[commonPrefix] === resultWords[commonPrefix]
  ) {
    commonPrefix++;
  }

  // Find common suffix length
  let commonSuffix = 0;
  while (
    commonSuffix < originalWords.length - commonPrefix &&
    commonSuffix < resultWords.length - commonPrefix &&
    originalWords[originalWords.length - 1 - commonSuffix] ===
      resultWords[resultWords.length - 1 - commonSuffix]
  ) {
    commonSuffix++;
  }

  const prefixPart = originalWords.slice(0, commonPrefix).join('');
  const suffixPart = originalWords.slice(originalWords.length - commonSuffix).join('');
  const removedPart = originalWords
    .slice(commonPrefix, originalWords.length - commonSuffix)
    .join('');
  const addedPart = resultWords
    .slice(commonPrefix, resultWords.length - commonSuffix)
    .join('');

  return (
    <span>
      {prefixPart}
      {removedPart && (
        <span className="bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300 line-through">
          {removedPart}
        </span>
      )}
      {addedPart && (
        <span className="bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300">
          {addedPart}
        </span>
      )}
      {suffixPart}
    </span>
  );
}

export function ReplacementPreview({ rules }: ReplacementPreviewProps) {
  const [input, setInput] = useState('');
  const [expandMacrosEnabled, setExpandMacrosEnabled] = useState(true);

  const enabledRules = useMemo(() => rules.filter((r) => r.enabled), [rules]);

  const result = useMemo(() => {
    let text = input;
    if (expandMacrosEnabled) {
      text = expandMacros(text);
    }
    return applyReplacements(text, enabledRules);
  }, [input, enabledRules, expandMacrosEnabled]);

  const hasChanges = input !== result;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h4 className="font-medium text-gray-900 dark:text-gray-100">
          Test Rules
        </h4>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={expandMacrosEnabled}
            onChange={(e) => setExpandMacrosEnabled(e.target.checked)}
            className="rounded text-blue-500"
          />
          <span className="text-gray-600 dark:text-gray-400">Expand macros</span>
        </label>
      </div>

      {/* Input */}
      <div>
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Input Text
        </label>
        <textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Type or paste text to test replacement rules..."
          rows={3}
          className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md text-sm
                     bg-white dark:bg-gray-700
                     text-gray-900 dark:text-gray-100
                     placeholder-gray-400 dark:placeholder-gray-500
                     focus:ring-2 focus:ring-blue-500 focus:border-transparent
                     resize-none"
        />
      </div>

      {/* Output */}
      <div>
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Output
        </label>
        <div
          className={`w-full px-3 py-2 border rounded-md text-sm min-h-[4rem]
                     ${hasChanges
                       ? 'border-green-300 dark:border-green-600 bg-green-50 dark:bg-green-900/20'
                       : 'border-gray-300 dark:border-gray-600 bg-gray-50 dark:bg-gray-800'}`}
        >
          {input ? (
            <span className="text-gray-900 dark:text-gray-100 whitespace-pre-wrap">
              {result}
            </span>
          ) : (
            <span className="text-gray-400 dark:text-gray-500 italic">
              Output will appear here
            </span>
          )}
        </div>
      </div>

      {/* Diff view */}
      {input && (
        <div>
          <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
            Changes
          </label>
          <div className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md text-sm
                         bg-gray-50 dark:bg-gray-800">
            <HighlightDiff original={input} result={result} />
          </div>
        </div>
      )}

      {/* Quick stats */}
      <div className="flex items-center gap-4 text-xs text-gray-500 dark:text-gray-400">
        <span>{enabledRules.length} rules active</span>
        {input && (
          <>
            <span>|</span>
            <span>{input.length} chars input</span>
            <span>|</span>
            <span>{result.length} chars output</span>
          </>
        )}
      </div>

      {/* Macro hints */}
      {expandMacrosEnabled && (
        <div className="p-3 bg-gray-50 dark:bg-gray-800 rounded-md">
          <p className="text-xs font-medium text-gray-600 dark:text-gray-400 mb-2">
            Available macros:
          </p>
          <div className="flex flex-wrap gap-2">
            {['{{date}}', '{{time}}', '{{datetime}}'].map((macro) => (
              <button
                key={macro}
                onClick={() => setInput((prev) => prev + macro)}
                className="px-2 py-1 text-xs font-mono bg-white dark:bg-gray-700
                           border border-gray-200 dark:border-gray-600 rounded
                           hover:bg-gray-100 dark:hover:bg-gray-600 transition-colors"
              >
                {macro}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export default ReplacementPreview;
