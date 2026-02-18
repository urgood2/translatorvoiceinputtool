/**
 * Microphone device selector component.
 *
 * Features:
 * - Shows list of available audio input devices
 * - Highlights default device
 * - Live-applies selection with rollback on failure
 */

import { useState } from 'react';
import type { AudioDevice } from '../../types';

interface MicrophoneSelectProps {
  devices: AudioDevice[];
  selectedUid: string | undefined;
  audioCuesEnabled: boolean;
  onDeviceChange: (uid: string) => Promise<void>;
  onAudioCuesChange: (enabled: boolean) => Promise<void>;
  isLoading?: boolean;
}

export function MicrophoneSelect({
  devices,
  selectedUid,
  audioCuesEnabled,
  onDeviceChange,
  onAudioCuesChange,
  isLoading,
}: MicrophoneSelectProps) {
  const [pendingDevice, setPendingDevice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleDeviceChange = async (uid: string) => {
    setError(null);
    setPendingDevice(uid);
    try {
      await onDeviceChange(uid);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to select device');
    } finally {
      setPendingDevice(null);
    }
  };

  const handleAudioCuesToggle = async () => {
    setError(null);
    try {
      await onAudioCuesChange(!audioCuesEnabled);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to update audio cues');
    }
  };

  // Find currently selected device or default
  const currentDevice = devices.find(d => d.uid === selectedUid) ||
                       devices.find(d => d.is_default);

  return (
    <div className="space-y-4">
      <h3 className="text-lg font-medium text-gray-900 dark:text-gray-100">
        Microphone
      </h3>

      {/* Device selector */}
      <div>
        <label htmlFor="mic-select" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
          Input Device
        </label>
        <select
          id="mic-select"
          value={selectedUid || ''}
          onChange={(e) => handleDeviceChange(e.target.value)}
          disabled={isLoading || pendingDevice !== null}
          className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md
                     bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100
                     focus:ring-2 focus:ring-blue-500 focus:border-transparent
                     disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {devices.length === 0 ? (
            <option value="">No devices found</option>
          ) : (
            devices.map((device) => (
              <option key={device.uid} value={device.uid}>
                {device.name} {device.is_default ? '(Default)' : ''}
              </option>
            ))
          )}
        </select>

        {/* Device info */}
        {currentDevice && (
          <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
            {currentDevice.sample_rate / 1000}kHz, {currentDevice.channels} channel{currentDevice.channels !== 1 ? 's' : ''}
          </p>
        )}

        {/* Loading indicator */}
        {pendingDevice && (
          <p className="mt-1 text-xs text-blue-500">Switching device...</p>
        )}
      </div>

      {/* Audio cues toggle */}
      <div className="flex items-center justify-between">
        <div>
          <label htmlFor="audio-cues" className="font-medium text-gray-900 dark:text-gray-100">
            Audio Cues
          </label>
          <p className="text-sm text-gray-500 dark:text-gray-400">
            Play sounds when recording starts/stops
          </p>
        </div>
        <button
          id="audio-cues"
          role="switch"
          aria-checked={audioCuesEnabled}
          onClick={handleAudioCuesToggle}
          disabled={isLoading}
          className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors
                     ${audioCuesEnabled ? 'bg-blue-500' : 'bg-gray-300 dark:bg-gray-600'}
                     disabled:opacity-50 disabled:cursor-not-allowed`}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform
                       ${audioCuesEnabled ? 'translate-x-6' : 'translate-x-1'}`}
          />
        </button>
      </div>

      {/* Error display */}
      {error && (
        <div className="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md">
          <p className="text-sm text-red-600 dark:text-red-400">{error}</p>
        </div>
      )}
    </div>
  );
}

export default MicrophoneSelect;
