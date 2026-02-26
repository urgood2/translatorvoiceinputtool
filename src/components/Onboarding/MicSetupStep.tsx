/**
 * Onboarding step for microphone setup.
 *
 * Lists available audio input devices, lets the user select one,
 * and provides a live mic test with level meter visualization.
 * Cleans up mic test on unmount.
 */

import { useEffect, useCallback } from 'react';
import { useAppStore } from '../../store/appStore';
import { MicrophoneTest } from '../Settings/MicrophoneTest';

export interface MicSetupStepProps {
  onReady: () => void;
}

export function MicSetupStep({ onReady: _onReady }: MicSetupStepProps) {
  const devices = useAppStore((s) => s.devices);
  const selectedDeviceUid = useAppStore((s) => s.selectedDeviceUid);
  const audioLevel = useAppStore((s) => s.audioLevel);
  const isMeterRunning = useAppStore((s) => s.isMeterRunning);
  const refreshDevices = useAppStore((s) => s.refreshDevices);
  const selectDevice = useAppStore((s) => s.selectDevice);
  const startMicTest = useAppStore((s) => s.startMicTest);
  const stopMicTest = useAppStore((s) => s.stopMicTest);

  // Load devices on mount
  useEffect(() => {
    refreshDevices();
  }, [refreshDevices]);

  // Stop mic test on unmount
  useEffect(() => {
    return () => {
      if (useAppStore.getState().isMeterRunning) {
        useAppStore.getState().stopMicTest();
      }
    };
  }, []);

  const handleDeviceChange = useCallback(
    async (e: React.ChangeEvent<HTMLSelectElement>) => {
      const uid = e.target.value || null;
      await selectDevice(uid);
    },
    [selectDevice],
  );

  const currentDevice =
    devices.find((d) => d.uid === selectedDeviceUid) ||
    devices.find((d) => d.is_default);

  return (
    <div>
      <h2 className="text-2xl font-bold mb-4">Microphone Setup</h2>
      <p className="text-gray-600 dark:text-gray-400 mb-6">
        Select your microphone and test that it picks up your voice.
      </p>

      {/* Device selector */}
      <div className="mb-6 text-left">
        <label
          htmlFor="onboarding-mic-select"
          className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
        >
          Input Device
        </label>
        <select
          id="onboarding-mic-select"
          value={selectedDeviceUid || ''}
          onChange={handleDeviceChange}
          className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md
                     bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100
                     focus:ring-2 focus:ring-blue-500 focus:border-transparent"
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
        {currentDevice && (
          <p className="mt-1 text-xs text-gray-500 dark:text-gray-400">
            {currentDevice.sample_rate / 1000}kHz, {currentDevice.channels}{' '}
            channel{currentDevice.channels !== 1 ? 's' : ''}
          </p>
        )}
      </div>

      {/* Mic test with level meter */}
      <div className="text-left">
        <MicrophoneTest
          deviceUid={selectedDeviceUid ?? undefined}
          onStartTest={startMicTest}
          onStopTest={stopMicTest}
          audioLevel={audioLevel}
          isRunning={isMeterRunning}
        />
      </div>
    </div>
  );
}
