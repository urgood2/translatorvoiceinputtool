/**
 * Panel for managing preset replacement rule sets.
 *
 * Features:
 * - List of available presets with descriptions
 * - Enable/disable presets
 * - View preset rules (read-only)
 * - Clear indication of preset vs user rules
 */

import { useState } from 'react';
import type { PresetInfo, ReplacementRule } from '../../types';

interface PresetsPanelProps {
  presets: PresetInfo[];
  enabledPresets: string[];
  onTogglePreset: (presetId: string, enabled: boolean) => void;
  presetRules?: Map<string, ReplacementRule[]>;
}

/** Preset card component. */
function PresetCard({
  preset,
  enabled,
  onToggle,
  rules,
}: {
  preset: PresetInfo;
  enabled: boolean;
  onToggle: (enabled: boolean) => void;
  rules?: ReplacementRule[];
}) {
  const [expanded, setExpanded] = useState(false);
  const rulesPanelId = `preset-rules-${preset.id}`;
  const toggleLabel = `${enabled ? 'Disable' : 'Enable'} preset ${preset.name}`;

  return (
    <div
      className={`border rounded-lg overflow-hidden transition-colors
                 ${enabled
                   ? 'border-blue-200 dark:border-blue-700 bg-blue-50/50 dark:bg-blue-900/20'
                   : 'border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800'}`}
    >
      <div className="p-4">
        <div className="flex items-start justify-between gap-4">
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <h4 className="font-medium text-gray-900 dark:text-gray-100 truncate">
                {preset.name}
              </h4>
              <span className="px-2 py-0.5 text-xs rounded-full bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-400">
                {preset.rule_count} rules
              </span>
            </div>
            <p className="text-sm text-gray-600 dark:text-gray-400 mt-1">
              {preset.description}
            </p>
          </div>
          <label className="relative inline-flex items-center cursor-pointer flex-shrink-0">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => onToggle(e.target.checked)}
              className="sr-only peer"
              aria-label={toggleLabel}
            />
            <div className="w-11 h-6 bg-gray-200 peer-focus:outline-none peer-focus:ring-2
                           peer-focus:ring-blue-500 dark:peer-focus:ring-blue-600
                           rounded-full peer dark:bg-gray-700
                           peer-checked:after:translate-x-full peer-checked:after:border-white
                           after:content-[''] after:absolute after:top-[2px] after:left-[2px]
                           after:bg-white after:border-gray-300 after:border after:rounded-full
                           after:h-5 after:w-5 after:transition-all dark:border-gray-600
                           peer-checked:bg-blue-500" />
          </label>
        </div>

        {/* Expand/collapse rules */}
        {rules && rules.length > 0 && (
          <button
            type="button"
            onClick={() => setExpanded(!expanded)}
            className="mt-3 text-sm text-blue-600 dark:text-blue-400 hover:underline"
            aria-expanded={expanded}
            aria-controls={rulesPanelId}
          >
            {expanded ? 'Hide rules' : 'View rules'}
          </button>
        )}
      </div>

      {/* Expanded rules list */}
      {expanded && rules && (
        <div
          id={rulesPanelId}
          className="border-t border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-900/50 px-4 py-3"
        >
          <div className="max-h-48 overflow-y-auto space-y-2">
            {rules.map((rule) => (
              <div
                key={rule.id}
                className="flex items-center justify-between text-sm py-1"
              >
                <div className="flex items-center gap-2 min-w-0">
                  <span
                    className={`px-1.5 py-0.5 text-xs rounded font-mono
                               ${rule.kind === 'regex'
                                 ? 'bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300'
                                 : 'bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-400'}`}
                  >
                    {rule.kind === 'regex' ? 'rx' : 'lit'}
                  </span>
                  <code className="font-mono text-gray-700 dark:text-gray-300 truncate">
                    {rule.pattern}
                  </code>
                  <span className="text-gray-400 dark:text-gray-500">→</span>
                  <code className="font-mono text-gray-700 dark:text-gray-300 truncate">
                    {rule.replacement || '(delete)'}
                  </code>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export function PresetsPanel({
  presets,
  enabledPresets,
  onTogglePreset,
  presetRules,
}: PresetsPanelProps) {
  if (presets.length === 0) {
    return (
      <div className="text-center py-8">
        <p className="text-gray-500 dark:text-gray-400">
          No presets available
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="font-medium text-gray-900 dark:text-gray-100">
          Preset Rule Sets
        </h3>
        <span className="text-sm text-gray-500 dark:text-gray-400">
          {enabledPresets.length} of {presets.length} enabled
        </span>
      </div>

      <p className="text-sm text-gray-600 dark:text-gray-400">
        Presets are curated replacement rules for common use cases.
        Enable a preset to automatically include its rules.
      </p>

      <div className="space-y-3">
        {presets.map((preset) => (
          <PresetCard
            key={preset.id}
            preset={preset}
            enabled={enabledPresets.includes(preset.id)}
            onToggle={(enabled) => onTogglePreset(preset.id, enabled)}
            rules={presetRules?.get(preset.id)}
          />
        ))}
      </div>
    </div>
  );
}

export default PresetsPanel;
