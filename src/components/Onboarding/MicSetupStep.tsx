/**
 * Onboarding step for microphone setup.
 *
 * Lists available audio input devices, lets the user select one,
 * and provides a live mic test with level meter visualization.
 * Cleans up mic test on unmount.
 */

import { useEffect, useCallback } from 'react';
import { useState } from 'react';
import { useAppStore } from '../../store/appStore';
import { MicrophoneTest } from '../Settings/MicrophoneTest';

export interface MicSetupStepProps {
  onReady: () => void;
}

export function MicSetupStep({ onReady }: MicSetupStepProps) {
  const devices = useAppStore((s) => s.devices);
  const selectedDeviceUid = useAppStore((s) => s.selectedDeviceUid);
  const audioLevel = useAppStore((s) => s.audioLevel);
  const isMeterRunning = useAppStore((s) => s.isMeterRunning);
  const refreshDevices = useAppStore((s) => s.refreshDevices);
  const selectDevice = useAppStore((s) => s.selectDevice);
  const startMicTest = useAppStore((s) => s.startMicTest);
  const stopMicTest = useAppStore((s) => s.stopMicTest);
  const [hasTestedMic, setHasTestedMic] = useState(false);
  const [confirmedWorking, setConfirmedWorking] = useState(false);

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
      setConfirmedWorking(false);
      setHasTestedMic(false);
    },
    [selectDevice],
  );

  const handleStartTest = useCallback(async () => {
    await startMicTest();
    setHasTestedMic(true);
  }, [startMicTest]);

  const handleContinue = useCallback(async () => {
    if (isMeterRunning) {
      await stopMicTest();
    }
    onReady();
  }, [isMeterRunning, onReady, stopMicTest]);

  const currentDevice =
    devices.find((d) => d.uid === selectedDeviceUid) ||
    devices.find((d) => d.is_default);
  const hasSelectedDevice = Boolean(selectedDeviceUid || currentDevice?.uid);
  const canContinue = hasSelectedDevice && hasTestedMic && confirmedWorking;

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
          onStartTest={handleStartTest}
          onStopTest={stopMicTest}
          audioLevel={audioLevel}
          isRunning={isMeterRunning}
        />
      </div>

      <div className="mt-6 text-left space-y-3">
        <label className="flex items-start gap-2 text-sm text-gray-700 dark:text-gray-300">
          <input
            type="checkbox"
            checked={confirmedWorking}
            onChange={(e) => setConfirmedWorking(e.target.checked)}
            disabled={!hasTestedMic || !hasSelectedDevice}
            className="mt-0.5 h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500 disabled:opacity-50"
          />
          <span>
            I tested this microphone and it is working.
          </span>
        </label>
        {!hasTestedMic && (
          <p className="text-xs text-gray-500 dark:text-gray-400">
            Start the mic test first, then confirm before continuing.
          </p>
        )}
        <div className="flex justify-end">
          <button
            type="button"
            onClick={handleContinue}
            disabled={!canContinue}
            className="px-4 py-2 text-sm rounded bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Continue
          </button>
        </div>
      </div>
    </div>
  );
}
