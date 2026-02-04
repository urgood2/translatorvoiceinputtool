/**
 * Main replacement rules list component.
 *
 * Features:
 * - List all user replacement rules
 * - Enable/disable individual rules
 * - Drag-to-reorder (rules apply in order)
 * - Edit and delete actions
 * - Add new rule button
 * - Import/export functionality
 */

import { useState, useCallback } from 'react';
import type { ReplacementRule } from '../../types';
import { ReplacementEditor } from './ReplacementEditor';

interface ReplacementListProps {
  rules: ReplacementRule[];
  onChange: (rules: ReplacementRule[]) => void;
  isLoading?: boolean;
}

/** Single rule row component. */
function RuleRow({
  rule,
  index,
  onToggle,
  onEdit,
  onDelete,
  onMoveUp,
  onMoveDown,
  isFirst,
  isLast,
}: {
  rule: ReplacementRule;
  index: number;
  onToggle: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  isFirst: boolean;
  isLast: boolean;
}) {
  const isPreset = rule.origin === 'preset';

  return (
    <div
      className={`group flex items-center gap-3 p-3 border rounded-lg transition-colors
                 ${rule.enabled
                   ? 'bg-white dark:bg-gray-800 border-gray-200 dark:border-gray-700'
                   : 'bg-gray-50 dark:bg-gray-900 border-gray-100 dark:border-gray-800 opacity-60'}`}
    >
      {/* Enable toggle */}
      <input
        type="checkbox"
        checked={rule.enabled}
        onChange={onToggle}
        className="rounded text-blue-500 flex-shrink-0"
        title={rule.enabled ? 'Disable rule' : 'Enable rule'}
      />

      {/* Rule number */}
      <span className="w-6 text-center text-xs text-gray-400 dark:text-gray-500 flex-shrink-0">
        {index + 1}
      </span>

      {/* Rule type badge */}
      <span
        className={`px-1.5 py-0.5 text-xs rounded font-mono flex-shrink-0
                   ${rule.kind === 'regex'
                     ? 'bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300'
                     : 'bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-400'}`}
      >
        {rule.kind === 'regex' ? 'rx' : 'lit'}
      </span>

      {/* Pattern and replacement */}
      <div className="flex-1 min-w-0 flex items-center gap-2">
        <code className="font-mono text-sm text-gray-700 dark:text-gray-300 truncate">
          {rule.pattern}
        </code>
        <span className="text-gray-400 dark:text-gray-500 flex-shrink-0">→</span>
        <code className="font-mono text-sm text-gray-700 dark:text-gray-300 truncate">
          {rule.replacement || <span className="italic text-gray-400">(delete)</span>}
        </code>
      </div>

      {/* Description */}
      {rule.description && (
        <span className="text-xs text-gray-500 dark:text-gray-400 truncate max-w-[150px]" title={rule.description}>
          {rule.description}
        </span>
      )}

      {/* Origin badge */}
      {isPreset && (
        <span className="px-2 py-0.5 text-xs rounded-full bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300">
          preset
        </span>
      )}

      {/* Actions - visible on hover */}
      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
        {/* Move buttons */}
        <button
          onClick={onMoveUp}
          disabled={isFirst}
          className="p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 disabled:opacity-30"
          title="Move up"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 15l7-7 7 7" />
          </svg>
        </button>
        <button
          onClick={onMoveDown}
          disabled={isLast}
          className="p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 disabled:opacity-30"
          title="Move down"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
          </svg>
        </button>

        {/* Edit */}
        {!isPreset && (
          <button
            onClick={onEdit}
            className="p-1 text-gray-400 hover:text-blue-500"
            title="Edit rule"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                    d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
            </svg>
          </button>
        )}

        {/* Delete */}
        {!isPreset && (
          <button
            onClick={onDelete}
            className="p-1 text-gray-400 hover:text-red-500"
            title="Delete rule"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
                    d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
}

export function ReplacementList({
  rules,
  onChange,
  isLoading,
}: ReplacementListProps) {
  const [editingRule, setEditingRule] = useState<ReplacementRule | null>(null);
  const [isAdding, setIsAdding] = useState(false);

  // Filter user rules (not preset)
  const userRules = rules.filter((r) => r.origin !== 'preset');
  const existingPatterns = userRules.map((r) => r.pattern);

  const handleToggle = useCallback((id: string) => {
    onChange(
      rules.map((r) =>
        r.id === id ? { ...r, enabled: !r.enabled } : r
      )
    );
  }, [rules, onChange]);

  const handleDelete = useCallback((id: string) => {
    onChange(rules.filter((r) => r.id !== id));
  }, [rules, onChange]);

  const handleMoveUp = useCallback((index: number) => {
    if (index === 0) return;
    const newRules = [...rules];
    [newRules[index - 1], newRules[index]] = [newRules[index], newRules[index - 1]];
    onChange(newRules);
  }, [rules, onChange]);

  const handleMoveDown = useCallback((index: number) => {
    if (index === rules.length - 1) return;
    const newRules = [...rules];
    [newRules[index], newRules[index + 1]] = [newRules[index + 1], newRules[index]];
    onChange(newRules);
  }, [rules, onChange]);

  const handleSave = useCallback((rule: ReplacementRule) => {
    if (editingRule) {
      // Update existing rule
      onChange(rules.map((r) => (r.id === rule.id ? rule : r)));
    } else {
      // Add new rule
      onChange([...rules, rule]);
    }
    setEditingRule(null);
    setIsAdding(false);
  }, [rules, onChange, editingRule]);

  const handleExport = useCallback(() => {
    const exportData = JSON.stringify(userRules, null, 2);
    const blob = new Blob([exportData], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'replacement-rules.json';
    a.click();
    URL.revokeObjectURL(url);
  }, [userRules]);

  const handleImport = useCallback(() => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.json';
    input.onchange = async (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (!file) return;

      try {
        const text = await file.text();
        const imported = JSON.parse(text) as ReplacementRule[];

        // Validate imported rules
        if (!Array.isArray(imported)) {
          throw new Error('Invalid format: expected an array');
        }

        // Assign new IDs to avoid conflicts
        const newRules = imported.map((r) => ({
          ...r,
          id: crypto.randomUUID?.() ?? `${Date.now()}-${Math.random()}`,
          origin: 'user' as const,
        }));

        onChange([...rules, ...newRules]);
      } catch (err) {
        alert(`Failed to import: ${err instanceof Error ? err.message : 'Unknown error'}`);
      }
    };
    input.click();
  }, [rules, onChange]);

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h3 className="font-medium text-gray-900 dark:text-gray-100">
          Replacement Rules
        </h3>
        <div className="flex items-center gap-2">
          <button
            onClick={handleImport}
            disabled={isLoading}
            className="px-3 py-1.5 text-sm text-gray-600 dark:text-gray-400
                       hover:bg-gray-100 dark:hover:bg-gray-700 rounded-md transition-colors"
          >
            Import
          </button>
          <button
            onClick={handleExport}
            disabled={isLoading || userRules.length === 0}
            className="px-3 py-1.5 text-sm text-gray-600 dark:text-gray-400
                       hover:bg-gray-100 dark:hover:bg-gray-700 rounded-md transition-colors
                       disabled:opacity-50"
          >
            Export
          </button>
          <button
            onClick={() => setIsAdding(true)}
            disabled={isLoading}
            className="px-3 py-1.5 text-sm bg-blue-500 hover:bg-blue-600 text-white
                       rounded-md transition-colors disabled:opacity-50"
          >
            Add Rule
          </button>
        </div>
      </div>

      {/* Info text */}
      <p className="text-sm text-gray-600 dark:text-gray-400">
        Rules are applied in order from top to bottom. Drag to reorder.
      </p>

      {/* Rules list */}
      {userRules.length === 0 ? (
        <div className="text-center py-8 border-2 border-dashed border-gray-200 dark:border-gray-700 rounded-lg">
          <p className="text-gray-500 dark:text-gray-400 mb-2">
            No custom replacement rules yet
          </p>
          <button
            onClick={() => setIsAdding(true)}
            className="text-blue-500 hover:text-blue-600 text-sm font-medium"
          >
            Add your first rule
          </button>
        </div>
      ) : (
        <div className="space-y-2">
          {userRules.map((rule, index) => (
            <RuleRow
              key={rule.id}
              rule={rule}
              index={index}
              onToggle={() => handleToggle(rule.id)}
              onEdit={() => setEditingRule(rule)}
              onDelete={() => handleDelete(rule.id)}
              onMoveUp={() => handleMoveUp(index)}
              onMoveDown={() => handleMoveDown(index)}
              isFirst={index === 0}
              isLast={index === userRules.length - 1}
            />
          ))}
        </div>
      )}

      {/* Stats */}
      <div className="flex items-center gap-4 text-xs text-gray-500 dark:text-gray-400">
        <span>{userRules.length} rules total</span>
        <span>{userRules.filter((r) => r.enabled).length} enabled</span>
      </div>

      {/* Editor modal */}
      {(isAdding || editingRule) && (
        <ReplacementEditor
          rule={editingRule}
          onSave={handleSave}
          onCancel={() => {
            setEditingRule(null);
            setIsAdding(false);
          }}
          existingPatterns={existingPatterns}
        />
      )}
    </div>
  );
}

export default ReplacementList;
