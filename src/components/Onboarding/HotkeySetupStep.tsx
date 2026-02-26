/**
 * Onboarding step for hotkey configuration.
 *
 * Shows the current recording hotkey, lets the user re-record it,
 * and explains hold vs toggle activation modes.
 */

import { useState, useCallback } from 'react';
import { useAppStore } from '../../store/appStore';
import type { HotkeyMode } from '../../types';

export interface HotkeySetupStepProps {
  onReady: () => void;
}

export function HotkeySetupStep({ onReady: _onReady }: HotkeySetupStepProps) {
  const config = useAppStore((s) => s.config);
  const updateHotkeyConfig = useAppStore((s) => s.updateHotkeyConfig);

  const primaryHotkey = config?.hotkeys.primary ?? 'Ctrl+Shift+Space';
  const mode = config?.hotkeys.mode ?? 'hold';

  const [isRecording, setIsRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleKeyDown = useCallback(
    async (e: React.KeyboardEvent) => {
      if (!isRecording) return;

      e.preventDefault();
      const parts: string[] = [];

      if (e.ctrlKey) parts.push('Ctrl');
      if (e.altKey) parts.push('Alt');
      if (e.shiftKey) parts.push('Shift');
      if (e.metaKey) parts.push('Meta');

      // Wait for a non-modifier key
      if (['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) return;

      parts.push(e.key.length === 1 ? e.key.toUpperCase() : e.key);

      const newHotkey = parts.join('+');
      setIsRecording(false);
      setError(null);

      try {
        await updateHotkeyConfig({ primary: newHotkey });
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to set hotkey');
      }
    },
    [isRecording, updateHotkeyConfig],
  );

  const handleModeChange = useCallback(
    async (newMode: HotkeyMode) => {
      setError(null);
      try {
        await updateHotkeyConfig({ mode: newMode });
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to change mode');
      }
    },
    [updateHotkeyConfig],
  );

  return (
    <div>
      <h2 className="text-2xl font-bold mb-4">Hotkey Configuration</h2>
      <p className="text-gray-600 dark:text-gray-400 mb-6">
        Choose the keyboard shortcut to start and stop voice recording.
      </p>

      {/* Primary hotkey recorder */}
      <div className="mb-6 text-left">
        <label id="onboarding-hotkey-label" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Recording Hotkey
        </label>
        <div
          tabIndex={0}
          role="button"
          aria-label="Recording hotkey"
          aria-labelledby="onboarding-hotkey-label"
          aria-describedby="onboarding-hotkey-description"
          onKeyDown={handleKeyDown}
          onClick={() => setIsRecording(true)}
          onFocus={() => setIsRecording(true)}
          onBlur={() => setIsRecording(false)}
          className={`w-full px-4 py-3 border rounded-md text-sm text-center font-mono
                     ${
                       isRecording
                         ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20 text-blue-700 dark:text-blue-300'
                         : 'border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100'
                     }
                     cursor-pointer focus:outline-none focus:ring-2 focus:ring-blue-500`}
        >
          {isRecording ? 'Press keys...' : primaryHotkey || 'Click to set'}
        </div>
        <p id="onboarding-hotkey-description" className="mt-1 text-xs text-gray-500 dark:text-gray-400">
          Click the box above and press your preferred key combination.
        </p>
      </div>

      {/* Mode selector */}
      <div className="mb-6 text-left">
        <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
          Activation Mode
        </label>
        <div className="space-y-3">
          <label className="flex items-start gap-3 cursor-pointer">
            <input
              type="radio"
              name="onboarding-hotkey-mode"
              value="hold"
              checked={mode === 'hold'}
              onChange={() => handleModeChange('hold')}
              className="mt-1 text-blue-500 focus:ring-blue-500"
            />
            <div>
              <span className="text-gray-900 dark:text-gray-100 font-medium">
                Hold to record
              </span>
              <p className="text-xs text-gray-500 dark:text-gray-400">
                Recording while the hotkey is held down. Release to stop.
              </p>
            </div>
          </label>
          <label className="flex items-start gap-3 cursor-pointer">
            <input
              type="radio"
              name="onboarding-hotkey-mode"
              value="toggle"
              checked={mode === 'toggle'}
              onChange={() => handleModeChange('toggle')}
              className="mt-1 text-blue-500 focus:ring-blue-500"
            />
            <div>
              <span className="text-gray-900 dark:text-gray-100 font-medium">
                Press to toggle
              </span>
              <p className="text-xs text-gray-500 dark:text-gray-400">
                Press once to start recording, press again to stop.
              </p>
            </div>
          </label>
        </div>
      </div>

      {/* Error display */}
      {error && (
        <div role="alert" className="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md">
          <p className="text-sm text-red-600 dark:text-red-400">{error}</p>
        </div>
      )}
    </div>
  );
}
