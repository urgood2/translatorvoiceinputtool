import { useCallback, useEffect, useState } from 'react';
import { useAppStore, selectAppState } from './store';
import { useTauriEvents } from './hooks';
import {
  SelfCheck,
  Diagnostics,
  SettingsPanel,
  StatusIndicator,
  HistoryPanel,
  StatusDashboard,
  TabBar,
  TabPanel,
} from './components';
import type { DiagnosticsReport } from './types';

type AppTab = 'status' | 'history' | 'replacements' | 'settings';

function App() {
  // Set up Tauri event listeners
  useTauriEvents();

  // Get store state and actions
  const appState = useAppStore(selectAppState);
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

  const initialize = useAppStore((state) => state.initialize);
  const refreshDevices = useAppStore((state) => state.refreshDevices);
  const runSelfCheck = useAppStore((state) => state.runSelfCheck);
  const generateDiagnostics = useAppStore((state) => state.generateDiagnostics);
  const updateAudioConfig = useAppStore((state) => state.updateAudioConfig);
  const updateHotkeyConfig = useAppStore((state) => state.updateHotkeyConfig);
  const updateInjectionConfig = useAppStore((state) => state.updateInjectionConfig);
  const startMicTest = useAppStore((state) => state.startMicTest);
  const stopMicTest = useAppStore((state) => state.stopMicTest);
  const copyTranscript = useAppStore((state) => state.copyTranscript);

  const [isSelfCheckLoading, setIsSelfCheckLoading] = useState(false);
  const [isDiagnosticsLoading, setIsDiagnosticsLoading] = useState(false);
  const [diagnosticsReport, setDiagnosticsReport] = useState<DiagnosticsReport | null>(null);
  const [activeTab, setActiveTab] = useState<AppTab>('status');

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
    }
  }, [updateAudioConfig, updateHotkeyConfig, updateInjectionConfig]);

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

  const tabs = [
    { id: 'status', label: 'Status' },
    { id: 'history', label: 'History' },
    { id: 'replacements', label: 'Replacements', badge: config?.replacements.length ?? 0 },
    { id: 'settings', label: 'Settings' },
  ];

  return (
    <div className="min-h-screen flex flex-col items-center justify-center p-8">
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
            <StatusDashboard />
          </TabPanel>

          <TabPanel id="history" activeTab={activeTab}>
            <HistoryPanel entries={history.slice(0, 25)} onCopy={copyTranscript} />
          </TabPanel>

          <TabPanel id="replacements" activeTab={activeTab}>
            <div className="space-y-3">
              <h2 className="text-xl font-semibold">Replacements</h2>
              <p className="text-sm text-gray-300">
                Replacements tab integration is in progress.
              </p>
              <p className="text-sm text-gray-400">
                Configured rules: {config?.replacements.length ?? 0}
              </p>
            </div>
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
    </div>
  );
}

export default App;
