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
  onPurgeHistory?: () => Promise<void>;
  historyCount?: number;
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
  onPurgeHistory,
  historyCount = 0,
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
  const [showPurgeConfirm, setShowPurgeConfirm] = useState(false);
  const [isPurging, setIsPurging] = useState(false);
  const [purgeError, setPurgeError] = useState<string | null>(null);
  const [purgeSuccess, setPurgeSuccess] = useState(false);

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

            {/* VAD Auto-Stop settings */}
            <div className="space-y-4">
              <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-400">
                Voice Activity Detection
              </h3>

              {/* VAD enable toggle */}
              <div className="flex items-center justify-between">
                <div>
                  <label id="vad-enabled-label" htmlFor="vad-enabled" className="font-medium text-gray-900 dark:text-gray-100">
                    Auto-Stop on Silence
                  </label>
                  <p id="vad-enabled-description" className="text-sm text-gray-500 dark:text-gray-400">
                    Automatically stop recording after a pause in speech
                  </p>
                </div>
                <button
                  type="button"
                  id="vad-enabled"
                  role="switch"
                  aria-checked={config.audio.vad_enabled}
                  aria-labelledby="vad-enabled-label"
                  aria-describedby="vad-enabled-description"
                  onClick={() => handleAudioChange('vad_enabled', !config.audio.vad_enabled)}
                  disabled={isLoading}
                  className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors
                             ${config.audio.vad_enabled ? 'bg-blue-500' : 'bg-gray-300 dark:bg-gray-600'}
                             disabled:opacity-50 disabled:cursor-not-allowed`}
                >
                  <span
                    className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform
                               ${config.audio.vad_enabled ? 'translate-x-6' : 'translate-x-1'}`}
                  />
                </button>
              </div>

              {/* VAD parameter sliders (only shown when enabled) */}
              {config.audio.vad_enabled && (
                <div className="space-y-4 pl-1">
                  {/* Silence duration slider */}
                  <div>
                    <label htmlFor="vad-silence-ms" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                      Silence before stop: {(config.audio.vad_silence_ms / 1000).toFixed(1)}s
                    </label>
                    <input
                      id="vad-silence-ms"
                      type="range"
                      min={400}
                      max={5000}
                      step={100}
                      value={config.audio.vad_silence_ms}
                      onChange={(e) => handleAudioChange('vad_silence_ms', Number(e.target.value))}
                      disabled={isLoading}
                      className="w-full accent-blue-500"
                    />
                    <div className="flex justify-between text-xs text-gray-400">
                      <span>0.4s</span>
                      <span>5.0s</span>
                    </div>
                  </div>

                  {/* Min speech duration slider */}
                  <div>
                    <label htmlFor="vad-min-speech-ms" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                      Min speech before stop: {(config.audio.vad_min_speech_ms / 1000).toFixed(1)}s
                    </label>
                    <input
                      id="vad-min-speech-ms"
                      type="range"
                      min={100}
                      max={2000}
                      step={50}
                      value={config.audio.vad_min_speech_ms}
                      onChange={(e) => handleAudioChange('vad_min_speech_ms', Number(e.target.value))}
                      disabled={isLoading}
                      className="w-full accent-blue-500"
                    />
                    <div className="flex justify-between text-xs text-gray-400">
                      <span>0.1s</span>
                      <span>2.0s</span>
                    </div>
                  </div>
                </div>
              )}
            </div>
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

            {/* Data Management */}
            {onPurgeHistory ? (
              <div className="mt-6 space-y-3 border-t border-gray-200 pt-4 dark:border-gray-700">
                <h3 className="text-sm font-semibold uppercase tracking-wide text-gray-400">
                  Data
                </h3>
                <div className="flex items-center justify-between">
                  <div>
                    <p className="font-medium text-gray-900 dark:text-gray-100">Transcript History</p>
                    <p className="text-sm text-gray-500 dark:text-gray-400">
                      {historyCount === 0
                        ? 'No entries stored.'
                        : `${historyCount} ${historyCount === 1 ? 'entry' : 'entries'} stored.`}
                      {config.history.persistence_mode === 'disk' ? ' Saved to disk.' : ' In memory only.'}
                    </p>
                  </div>
                  <button
                    type="button"
                    data-testid="settings-purge-history-button"
                    disabled={historyCount === 0 || isPurging}
                    onClick={() => {
                      setPurgeError(null);
                      setPurgeSuccess(false);
                      setShowPurgeConfirm(true);
                    }}
                    className="rounded-md border border-red-300 px-3 py-1.5 text-xs font-semibold text-red-600 hover:bg-red-50 disabled:cursor-not-allowed disabled:opacity-60 dark:border-red-700 dark:text-red-400 dark:hover:bg-red-900/20"
                  >
                    Purge History
                  </button>
                </div>

                {purgeSuccess ? (
                  <p data-testid="settings-purge-success" className="text-xs text-emerald-600 dark:text-emerald-400">
                    History purged successfully.
                  </p>
                ) : null}

                {purgeError ? (
                  <p role="alert" data-testid="settings-purge-error" className="text-xs text-red-600 dark:text-red-400">
                    {purgeError}
                  </p>
                ) : null}

                {showPurgeConfirm ? (
                  <div
                    data-testid="settings-purge-confirm-dialog"
                    className="rounded-md border border-red-300 bg-red-50 p-3 dark:border-red-700 dark:bg-red-900/20"
                  >
                    <p className="text-sm font-medium text-red-700 dark:text-red-300">
                      Permanently delete all transcript history?
                    </p>
                    <p className="mt-1 text-xs text-red-600 dark:text-red-400">
                      This removes both in-memory and disk-persisted history. This action cannot be undone.
                    </p>
                    <div className="mt-2 flex gap-2">
                      <button
                        type="button"
                        data-testid="settings-purge-confirm"
                        disabled={isPurging}
                        onClick={async () => {
                          setPurgeError(null);
                          setIsPurging(true);
                          try {
                            await onPurgeHistory();
                            setPurgeSuccess(true);
                            setShowPurgeConfirm(false);
                          } catch (error) {
                            setPurgeError(error instanceof Error ? error.message : 'Purge failed');
                          } finally {
                            setIsPurging(false);
                          }
                        }}
                        className="rounded-md bg-red-600 px-3 py-1 text-xs font-semibold text-white hover:bg-red-500 disabled:cursor-not-allowed disabled:opacity-60"
                      >
                        {isPurging ? 'Purging…' : 'Confirm Purge'}
                      </button>
                      <button
                        type="button"
                        data-testid="settings-purge-cancel"
                        disabled={isPurging}
                        onClick={() => {
                          setShowPurgeConfirm(false);
                          setPurgeError(null);
                        }}
                        className="rounded-md border border-gray-300 px-3 py-1 text-xs text-gray-700 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                ) : null}
              </div>
            ) : null}
          </div>
        )}
      </div>
    </div>
  );
}

export default SettingsPanel;
