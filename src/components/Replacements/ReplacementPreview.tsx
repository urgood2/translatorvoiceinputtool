/**
 * Preview component for testing replacement rules.
 *
 * Features:
 * - Input text area for testing
 * - Real-time preview of transformations
 * - Visual diff showing changes
 * - Support for testing individual rules or entire ruleset
 */

import { invoke } from '@tauri-apps/api/core';
import { useEffect, useMemo, useRef, useState, useId } from 'react';
import type { ReplacementRule } from '../../types';

interface ReplacementPreviewProps {
  rules: ReplacementRule[];
}

type PreviewReplacementResponse = {
  result?: string;
};

const PREVIEW_RPC_DEBOUNCE_MS = 120;

/** Apply replacement rules to text (local mirror of sidecar logic). */
export function applyReplacements(text: string, rules: ReplacementRule[]): string {
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
        // Shared vectors and sidecar rules use Python-style backrefs (\1, \2).
        // Convert to JavaScript replacement syntax ($1, $2).
        const jsReplacement = rule.replacement.replace(/\\(\d+)/g, '$$$1');
        result = result.replace(regex, jsReplacement);
      }
    } catch {
      // Skip invalid rules
    }
  }

  return result;
}

/** Normalize whitespace and common ASR punctuation artifacts. */
export function normalizeText(text: string): string {
  // Normalize unicode spaces to regular space.
  let result = text.replace(/[\u00a0\u2000-\u200a\u202f\u205f\u3000]/g, ' ');

  // First pass: collapse spaces and trim.
  result = result.replace(/ +/g, ' ').trim();

  // Fix spacing around punctuation artifacts from ASR.
  result = result.replace(/ ([,.!?;:])/g, '$1');
  result = result.replace(/([.!?])([A-Z])/g, '$1 $2');

  // Normalize repeated punctuation.
  result = result.replace(/\.{4,}/g, '...');
  result = result.replace(/!{2,}/g, '!');
  result = result.replace(/\?{2,}/g, '?');

  // Final cleanup.
  result = result.replace(/ +/g, ' ').trim();
  return result;
}

function formatIsoDate(now: Date): string {
  const yyyy = String(now.getFullYear());
  const mm = String(now.getMonth() + 1).padStart(2, '0');
  const dd = String(now.getDate()).padStart(2, '0');
  return `${yyyy}-${mm}-${dd}`;
}

function formatIsoTime(now: Date): string {
  const hh = String(now.getHours()).padStart(2, '0');
  const mm = String(now.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

/** Expand macros in text. */
export function expandMacros(text: string, now: Date = new Date()): string {
  const date = formatIsoDate(now);
  const time = formatIsoTime(now);
  const datetime = `${date} ${time}`;

  return text
    .replace(/\{\{date\}\}/g, date)
    .replace(/\{\{time\}\}/g, time)
    .replace(/\{\{datetime\}\}/g, datetime);
}

/** Full preview pipeline mirroring sidecar replacements.process_text. */
export function processPreviewText(
  text: string,
  rules: ReplacementRule[],
  skipMacros = false,
  now: Date = new Date()
): string {
  let result = normalizeText(text);
  if (!skipMacros) {
    result = expandMacros(result, now);
  }
  return applyReplacements(result, rules);
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
  const [result, setResult] = useState('');
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const inputId = useId();
  const outputLabelId = useId();
  const changesLabelId = useId();
  const previewRequestSeqRef = useRef(0);

  const enabledRules = useMemo(() => rules.filter((r) => r.enabled), [rules]);
  useEffect(() => {
    if (!input) {
      setResult('');
      setPreviewLoading(false);
      setPreviewError(null);
      return;
    }

    const localPreview = processPreviewText(input, enabledRules, !expandMacrosEnabled);
    setResult(localPreview);
    setPreviewError(null);

    if (!expandMacrosEnabled) {
      setPreviewLoading(false);
      return;
    }

    let cancelled = false;
    const requestSeq = previewRequestSeqRef.current + 1;
    previewRequestSeqRef.current = requestSeq;
    setPreviewLoading(true);
    const timeoutId = window.setTimeout(() => {
      void Promise.resolve(
        invoke<PreviewReplacementResponse>('preview_replacement', {
          input,
          rules,
        })
      )
        .then((rpcResult) => {
          if (cancelled || requestSeq !== previewRequestSeqRef.current) {
            return;
          }
          if (rpcResult && typeof rpcResult.result === 'string') {
            setResult(rpcResult.result);
            setPreviewError(null);
          }
        })
        .catch(() => {
          if (cancelled || requestSeq !== previewRequestSeqRef.current) {
            return;
          }
          setPreviewError('Sidecar preview unavailable; showing local fallback.');
        })
        .finally(() => {
          if (cancelled || requestSeq !== previewRequestSeqRef.current) {
            return;
          }
          setPreviewLoading(false);
        });
    }, PREVIEW_RPC_DEBOUNCE_MS);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [enabledRules, expandMacrosEnabled, input, rules]);

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
        <label htmlFor={inputId} className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Input Text
        </label>
        <textarea
          id={inputId}
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
        <p id={outputLabelId} className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Output
        </p>
        <div
          role="status"
          aria-live="polite"
          aria-atomic="true"
          aria-labelledby={outputLabelId}
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
          <p id={changesLabelId} className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
            Changes
          </p>
          <div
            role="region"
            aria-labelledby={changesLabelId}
            className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md text-sm
                         bg-gray-50 dark:bg-gray-800"
          >
            <HighlightDiff original={input} result={result} />
          </div>
        </div>
      )}

      {/* Quick stats */}
      <div className="flex items-center gap-4 text-xs text-gray-500 dark:text-gray-400">
        <span>{enabledRules.length} rules active</span>
        {previewLoading && (
          <span data-testid="preview-loading">Previewing…</span>
        )}
        {!previewLoading && previewError && (
          <span data-testid="preview-fallback" className="text-amber-600 dark:text-amber-400">
            {previewError}
          </span>
        )}
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
                type="button"
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
