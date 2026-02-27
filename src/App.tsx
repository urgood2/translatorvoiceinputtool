import { useCallback, useEffect, useMemo, useState } from 'react';
import { useAppStore, selectAppState, selectReplacementBadgeCount } from './store';
import { useReducedMotion, useTauriEvents } from './hooks';
import {
  SelfCheck,
  Diagnostics,
  SettingsPanel,
  StatusIndicator,
  HistoryPanel,
  StatusDashboard,
  TabBar,
  TabPanel,
  ReplacementList,
  PresetsPanel,
} from './components';
import { OnboardingWizard } from './components/Onboarding';
import type { DiagnosticsReport, ReplacementRule } from './types';

type AppTab = 'status' | 'history' | 'replacements' | 'settings';

function App() {
  // Set up Tauri event listeners
  useTauriEvents();
  useReducedMotion();

  // Get store state and actions
  const appState = useAppStore(selectAppState);
  const replacementsBadgeCount = useAppStore(selectReplacementBadgeCount);
  const enabled = useAppStore((state) => state.enabled);
  const errorDetail = useAppStore((state) => state.errorDetail);
  const isInitialized = useAppStore((state) => state.isInitialized);
  const isLoading = useAppStore((state) => state.isLoading);
  const devices = useAppStore((state) => state.devices);
  const audioLevel = useAppStore((state) => state.audioLevel);
  const isMeterRunning = useAppStore((state) => state.isMeterRunning);
  const history = useAppStore((state) => state.history);
  const downloadProgress = useAppStore((state) => state.downloadProgress);
  const selfCheckResult = useAppStore((state) => state.selfCheckResult);
  const config = useAppStore((state) => state.config);
  const capabilities = useAppStore((state) => state.capabilities);
  const presets = useAppStore((state) => state.presets);

  const initialize = useAppStore((state) => state.initialize);
  const refreshDevices = useAppStore((state) => state.refreshDevices);
  const runSelfCheck = useAppStore((state) => state.runSelfCheck);
  const generateDiagnostics = useAppStore((state) => state.generateDiagnostics);
  const updateAudioConfig = useAppStore((state) => state.updateAudioConfig);
  const updateHotkeyConfig = useAppStore((state) => state.updateHotkeyConfig);
  const updateInjectionConfig = useAppStore((state) => state.updateInjectionConfig);
  const updateUiConfig = useAppStore((state) => state.updateUiConfig);
  const setReplacementRules = useAppStore((state) => state.setReplacementRules);
  const loadPreset = useAppStore((state) => state.loadPreset);
  const startMicTest = useAppStore((state) => state.startMicTest);
  const stopMicTest = useAppStore((state) => state.stopMicTest);
  const copyTranscript = useAppStore((state) => state.copyTranscript);
  const clearHistory = useAppStore((state) => state.clearHistory);

  const [isSelfCheckLoading, setIsSelfCheckLoading] = useState(false);
  const [isDiagnosticsLoading, setIsDiagnosticsLoading] = useState(false);
  const [diagnosticsReport, setDiagnosticsReport] = useState<DiagnosticsReport | null>(null);
  const [activeTab, setActiveTab] = useState<AppTab>('status');
  const [presetRulesById, setPresetRulesById] = useState<Map<string, ReplacementRule[]>>(new Map());
  const [onboardingDismissed, setOnboardingDismissed] = useState(false);

  // Initialize store on mount
  useEffect(() => {
    initialize();
  }, [initialize]);

  const refreshSelfCheck = useCallback(async () => {
    setIsSelfCheckLoading(true);
    try {
      await runSelfCheck();
    } finally {
      setIsSelfCheckLoading(false);
    }
  }, [runSelfCheck]);

  const refreshDiagnostics = useCallback(async () => {
    setIsDiagnosticsLoading(true);
    try {
      const report = await generateDiagnostics();
      setDiagnosticsReport(report);
    } finally {
      setIsDiagnosticsLoading(false);
    }
  }, [generateDiagnostics]);

  const handleSettingsChange = useCallback(async (path: string[], value: any) => {
    const [section, key] = path;
    if (!section || !key) return;

    if (section === 'audio') {
      await updateAudioConfig({ [key]: value });
      return;
    }
    if (section === 'hotkeys') {
      await updateHotkeyConfig({ [key]: value });
      return;
    }
    if (section === 'injection') {
      await updateInjectionConfig({ [key]: value });
      return;
    }
    if (section === 'ui') {
      await updateUiConfig({ [key]: value });
    }
  }, [updateAudioConfig, updateHotkeyConfig, updateInjectionConfig, updateUiConfig]);

  const handleTabChange = useCallback((tabId: string) => {
    if (
      tabId === 'status'
      || tabId === 'history'
      || tabId === 'replacements'
      || tabId === 'settings'
    ) {
      setActiveTab(tabId);
    }
  }, []);

  const handleReplacementRulesChange = useCallback((rules: ReplacementRule[]) => {
    void setReplacementRules(rules).catch((error) => {
      console.error('Failed to update replacement rules from tab', error);
    });
  }, [setReplacementRules]);

  const enabledPresetIds = useMemo(() => {
    if (!config) {
      return [];
    }

    const enabled = new Set<string>(config.presets.enabled_presets);
    for (const rule of config.replacements) {
      if (typeof rule.origin === 'string' && rule.origin.startsWith('preset:')) {
        enabled.add(rule.origin.slice('preset:'.length));
      }
    }
    return [...enabled];
  }, [config]);

  const handleTogglePreset = useCallback((presetId: string, enabled: boolean) => {
    // Read fresh config from store to avoid stale closure overwriting concurrent edits
    const freshConfig = useAppStore.getState().config;
    if (!freshConfig) {
      return;
    }

    const withoutPresetRules = freshConfig.replacements.filter((rule) => rule.origin !== `preset:${presetId}`);

    if (!enabled) {
      void setReplacementRules(withoutPresetRules).catch((error) => {
        console.error(`Failed to disable preset '${presetId}'`, error);
      });
      return;
    }

    void (async () => {
      try {
        const cachedRules = presetRulesById.get(presetId);
        const loadedRules = cachedRules ?? await loadPreset(presetId);
        const normalizedRules = (Array.isArray(loadedRules) ? loadedRules : []).map((rule, index) => ({
          ...rule,
          id: `${presetId}-${rule.id || index}`,
          origin: `preset:${presetId}` as const,
          enabled: true,
        }));

        if (!cachedRules) {
          setPresetRulesById((current) => {
            const next = new Map(current);
            next.set(presetId, normalizedRules);
            return next;
          });
        }

        // Re-read fresh state before merging to avoid overwriting concurrent edits
        const latestConfig = useAppStore.getState().config;
        const latestWithout = (latestConfig?.replacements ?? []).filter((rule) => rule.origin !== `preset:${presetId}`);
        await setReplacementRules([...latestWithout, ...normalizedRules]);
      } catch (error) {
        console.error(`Failed to enable preset '${presetId}'`, error);
      }
    })();
  }, [loadPreset, presetRulesById, setReplacementRules]);

  useEffect(() => {
    if (!isInitialized) {
      return;
    }
    void refreshSelfCheck();
    void refreshDiagnostics();
  }, [isInitialized, refreshSelfCheck, refreshDiagnostics]);

  // Loading state
  if (isLoading && !isInitialized) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500 mx-auto mb-4" />
          <p className="text-gray-400">Initializing...</p>
        </div>
      </div>
    );
  }

  // Onboarding gate: show wizard for new users
  // Treat missing onboarding_completed as "already completed" (migration safety)
  if (config && config.ui.onboarding_completed === false && !onboardingDismissed) {
    return <OnboardingWizard onComplete={() => setOnboardingDismissed(true)} />;
  }

  const tabs = [
    { id: 'status', label: 'Status' },
    { id: 'history', label: 'History' },
    { id: 'replacements', label: 'Replacements', badge: replacementsBadgeCount },
    { id: 'settings', label: 'Settings' },
  ];

  return (
    <div className="min-h-screen">
      <a
        href="#main-content"
        className="sr-only focus:not-sr-only focus:absolute focus:left-4 focus:top-4 focus:z-50 focus:rounded-md focus:bg-white focus:px-3 focus:py-2 focus:text-sm focus:font-semibold focus:text-blue-700"
      >
        Skip to main content
      </a>

      <main id="main-content" role="main" className="flex flex-col items-center justify-center p-8">
        <h1 className="text-4xl font-bold mb-8">Voice Input Tool</h1>

        <div className="w-full max-w-4xl mb-6">
          <StatusIndicator
            state={appState}
            enabled={enabled}
            detail={errorDetail}
            progress={downloadProgress ?? undefined}
          />
        </div>

        <div className="w-full max-w-4xl space-y-4">
          <TabBar tabs={tabs} activeTab={activeTab} onTabChange={handleTabChange} />

          <div className="h-[65vh] min-h-[480px]">
            <TabPanel id="status" activeTab={activeTab}>
              <StatusDashboard onNavigateSettings={() => setActiveTab('settings')} />
            </TabPanel>

            <TabPanel id="history" activeTab={activeTab}>
              <HistoryPanel
                entries={history.slice(0, 25)}
                onCopy={copyTranscript}
                onClearAll={clearHistory}
              />
            </TabPanel>

            <TabPanel id="replacements" activeTab={activeTab}>
              {config ? (
                <div className="flex h-full min-h-0 flex-col gap-4 overflow-y-auto pr-1">
                  <div className="rounded-lg border border-gray-700 bg-gray-800/70 p-4">
                    <PresetsPanel
                      presets={presets}
                      enabledPresets={enabledPresetIds}
                      onTogglePreset={handleTogglePreset}
                      presetRules={presetRulesById}
                    />
                  </div>
                  <div className="rounded-lg border border-gray-700 bg-gray-800/70 p-4">
                    <ReplacementList
                      rules={config.replacements}
                      onChange={handleReplacementRulesChange}
                      isLoading={isLoading}
                    />
                  </div>
                </div>
              ) : (
                <p className="text-sm text-gray-300">Loading replacement rules...</p>
              )}
            </TabPanel>

            <TabPanel id="settings" activeTab={activeTab}>
              <div className="space-y-4">
                {config && (
                  <div className="rounded-lg border border-gray-700 bg-gray-800/70 p-4">
                    <SettingsPanel
                      config={config}
                      devices={devices}
                      audioLevel={audioLevel}
                      isMeterRunning={isMeterRunning}
                      effectiveHotkeyMode={capabilities?.hotkey_mode}
                      onStartMicTest={startMicTest}
                      onStopMicTest={stopMicTest}
                      onRefreshDevices={refreshDevices}
                      onConfigChange={handleSettingsChange}
                      isLoading={isLoading}
                    />
                  </div>
                )}

                <div className="rounded-lg border border-gray-700 bg-gray-800/70 p-4">
                  <SelfCheck
                    result={selfCheckResult}
                    onRefresh={refreshSelfCheck}
                    isLoading={isSelfCheckLoading}
                  />
                </div>

                <div className="rounded-lg border border-gray-700 bg-gray-800/70 p-4">
                  <Diagnostics
                    report={diagnosticsReport}
                    onRefresh={refreshDiagnostics}
                    isLoading={isDiagnosticsLoading}
                  />
                </div>
              </div>
            </TabPanel>
          </div>

          <p className="text-center text-gray-500 text-sm">
            Press <code className="text-blue-400">Ctrl+Shift+Space</code> to
            record
          </p>
        </div>
      </main>
    </div>
  );
}

export default App;
