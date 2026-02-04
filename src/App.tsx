import { useEffect } from 'react';
import { useAppStore, selectAppState, selectIsRecording } from './store';
import { useTauriEvents } from './hooks';

function App() {
  // Set up Tauri event listeners
  useTauriEvents();

  // Get store state and actions
  const appState = useAppStore(selectAppState);
  const isRecording = useAppStore(selectIsRecording);
  const errorDetail = useAppStore((state) => state.errorDetail);
  const isInitialized = useAppStore((state) => state.isInitialized);
  const isLoading = useAppStore((state) => state.isLoading);
  const devices = useAppStore((state) => state.devices);
  const history = useAppStore((state) => state.history);
  const modelStatus = useAppStore((state) => state.modelStatus);

  const initialize = useAppStore((state) => state.initialize);
  const refreshDevices = useAppStore((state) => state.refreshDevices);

  // Initialize store on mount
  useEffect(() => {
    initialize();
  }, [initialize]);

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

  return (
    <div className="min-h-screen flex flex-col items-center justify-center p-8">
      <h1 className="text-4xl font-bold mb-8">Voice Input Tool</h1>

      {/* Status Badge */}
      <div className="mb-6">
        <span
          className={`px-4 py-2 rounded-full text-sm font-medium ${
            isRecording
              ? 'bg-red-500/20 text-red-400 animate-pulse'
              : appState === 'transcribing'
              ? 'bg-yellow-500/20 text-yellow-400'
              : appState === 'error'
              ? 'bg-red-500/20 text-red-400'
              : 'bg-green-500/20 text-green-400'
          }`}
        >
          {appState === 'idle' && 'Ready'}
          {appState === 'loading_model' && 'Loading Model...'}
          {appState === 'recording' && 'Recording...'}
          {appState === 'transcribing' && 'Transcribing...'}
          {appState === 'error' && 'Error'}
        </span>
      </div>

      {/* Error Display */}
      {errorDetail && (
        <div className="mb-6 p-4 bg-red-500/10 border border-red-500/20 rounded-lg max-w-md">
          <p className="text-red-400 text-sm">{errorDetail}</p>
        </div>
      )}

      <div className="w-full max-w-md space-y-4">
        {/* Model Status */}
        <div className="bg-gray-800 rounded-lg p-6">
          <h2 className="text-xl font-semibold mb-4">Model Status</h2>
          <div className="space-y-2">
            <p className="text-sm text-gray-400">
              Status:{' '}
              <span className="text-white">
                {modelStatus?.status ?? 'Unknown'}
              </span>
            </p>
            {modelStatus?.model_id && (
              <p className="text-sm text-gray-400">
                Model:{' '}
                <span className="text-white font-mono text-xs">
                  {modelStatus.model_id}
                </span>
              </p>
            )}
          </div>
        </div>

        {/* Audio Devices */}
        <div className="bg-gray-800 rounded-lg p-6">
          <div className="flex justify-between items-center mb-4">
            <h2 className="text-xl font-semibold">Audio Devices</h2>
            <button
              onClick={() => refreshDevices()}
              className="text-sm text-blue-400 hover:text-blue-300"
            >
              Refresh
            </button>
          </div>
          <div className="space-y-2">
            {devices.length === 0 ? (
              <p className="text-gray-500 text-sm">No devices found</p>
            ) : (
              devices.map((device) => (
                <div
                  key={device.uid}
                  className="p-2 bg-gray-700 rounded text-sm flex justify-between"
                >
                  <span className="truncate">{device.name}</span>
                  {device.is_default && (
                    <span className="text-green-400 text-xs">Default</span>
                  )}
                </div>
              ))
            )}
          </div>
        </div>

        {/* Recent Transcripts */}
        <div className="bg-gray-800 rounded-lg p-6">
          <h2 className="text-xl font-semibold mb-4">Recent Transcripts</h2>
          <div className="space-y-2 max-h-48 overflow-y-auto">
            {history.length === 0 ? (
              <p className="text-gray-500 text-sm">No transcripts yet</p>
            ) : (
              history.slice(0, 5).map((entry) => (
                <div key={entry.id} className="p-2 bg-gray-700 rounded text-sm">
                  <p className="truncate">{entry.text}</p>
                  <p className="text-xs text-gray-500 mt-1">
                    {new Date(entry.timestamp).toLocaleTimeString()}
                  </p>
                </div>
              ))
            )}
          </div>
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
