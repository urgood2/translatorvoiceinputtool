/**
 * Main settings panel combining all configuration sections.
 *
 * Features:
 * - Tabbed navigation for different settings areas
 * - Live apply with rollback on failure
 * - Integrates MicrophoneSelect, HotkeyConfig, InjectionSettings
 */

import { useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';
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
  id,
  controls,
  tabIndex,
  active,
  buttonRef,
  onKeyDown,
  onClick,
  children,
}: {
  id: string;
  controls: string;
  tabIndex: number;
  active: boolean;
  buttonRef: (element: HTMLButtonElement | null) => void;
  onKeyDown: (event: KeyboardEvent<HTMLButtonElement>) => void;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      ref={buttonRef}
      id={id}
      type="button"
      role="tab"
      aria-selected={active}
      aria-controls={controls}
      tabIndex={tabIndex}
      onKeyDown={onKeyDown}
      onClick={onClick}
      className={`px-4 py-2 text-sm font-medium rounded-t-lg transition-colors
                 ${active
                   ? 'bg-white dark:bg-gray-800 text-blue-600 dark:text-blue-400 border-b-2 border-blue-500'
                   : 'text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300'}
                 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500`}
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
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const settingsTabs: Array<{ id: SettingsTab; label: string }> = [
    { id: 'audio', label: 'Audio' },
    { id: 'hotkeys', label: 'Hotkeys' },
    { id: 'injection', label: 'Injection' },
    { id: 'appearance', label: 'Appearance' },
  ];
  const themeOptions = ['system', 'light', 'dark'] as const;

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

  const focusTabAt = (index: number) => {
    const bounded = ((index % settingsTabs.length) + settingsTabs.length) % settingsTabs.length;
    tabRefs.current[bounded]?.focus();
  };

  const handleTabKeyDown = (
    event: KeyboardEvent<HTMLButtonElement>,
    index: number,
    tabId: SettingsTab
  ) => {
    switch (event.key) {
      case 'ArrowRight':
      case 'ArrowDown': {
        event.preventDefault();
        const next = (index + 1) % settingsTabs.length;
        setActiveTab(settingsTabs[next].id);
        focusTabAt(next);
        break;
      }
      case 'ArrowLeft':
      case 'ArrowUp': {
        event.preventDefault();
        const prev = (index - 1 + settingsTabs.length) % settingsTabs.length;
        setActiveTab(settingsTabs[prev].id);
        focusTabAt(prev);
        break;
      }
      case 'Home':
        event.preventDefault();
        setActiveTab(settingsTabs[0].id);
        focusTabAt(0);
        break;
      case 'End':
        event.preventDefault();
        setActiveTab(settingsTabs[settingsTabs.length - 1].id);
        focusTabAt(settingsTabs.length - 1);
        break;
      case 'Enter':
      case ' ':
        event.preventDefault();
        setActiveTab(tabId);
        break;
      default:
        break;
    }
  };

  const focusThemeOption = (index: number) => {
    const bounded = ((index % themeOptions.length) + themeOptions.length) % themeOptions.length;
    const target = document.querySelector<HTMLButtonElement>(
      `[data-theme-option="${themeOptions[bounded]}"]`
    );
    target?.focus();
  };

  const handleThemeKeyDown = (
    event: KeyboardEvent<HTMLButtonElement>,
    currentIndex: number
  ) => {
    let nextIndex = currentIndex;
    switch (event.key) {
      case 'ArrowRight':
      case 'ArrowDown':
        nextIndex = (currentIndex + 1) % themeOptions.length;
        break;
      case 'ArrowLeft':
      case 'ArrowUp':
        nextIndex = (currentIndex - 1 + themeOptions.length) % themeOptions.length;
        break;
      case 'Home':
        nextIndex = 0;
        break;
      case 'End':
        nextIndex = themeOptions.length - 1;
        break;
      default:
        return;
    }

    event.preventDefault();
    const nextOption = themeOptions[nextIndex];
    void onConfigChange(['ui', 'theme'], nextOption);
    focusThemeOption(nextIndex);
  };

  return (
    <div className="bg-gray-50 dark:bg-gray-900 rounded-lg">
      {/* Tab navigation */}
      <div
        className="flex border-b border-gray-200 dark:border-gray-700 px-4 pt-2"
        role="tablist"
        aria-label="Settings sections"
      >
        {settingsTabs.map((tab, index) => (
          <TabButton
            key={tab.id}
            id={`settings-tab-${tab.id}`}
            controls={`settings-panel-${tab.id}`}
            tabIndex={activeTab === tab.id ? 0 : -1}
            active={activeTab === tab.id}
            buttonRef={(element) => {
              tabRefs.current[index] = element;
            }}
            onClick={() => setActiveTab(tab.id)}
            onKeyDown={(event) => handleTabKeyDown(event, index, tab.id)}
          >
            {tab.label}
          </TabButton>
        ))}
      </div>

      {/* Tab content */}
      <div
        id={`settings-panel-${activeTab}`}
        role="tabpanel"
        aria-labelledby={`settings-tab-${activeTab}`}
        className="p-4 bg-white dark:bg-gray-800 rounded-b-lg"
      >
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
              {themeOptions.map((option, index) => {
                const active = config.ui.theme === option;
                return (
                  <button
                    key={option}
                    type="button"
                    data-theme-option={option}
                    role="radio"
                    aria-checked={active}
                    tabIndex={active ? 0 : -1}
                    onClick={() => {
                      void onConfigChange(['ui', 'theme'], option);
                    }}
                    onKeyDown={(event) => handleThemeKeyDown(event, index)}
                    className={`px-4 py-2 rounded text-sm font-medium capitalize transition-colors
                      ${active
                        ? 'bg-blue-600 text-white'
                        : 'bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-300 dark:hover:bg-gray-600'}
                      focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500`}
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
