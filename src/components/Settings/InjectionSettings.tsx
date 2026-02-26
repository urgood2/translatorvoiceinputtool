/**
 * Text injection settings component.
 *
 * Features:
 * - Paste delay slider (10-500ms)
 * - Restore clipboard toggle
 * - Suffix selector (none, space, newline)
 * - Focus Guard toggle with explanation
 */

import { useState } from 'react';
import type { InjectionConfig } from '../../types';

interface InjectionSettingsProps {
  config: InjectionConfig;
  onChange: (key: keyof InjectionConfig, value: any) => Promise<void>;
  isLoading?: boolean;
}

/** Suffix options for after injected text. */
const SUFFIX_OPTIONS = [
  { value: '', label: 'None', description: 'No suffix after text' },
  { value: ' ', label: 'Space', description: 'Add a space after text' },
  { value: '\n', label: 'Newline', description: 'Add a newline after text' },
];

/** Tooltip component for explanations. */
function Tooltip({ text }: { text: string }) {
  return (
    <span className="ml-1 inline-block" title={text}>
      <svg className="w-4 h-4 text-gray-400 inline" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2}
          d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
    </span>
  );
}

export function InjectionSettings({ config, onChange, isLoading }: InjectionSettingsProps) {
  const [errors, setErrors] = useState<Record<string, string>>({});

  const handleChange = async (key: keyof InjectionConfig, value: any) => {
    setErrors(prev => ({ ...prev, [key]: '' }));
    try {
      await onChange(key, value);
    } catch (e) {
      setErrors(prev => ({
        ...prev,
        [key]: e instanceof Error ? e.message : 'Failed to update setting',
      }));
    }
  };

  return (
    <div className="space-y-4">
      <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
        Text Injection
      </h3>

      {/* Paste delay slider */}
      <div>
        <label htmlFor="paste-delay" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Paste Delay
          <Tooltip text="Time to wait after pasting before restoring clipboard. Increase if paste doesn't complete." />
        </label>
        <div className="flex items-center gap-4">
          <input
            id="paste-delay"
            type="range"
            min={10}
            max={500}
            step={10}
            value={config.paste_delay_ms}
            onChange={(e) => handleChange('paste_delay_ms', parseInt(e.target.value))}
            disabled={isLoading}
            className="flex-1 h-2 bg-gray-200 dark:bg-gray-600 rounded-lg appearance-none cursor-pointer"
          />
          <span className="w-16 text-sm text-gray-900 dark:text-gray-100 text-right">
            {config.paste_delay_ms}ms
          </span>
        </div>
        {errors.paste_delay_ms && (
          <p className="mt-1 text-xs text-red-600 dark:text-red-400">{errors.paste_delay_ms}</p>
        )}
      </div>

      {/* Restore clipboard toggle */}
      <div className="flex items-center justify-between">
        <div>
          <label id="restore-clipboard-label" htmlFor="restore-clipboard" className="font-medium text-gray-900 dark:text-gray-100">
            Restore Clipboard
          </label>
          <Tooltip text="Restore original clipboard contents after pasting transcript" />
          <p id="restore-clipboard-description" className="text-sm text-gray-500 dark:text-gray-400">
            Keep your previous clipboard after injection
          </p>
        </div>
        <button
          type="button"
          id="restore-clipboard"
          role="switch"
          aria-checked={config.restore_clipboard}
          aria-labelledby="restore-clipboard-label"
          aria-describedby="restore-clipboard-description"
          onClick={() => handleChange('restore_clipboard', !config.restore_clipboard)}
          disabled={isLoading}
          className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors
                     ${config.restore_clipboard ? 'bg-blue-500' : 'bg-gray-300 dark:bg-gray-600'}
                     disabled:opacity-50 disabled:cursor-not-allowed`}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform
                       ${config.restore_clipboard ? 'translate-x-6' : 'translate-x-1'}`}
          />
        </button>
      </div>
      {errors.restore_clipboard && (
        <p role="alert" className="text-xs text-red-600 dark:text-red-400">{errors.restore_clipboard}</p>
      )}

      {/* Suffix selector */}
      <div>
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
          Text Suffix
          <Tooltip text="Character to add after the injected text" />
        </label>
        <div className="flex gap-2" role="radiogroup" aria-label="Text suffix">
          {SUFFIX_OPTIONS.map((option) => (
            <button
              type="button"
              key={option.label}
              role="radio"
              aria-checked={config.suffix === option.value}
              onClick={() => handleChange('suffix', option.value)}
              disabled={isLoading}
              className={`px-4 py-2 text-sm rounded-md transition-colors
                         ${config.suffix === option.value
                           ? 'bg-blue-500 text-white'
                           : 'bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600'}
                         disabled:opacity-50 disabled:cursor-not-allowed`}
              title={option.description}
            >
              {option.label}
            </button>
          ))}
        </div>
        {errors.suffix && (
          <p role="alert" className="mt-1 text-xs text-red-600 dark:text-red-400">{errors.suffix}</p>
        )}
      </div>

      {/* Focus Guard toggle */}
      <div className="flex items-center justify-between">
        <div>
          <label id="focus-guard-label" htmlFor="focus-guard" className="font-medium text-gray-900 dark:text-gray-100">
            Focus Guard
          </label>
          <Tooltip text="Prevents text from being injected into the wrong window if focus changes during recording" />
          <p id="focus-guard-description" className="text-sm text-gray-500 dark:text-gray-400">
            Skip injection if window focus changed
          </p>
        </div>
        <button
          type="button"
          id="focus-guard"
          role="switch"
          aria-checked={config.focus_guard_enabled}
          aria-labelledby="focus-guard-label"
          aria-describedby="focus-guard-description"
          onClick={() => handleChange('focus_guard_enabled', !config.focus_guard_enabled)}
          disabled={isLoading}
          className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors
                     ${config.focus_guard_enabled ? 'bg-blue-500' : 'bg-gray-300 dark:bg-gray-600'}
                     disabled:opacity-50 disabled:cursor-not-allowed`}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform
                       ${config.focus_guard_enabled ? 'translate-x-6' : 'translate-x-1'}`}
          />
        </button>
      </div>
      {errors.focus_guard_enabled && (
        <p role="alert" className="text-xs text-red-600 dark:text-red-400">{errors.focus_guard_enabled}</p>
      )}

      {/* Focus Guard explanation */}
      {config.focus_guard_enabled && (
        <div
          className="p-3 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-md"
          aria-live="polite"
        >
          <p className="text-sm text-blue-700 dark:text-blue-300">
            When enabled, text will only be copied to clipboard (not injected) if you switch windows during recording.
            This prevents accidentally typing text into the wrong application.
          </p>
        </div>
      )}
    </div>
  );
}

export default InjectionSettings;
