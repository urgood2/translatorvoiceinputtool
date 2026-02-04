/**
 * Main settings panel combining all configuration sections.
 *
 * Features:
 * - Tabbed navigation for different settings areas
 * - Live apply with rollback on failure
 * - Integrates MicrophoneSelect, HotkeyConfig, InjectionSettings
 */

import { useState } from 'react';
import type { AppConfig, AudioDevice, EffectiveMode, ActivationMode } from '../../types';
import { MicrophoneSelect } from './MicrophoneSelect';
import { HotkeyConfig } from './HotkeyConfig';
import { InjectionSettings } from './InjectionSettings';

type SettingsTab = 'audio' | 'hotkeys' | 'injection';

interface SettingsPanelProps {
  config: AppConfig;
  devices: AudioDevice[];
  effectiveHotkeyMode?: EffectiveMode<ActivationMode>;
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
  effectiveHotkeyMode,
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
      </div>

      {/* Tab content */}
      <div className="p-4 bg-white dark:bg-gray-800 rounded-b-lg">
        {activeTab === 'audio' && (
          <MicrophoneSelect
            devices={devices}
            selectedUid={config.audio.device_uid}
            audioCuesEnabled={config.audio.audio_cues_enabled}
            onDeviceChange={(uid) => handleAudioChange('device_uid', uid)}
            onAudioCuesChange={(enabled) => handleAudioChange('audio_cues_enabled', enabled)}
            isLoading={isLoading}
          />
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
      </div>
    </div>
  );
}

export default SettingsPanel;
