import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

function App() {
  const [echoInput, setEchoInput] = useState("");
  const [echoResult, setEchoResult] = useState("");
  const [isLoading, setIsLoading] = useState(false);

  async function handleEcho() {
    setIsLoading(true);
    try {
      const result = await invoke<string>("echo", { message: echoInput });
      setEchoResult(result);
    } catch (error) {
      setEchoResult(`Error: ${error}`);
    } finally {
      setIsLoading(false);
    }
  }

  return (
    <div className="min-h-screen flex flex-col items-center justify-center p-8">
      <h1 className="text-4xl font-bold mb-8">Voice Input Tool</h1>

      <div className="w-full max-w-md space-y-4">
        <div className="bg-gray-800 rounded-lg p-6">
          <h2 className="text-xl font-semibold mb-4">Test Rust Command</h2>

          <div className="space-y-4">
            <input
              type="text"
              value={echoInput}
              onChange={(e) => setEchoInput(e.target.value)}
              placeholder="Enter a message to echo"
              className="w-full px-4 py-2 bg-gray-700 rounded border border-gray-600 focus:border-blue-500 focus:outline-none"
            />

            <button
              onClick={handleEcho}
              disabled={isLoading}
              className="w-full px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-600 rounded font-medium transition-colors"
            >
              {isLoading ? "Calling..." : "Call Rust Echo Command"}
            </button>

            {echoResult && (
              <div className="p-4 bg-gray-700 rounded">
                <p className="text-sm text-gray-400">Result:</p>
                <p className="font-mono">{echoResult}</p>
              </div>
            )}
          </div>
        </div>

        <p className="text-center text-gray-500 text-sm">
          Edit <code className="text-blue-400">src/App.tsx</code> and save to
          test hot reload
        </p>
      </div>
    </div>
  );
}

export default App;
