/**
 * Main settings panel combining all configuration sections.
 *
 * Features:
 * - Tabbed navigation for different settings areas
 * - Live apply with rollback on failure
 * - Integrates MicrophoneSelect, HotkeyConfig, InjectionSettings
 */

import { useState } from 'react';
import type {
  AppConfig,
  AudioDevice,
  AudioLevelEvent,
  EffectiveMode,
  ActivationMode,
} from '../../types';
import { MicrophoneSelect } from './MicrophoneSelect';
import { HotkeyConfig } from './HotkeyConfig';
import { InjectionSettings } from './InjectionSettings';
import { MicrophoneTest } from './MicrophoneTest';

type SettingsTab = 'audio' | 'hotkeys' | 'injection' | 'appearance';

interface SettingsPanelProps {
  config: AppConfig;
  devices: AudioDevice[];
  audioLevel?: AudioLevelEvent | null;
  isMeterRunning?: boolean;
  effectiveHotkeyMode?: EffectiveMode<ActivationMode>;
  onStartMicTest?: () => Promise<void>;
  onStopMicTest?: () => Promise<void>;
  onRefreshDevices?: () => Promise<void> | void;
  onConfigChange: (path: string[], value: any) => Promise<void>;
  isLoading?: boolean;
}

/** Tab button component. */
function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={`px-4 py-2 text-sm font-medium rounded-t-lg transition-colors
                 ${active
                   ? 'bg-white dark:bg-gray-800 text-blue-600 dark:text-blue-400 border-b-2 border-blue-500'
                   : 'text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300'}`}
    >
      {children}
    </button>
  );
}

export function SettingsPanel({
  config,
  devices,
  audioLevel,
  isMeterRunning = false,
  effectiveHotkeyMode,
  onStartMicTest,
  onStopMicTest,
  onRefreshDevices,
  onConfigChange,
  isLoading,
}: SettingsPanelProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>('audio');

  // Helper to create path-based config updaters
  const handleAudioChange = async (key: string, value: any) => {
    await onConfigChange(['audio', key], value);
  };

  const handleHotkeyChange = async (key: string, value: any) => {
    await onConfigChange(['hotkeys', key], value);
  };

  const handleInjectionChange = async (key: string, value: any) => {
    await onConfigChange(['injection', key], value);
  };

  const handleStartMicTest = async () => {
    if (!onStartMicTest) return;
    await onStartMicTest();
  };

  const handleStopMicTest = async () => {
    if (!onStopMicTest) return;
    await onStopMicTest();
  };

  return (
    <div className="bg-gray-50 dark:bg-gray-900 rounded-lg">
      {/* Tab navigation */}
      <div className="flex border-b border-gray-200 dark:border-gray-700 px-4 pt-2">
        <TabButton
          active={activeTab === 'audio'}
          onClick={() => setActiveTab('audio')}
        >
          Audio
        </TabButton>
        <TabButton
          active={activeTab === 'hotkeys'}
          onClick={() => setActiveTab('hotkeys')}
        >
          Hotkeys
        </TabButton>
        <TabButton
          active={activeTab === 'injection'}
          onClick={() => setActiveTab('injection')}
        >
          Injection
        </TabButton>
        <TabButton
          active={activeTab === 'appearance'}
          onClick={() => setActiveTab('appearance')}
        >
          Appearance
        </TabButton>
      </div>

      {/* Tab content */}
      <div className="p-4 bg-white dark:bg-gray-800 rounded-b-lg">
        {activeTab === 'audio' && (
          <div className="space-y-6">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-400">
                Audio Devices
              </h3>
              {onRefreshDevices ? (
                <button
                  type="button"
                  onClick={() => {
                    void onRefreshDevices();
                  }}
                  className="text-sm text-blue-400 hover:text-blue-300"
                >
                  Refresh
                </button>
              ) : null}
            </div>
            <MicrophoneSelect
              devices={devices}
              selectedUid={config.audio.device_uid}
              audioCuesEnabled={config.audio.audio_cues_enabled}
              onDeviceChange={(uid) => handleAudioChange('device_uid', uid)}
              onAudioCuesChange={(enabled) => handleAudioChange('audio_cues_enabled', enabled)}
              isLoading={isLoading}
            />
            <MicrophoneTest
              deviceUid={config.audio.device_uid}
              onStartTest={handleStartMicTest}
              onStopTest={handleStopMicTest}
              audioLevel={audioLevel ?? null}
              isRunning={isMeterRunning}
            />
          </div>
        )}

        {activeTab === 'hotkeys' && (
          <HotkeyConfig
            primaryHotkey={config.hotkeys.primary}
            copyLastHotkey={config.hotkeys.copy_last}
            mode={config.hotkeys.mode}
            effectiveMode={effectiveHotkeyMode}
            onPrimaryChange={(value) => handleHotkeyChange('primary', value)}
            onCopyLastChange={(value) => handleHotkeyChange('copy_last', value)}
            onModeChange={(value) => handleHotkeyChange('mode', value)}
            isLoading={isLoading}
          />
        )}

        {activeTab === 'injection' && (
          <InjectionSettings
            config={config.injection}
            onChange={handleInjectionChange}
            isLoading={isLoading}
          />
        )}

        {activeTab === 'appearance' && (
          <div className="space-y-4">
            <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-400">
              Theme
            </h3>
            <div className="flex gap-2" role="radiogroup" aria-label="Theme">
              {(['system', 'light', 'dark'] as const).map((option) => {
                const active = config.ui.theme === option;
                return (
                  <button
                    key={option}
                    role="radio"
                    aria-checked={active}
                    onClick={() => onConfigChange(['ui', 'theme'], option)}
                    className={`px-4 py-2 rounded text-sm font-medium capitalize transition-colors
                      ${active
                        ? 'bg-blue-600 text-white'
                        : 'bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-300 dark:hover:bg-gray-600'}`}
                  >
                    {option}
                  </button>
                );
              })}
            </div>
            <p className="text-xs text-gray-500 dark:text-gray-400">
              Choose &ldquo;System&rdquo; to follow your OS preference.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

export default SettingsPanel;
