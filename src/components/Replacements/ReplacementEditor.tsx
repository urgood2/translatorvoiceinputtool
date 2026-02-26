/**
 * Editor dialog for creating/editing replacement rules.
 *
 * Features:
 * - Literal vs regex mode selector
 * - Pattern validation (especially regex)
 * - Word boundary and case sensitivity options
 * - Real-time feedback on regex errors
 */

import { useState, useEffect, useCallback, useId } from 'react';
import type { ReplacementRule, ReplacementKind } from '../../types';

interface ReplacementEditorProps {
  rule?: ReplacementRule | null;
  onSave: (rule: ReplacementRule) => void;
  onCancel: () => void;
  existingPatterns?: string[];
}

/** Generate a UUID v4. */
function generateId(): string {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0;
    const v = c === 'x' ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

/** Validate a regex pattern. */
function validateRegex(pattern: string): string | null {
  if (!pattern) return 'Pattern is required';
  try {
    new RegExp(pattern);
    return null;
  } catch (e) {
    return e instanceof Error ? e.message : 'Invalid regex';
  }
}

export function ReplacementEditor({
  rule,
  onSave,
  onCancel,
  existingPatterns = [],
}: ReplacementEditorProps) {
  const isEditing = !!rule;
  const kindLegendId = useId();
  const literalId = useId();
  const regexId = useId();
  const patternId = useId();
  const patternErrorId = useId();
  const replacementId = useId();
  const replacementHintId = useId();
  const wordBoundaryId = useId();
  const caseSensitiveId = useId();
  const descriptionId = useId();

  const [kind, setKind] = useState<ReplacementKind>(rule?.kind ?? 'literal');
  const [pattern, setPattern] = useState(rule?.pattern ?? '');
  const [replacement, setReplacement] = useState(rule?.replacement ?? '');
  const [wordBoundary, setWordBoundary] = useState(rule?.word_boundary ?? true);
  const [caseSensitive, setCaseSensitive] = useState(rule?.case_sensitive ?? false);
  const [description, setDescription] = useState(rule?.description ?? '');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onEscape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') {
        return;
      }
      event.preventDefault();
      onCancel();
    };

    window.addEventListener('keydown', onEscape);
    return () => window.removeEventListener('keydown', onEscape);
  }, [onCancel]);

  // Validate pattern on changes
  useEffect(() => {
    if (!pattern) {
      setError(null);
      return;
    }

    if (kind === 'regex') {
      const regexError = validateRegex(pattern);
      setError(regexError);
    } else {
      // Check for duplicate literal patterns
      const isDuplicate = existingPatterns.some(
        (p) => p.toLowerCase() === pattern.toLowerCase() && (!rule || rule.pattern !== p)
      );
      setError(isDuplicate ? 'A rule with this pattern already exists' : null);
    }
  }, [pattern, kind, existingPatterns, rule]);

  const handleSave = useCallback(() => {
    if (!pattern.trim()) {
      setError('Pattern is required');
      return;
    }

    if (error) return;

    const newRule: ReplacementRule = {
      id: rule?.id ?? generateId(),
      enabled: rule?.enabled ?? true,
      kind,
      pattern: pattern.trim(),
      replacement,
      word_boundary: kind === 'literal' ? wordBoundary : false,
      case_sensitive: caseSensitive,
      description: description.trim() || undefined,
      origin: rule?.origin ?? 'user',
    };

    onSave(newRule);
  }, [rule, kind, pattern, replacement, wordBoundary, caseSensitive, description, error, onSave]);

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="replacement-editor-title"
        className="bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-lg mx-4"
      >
        <div className="px-6 py-4 border-b border-gray-200 dark:border-gray-700">
          <h3 id="replacement-editor-title" className="text-lg font-medium text-gray-900 dark:text-gray-100">
            {isEditing ? 'Edit Replacement Rule' : 'Add Replacement Rule'}
          </h3>
        </div>

        <div className="px-6 py-4 space-y-4">
          {/* Kind selector */}
          <div>
            <p id={kindLegendId} className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
              Match Type
            </p>
            <div className="flex gap-4" role="radiogroup" aria-labelledby={kindLegendId}>
              <label htmlFor={literalId} className="flex items-center gap-2">
                <input
                  id={literalId}
                  type="radio"
                  name="kind"
                  value="literal"
                  checked={kind === 'literal'}
                  onChange={() => setKind('literal')}
                  className="text-blue-500"
                />
                <span className="text-sm text-gray-700 dark:text-gray-300">Literal text</span>
              </label>
              <label htmlFor={regexId} className="flex items-center gap-2">
                <input
                  id={regexId}
                  type="radio"
                  name="kind"
                  value="regex"
                  checked={kind === 'regex'}
                  onChange={() => setKind('regex')}
                  className="text-blue-500"
                />
                <span className="text-sm text-gray-700 dark:text-gray-300">Regular expression</span>
              </label>
            </div>
          </div>

          {/* Pattern input */}
          <div>
            <label
              htmlFor={patternId}
              className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
            >
              Pattern
            </label>
            <input
              id={patternId}
              type="text"
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              placeholder={kind === 'literal' ? 'Text to find' : 'Regular expression'}
              aria-invalid={Boolean(error)}
              aria-describedby={error ? patternErrorId : undefined}
              className={`w-full px-3 py-2 border rounded-md text-sm
                         bg-white dark:bg-gray-700
                         text-gray-900 dark:text-gray-100
                         placeholder-gray-400 dark:placeholder-gray-500
                         focus:ring-2 focus:ring-blue-500 focus:border-transparent
                         ${error ? 'border-red-300 dark:border-red-600' : 'border-gray-300 dark:border-gray-600'}`}
            />
            {error && (
              <p id={patternErrorId} role="alert" className="mt-1 text-sm text-red-600 dark:text-red-400">{error}</p>
            )}
            {kind === 'regex' && !error && pattern && (
              <p className="mt-1 text-sm text-green-600 dark:text-green-400" aria-live="polite">Valid regex</p>
            )}
          </div>

          {/* Replacement input */}
          <div>
            <label
              htmlFor={replacementId}
              className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
            >
              Replacement
            </label>
            <input
              id={replacementId}
              type="text"
              value={replacement}
              onChange={(e) => setReplacement(e.target.value)}
              placeholder="Replace with (leave empty to delete matches)"
              aria-describedby={kind === 'regex' ? replacementHintId : undefined}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md text-sm
                         bg-white dark:bg-gray-700
                         text-gray-900 dark:text-gray-100
                         placeholder-gray-400 dark:placeholder-gray-500
                         focus:ring-2 focus:ring-blue-500 focus:border-transparent"
            />
            {kind === 'regex' && (
              <p id={replacementHintId} className="mt-1 text-xs text-gray-500 dark:text-gray-400">
                Use $1, $2, etc. for capture groups
              </p>
            )}
          </div>

          {/* Options */}
          <div className="space-y-2">
            {kind === 'literal' && (
              <label htmlFor={wordBoundaryId} className="flex items-center gap-2">
                <input
                  id={wordBoundaryId}
                  type="checkbox"
                  checked={wordBoundary}
                  onChange={(e) => setWordBoundary(e.target.checked)}
                  className="rounded text-blue-500"
                />
                <span className="text-sm text-gray-700 dark:text-gray-300">
                  Match whole words only
                </span>
              </label>
            )}
            <label htmlFor={caseSensitiveId} className="flex items-center gap-2">
              <input
                id={caseSensitiveId}
                type="checkbox"
                checked={caseSensitive}
                onChange={(e) => setCaseSensitive(e.target.checked)}
                className="rounded text-blue-500"
              />
              <span className="text-sm text-gray-700 dark:text-gray-300">
                Case sensitive
              </span>
            </label>
          </div>

          {/* Description */}
          <div>
            <label
              htmlFor={descriptionId}
              className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
            >
              Description (optional)
            </label>
            <input
              id={descriptionId}
              type="text"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Brief description of this rule"
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md text-sm
                         bg-white dark:bg-gray-700
                         text-gray-900 dark:text-gray-100
                         placeholder-gray-400 dark:placeholder-gray-500
                         focus:ring-2 focus:ring-blue-500 focus:border-transparent"
            />
          </div>
        </div>

        {/* Actions */}
        <div className="px-6 py-4 border-t border-gray-200 dark:border-gray-700 flex justify-end gap-3">
          <button
            type="button"
            onClick={onCancel}
            className="px-4 py-2 text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700
                       rounded-md text-sm font-medium transition-colors"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSave}
            disabled={!pattern.trim() || !!error}
            className="px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white
                       rounded-md text-sm font-medium transition-colors
                       disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {isEditing ? 'Save Changes' : 'Add Rule'}
          </button>
        </div>
      </div>
    </div>
  );
}

export default ReplacementEditor;
