/**
 * Hotkey configuration component.
 *
 * Features:
 * - Primary hotkey input for recording
 * - Copy-last hotkey input
 * - Hold/Toggle mode selector
 * - Shows effective mode with reason if different
 */

import { useEffect, useState } from 'react';
import type { HotkeyMode, EffectiveMode, ActivationMode } from '../../types';

interface HotkeyConfigProps {
  primaryHotkey: string;
  copyLastHotkey: string;
  mode: HotkeyMode;
  effectiveMode?: EffectiveMode<ActivationMode>;
  onPrimaryChange: (hotkey: string) => Promise<void>;
  onCopyLastChange: (hotkey: string) => Promise<void>;
  onModeChange: (mode: HotkeyMode) => Promise<void>;
  isLoading?: boolean;
}

interface HotkeyInputProps {
  label: string;
  description: string;
  value: string;
  onChange: (value: string) => Promise<void>;
  disabled?: boolean;
}

function HotkeyInput({ label, description, value, onChange, disabled }: HotkeyInputProps) {
  const [isRecording, setIsRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pendingValue, setPendingValue] = useState<string | null>(null);

  const handleKeyDown = async (e: React.KeyboardEvent) => {
    if (disabled) return;
    if (!isRecording) return;

    e.preventDefault();
    const parts: string[] = [];

    if (e.ctrlKey) parts.push('Ctrl');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');
    if (e.metaKey) parts.push('Meta');

    // Don't accept modifier-only combinations
    if (!['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) {
      parts.push(e.key.length === 1 ? e.key.toUpperCase() : e.key);
    } else {
      return; // Wait for a non-modifier key
    }

    const newHotkey = parts.join('+');
    setPendingValue(newHotkey);
    setIsRecording(false);
    setError(null);

    try {
      await onChange(newHotkey);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to set hotkey');
    } finally {
      setPendingValue(null);
    }
  };

  const displayValue = pendingValue || value;

  useEffect(() => {
    if (disabled) {
      setIsRecording(false);
    }
  }, [disabled]);

  return (
    <div>
      <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
        {label}
      </label>
      <div className="flex gap-2">
        <div
          tabIndex={disabled ? -1 : 0}
          role="button"
          onKeyDown={handleKeyDown}
          aria-disabled={disabled}
          onFocus={() => {
            if (disabled) return;
            setIsRecording(true);
          }}
          onBlur={() => setIsRecording(false)}
          className={`flex-1 px-3 py-2 border rounded-md text-sm
                     ${isRecording
                       ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20 text-blue-700 dark:text-blue-300'
                       : 'border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100'}
                     ${disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}
                     focus:outline-none focus:ring-2 focus:ring-blue-500`}
        >
          {isRecording ? 'Press keys...' : displayValue || 'Click to set'}
        </div>
        {value && (
          <button
            onClick={() => onChange('')}
            disabled={disabled}
            className="px-3 py-2 text-sm text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-md
                       disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Clear
          </button>
        )}
      </div>
      <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">{description}</p>
      {error && (
        <p className="mt-1 text-xs text-red-600 dark:text-red-400">{error}</p>
      )}
    </div>
  );
}

export function HotkeyConfig({
  primaryHotkey,
  copyLastHotkey,
  mode,
  effectiveMode,
  onPrimaryChange,
  onCopyLastChange,
  onModeChange,
  isLoading,
}: HotkeyConfigProps) {
  const [error, setError] = useState<string | null>(null);

  const handleModeChange = async (newMode: HotkeyMode) => {
    setError(null);
    try {
      await onModeChange(newMode);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to change mode');
    }
  };

  const showEffectiveWarning = effectiveMode && effectiveMode.configured !== effectiveMode.effective;

  return (
    <div className="space-y-4">
      <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
        Hotkeys
      </h3>

      {/* Primary hotkey */}
      <HotkeyInput
        label="Recording Hotkey"
        description="Start/stop voice recording"
        value={primaryHotkey}
        onChange={onPrimaryChange}
        disabled={isLoading}
      />

      {/* Copy last hotkey */}
      <HotkeyInput
        label="Copy Last Hotkey"
        description="Copy the most recent transcript to clipboard"
        value={copyLastHotkey}
        onChange={onCopyLastChange}
        disabled={isLoading}
      />

      {/* Mode selector */}
      <div>
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
          Activation Mode
        </label>
        <div className="flex gap-4">
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="radio"
              name="hotkey-mode"
              value="hold"
              checked={mode === 'hold'}
              onChange={() => handleModeChange('hold')}
              disabled={isLoading}
              className="text-blue-500 focus:ring-blue-500"
            />
            <span className="text-gray-900 dark:text-gray-100">Hold to record</span>
          </label>
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="radio"
              name="hotkey-mode"
              value="toggle"
              checked={mode === 'toggle'}
              onChange={() => handleModeChange('toggle')}
              disabled={isLoading}
              className="text-blue-500 focus:ring-blue-500"
            />
            <span className="text-gray-900 dark:text-gray-100">Press to toggle</span>
          </label>
        </div>
        <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
          {mode === 'hold'
            ? 'Recording while hotkey is held down'
            : 'Press once to start, press again to stop'}
        </p>
      </div>

      {/* Effective mode warning */}
      {showEffectiveWarning && (
        <div className="p-3 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-md">
          <p className="text-sm text-yellow-700 dark:text-yellow-300">
            <strong>Note:</strong> Using "{effectiveMode.effective}" mode instead of "{effectiveMode.configured}".
          </p>
          {effectiveMode.reason && (
            <p className="text-xs text-yellow-600 dark:text-yellow-400 mt-1">
              Reason: {effectiveMode.reason}
            </p>
          )}
        </div>
      )}

      {/* Error display */}
      {error && (
        <div className="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md">
          <p className="text-sm text-red-600 dark:text-red-400">{error}</p>
        </div>
      )}
    </div>
  );
}

export default HotkeyConfig;
