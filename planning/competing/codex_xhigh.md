**Executive Summary (TL;DR)**  
OpenVoicy MVP will ship a Tauri 2.x desktop shell with a React/Tailwind UI and a Python sidecar that handles audio capture, Parakeet V3 inference, text post-processing, and replacement rules. The Rust core manages global hotkeys, system tray state, IPC to the sidecar, and text injection via `enigo`. The plan below details concrete files, modules, and acceptance criteria for each task, with explicit dependencies, risk mitigation, and parallelization.

**Architecture Overview**  
Core components and file layout (proposed):

1. Rust Tauri Core (IPC, hotkeys, tray, injection)  
`src-tauri/src/main.rs`  
`src-tauri/src/ipc/mod.rs`  
`src-tauri/src/ipc/types.rs`  
`src-tauri/src/hotkeys.rs`  
`src-tauri/src/tray.rs`  
`src-tauri/src/inject.rs`  
`src-tauri/src/state.rs`  

2. Web UI (settings, status, replacements, history placeholder)  
`src/ui/App.tsx`  
`src/ui/components/StatusIndicator.tsx`  
`src/ui/components/SettingsPanel.tsx`  
`src/ui/components/HotkeyPicker.tsx`  
`src/ui/components/MicSelector.tsx`  
`src/ui/components/ReplacementsEditor.tsx`  
`src/ui/state/store.ts`  
`src/ui/state/types.ts`  

3. Python Sidecar (audio capture, ASR, replacements)  
`sidecar/main.py`  
`sidecar/ipc/server.py`  
`sidecar/ipc/messages.py`  
`sidecar/audio/capture.py`  
`sidecar/asr/parakeet.py`  
`sidecar/asr/device.py`  
`sidecar/text/postprocess.py`  
`sidecar/text/replacements.py`  
`sidecar/config/schema.py`  

4. Shared Config and Defaults  
`config/defaults.json`  
`config/replacements.json`  
`src-tauri/tauri.conf.json`  

**Phase Breakdown With Numbered Tasks**  

**Phase 1: Core MVP (v0.1.0)**  

1. IPC Protocol Definition (Rust + Python)  
Complexity: M  
Dependencies: None  
Files: `src-tauri/src/ipc/types.rs`, `sidecar/ipc/messages.py`, `sidecar/ipc/server.py`  
Task: Define JSON-RPC message types and payloads for `start_recording`, `stop_recording`, `transcribe`, `status`, `list_devices`, `set_device`, `set_replacements`, and error responses with `code`, `message`, `details`.  
Acceptance criteria:  
1. Rust and Python use identical message schemas and enums for request/response.  
2. A round-trip smoke test of `status` returns sidecar readiness info and model load state.  
3. Error payloads include a stable error code and do not crash either process.  
Edge cases and failure modes:  
1. Unknown method returns `METHOD_NOT_FOUND` error without sidecar exit.  
2. Invalid payload returns `INVALID_PARAMS` with details.  

2. Python Sidecar Process Lifecycle (Rust)  
Complexity: M  
Dependencies: Task 1  
Files: `src-tauri/src/ipc/mod.rs`, `src-tauri/src/state.rs`, `src-tauri/src/main.rs`  
Task: Spawn and supervise the Python sidecar; restart on failure; expose ready state to UI.  
Acceptance criteria:  
1. Sidecar is spawned on app start and restarts on non-zero exit.  
2. Rust exposes sidecar status to UI via Tauri command `get_status`.  
3. Logs include sidecar stderr lines with a prefix.  
Edge cases and failure modes:  
1. Sidecar binary not found yields user-visible error and disables hotkey.  
2. Rapid crash loop triggers backoff (e.g., exponential with cap).  

3. Audio Device Enumeration (Python)  
Complexity: S  
Dependencies: Task 1  
Files: `sidecar/audio/capture.py`, `sidecar/ipc/server.py`  
Task: List input devices and current default device.  
Acceptance criteria:  
1. `list_devices` returns id, name, sample_rate, channels, and default flag.  
2. `set_device` persists selection to `config/defaults.json`.  
Edge cases and failure modes:  
1. No input devices returns empty list and an error reason.  
2. Selected device missing on next start falls back to OS default.  

4. Audio Capture and Buffering (Python)  
Complexity: M  
Dependencies: Task 3  
Files: `sidecar/audio/capture.py`, `sidecar/config/schema.py`  
Task: Implement push-to-talk recording into a ring buffer with 16 kHz PCM.  
Acceptance criteria:  
1. Recording starts within 150 ms after start command.  
2. Buffer length is bounded and prevents memory growth.  
3. Recording stop returns a mono PCM array compatible with ASR.  
Edge cases and failure modes:  
1. Device stream error triggers reset and returns a recoverable error.  
2. Sample rate mismatch is resampled to 16 kHz.  

5. Parakeet V3 Model Loader and Inference (Python)  
Complexity: L  
Dependencies: Task 4  
Files: `sidecar/asr/parakeet.py`, `sidecar/asr/device.py`  
Task: Load Parakeet V3 with GPU acceleration if available; CPU fallback.  
Acceptance criteria:  
1. Model loads once and remains resident between transcriptions.  
2. Inference latency for 10s audio under target (documented baseline).  
3. Inference returns text with confidence metadata.  
Edge cases and failure modes:  
1. CUDA/MLX unavailable triggers CPU fallback.  
2. Model load failure surfaces an error and disables transcription.  

6. Text Post-Processing (Python)  
Complexity: S  
Dependencies: Task 5  
Files: `sidecar/text/postprocess.py`  
Task: Punctuation and capitalization pass and cleanup of filler artifacts.  
Acceptance criteria:  
1. Text starts with capital letter and ends with punctuation if appropriate.  
2. Multiple spaces are collapsed.  
Edge cases and failure modes:  
1. Empty transcript returns empty string without injection.  
2. Non-ASCII is preserved.  

7. Text Replacement Engine (Python)  
Complexity: M  
Dependencies: Task 6  
Files: `sidecar/text/replacements.py`, `config/replacements.json`  
Task: Implement snippet expansion, smart tokens, and correction rules.  
Acceptance criteria:  
1. Simple mappings applied case-insensitively with preserve-case option.  
2. Tokens like `@@date`, `@@time`, `@@email` are expanded.  
3. Replacement rules are loaded from JSON and hot-reloaded on update.  
Edge cases and failure modes:  
1. Recursive replacements are prevented with a max depth.  
2. Invalid JSON config returns error and uses last known good.  

8. Text Injection (Rust)  
Complexity: M  
Dependencies: Task 6  
Files: `src-tauri/src/inject.rs`  
Task: Use `enigo` to inject text into focused field with configurable delay.  
Acceptance criteria:  
1. Unicode text injects correctly across Windows/macOS/Linux.  
2. Configurable delay is honored.  
Edge cases and failure modes:  
1. Rapid injections are serialized to avoid interleaving.  
2. Injection failure returns error and shows UI notification.  

9. Global Hotkeys (Rust)  
Complexity: M  
Dependencies: Task 2  
Files: `src-tauri/src/hotkeys.rs`, `src-tauri/src/state.rs`  
Task: Register global hotkey for push-to-talk; handle hold-to-record.  
Acceptance criteria:  
1. Press-and-hold starts recording; release stops and triggers transcription.  
2. Hotkey changes are persisted and applied without restart.  
Edge cases and failure modes:  
1. Conflict detection warns and refuses to register conflicting hotkeys.  
2. Hotkey registration errors do not crash app.  

10. System Tray Integration (Rust)  
Complexity: S  
Dependencies: Task 2  
Files: `src-tauri/src/tray.rs`  
Task: Tray icon with status and menu.  
Acceptance criteria:  
1. Tray shows idle/recording/error status states.  
2. Menu includes Settings and Quit.  
Edge cases and failure modes:  
1. Tray initialization failure logs error and continues without tray.  

11. Settings UI (Web)  
Complexity: M  
Dependencies: Task 1, Task 3, Task 9  
Files: `src/ui/components/SettingsPanel.tsx`, `src/ui/components/HotkeyPicker.tsx`, `src/ui/components/MicSelector.tsx`, `src/ui/state/store.ts`  
Task: UI for mic selection, hotkeys, replacements CRUD.  
Acceptance criteria:  
1. Mic list is populated and selection persists.  
2. Hotkey picker validates conflicts and displays error messages.  
3. Replacements editor edits `config/replacements.json` via IPC.  
Edge cases and failure modes:  
1. Sidecar offline shows a read-only UI state with reconnect option.  
2. Invalid replacement rules show inline validation messages.  

12. Status Indicator UI (Web + Rust)  
Complexity: S  
Dependencies: Task 2, Task 9  
Files: `src/ui/components/StatusIndicator.tsx`, `src/ui/state/store.ts`, `src-tauri/src/state.rs`  
Task: Visual indicator for idle/recording/transcribing/error.  
Acceptance criteria:  
1. UI status updates within 200 ms of state change.  
2. Error state provides a user-readable reason.  
Edge cases and failure modes:  
1. Unknown state falls back to idle with warning log.  

**Phase 2: Enhancements (v0.2.0)**  

13. Alternative Model Support (Whisper Turbo v3)  
Complexity: L  
Dependencies: Phase 1 complete  
Files: `sidecar/asr/whisper.py`, `sidecar/asr/device.py`, `sidecar/config/schema.py`  
Acceptance criteria:  
1. Model selection in config switches inference backend.  
2. Fallback to Parakeet when model load fails.  
Edge cases and failure modes:  
1. Model package missing produces error and reverts to default.  

14. History View (UI + Storage)  
Complexity: M  
Dependencies: Task 6, Task 7  
Files: `sidecar/text/history.py`, `src/ui/components/HistoryView.tsx`  
Acceptance criteria:  
1. Last N transcripts are stored with timestamps.  
2. UI search filters by substring.  
Edge cases and failure modes:  
1. Storage failure logs error and disables history without breaking transcription.  

15. Multi-language Support  
Complexity: M  
Dependencies: Task 5  
Files: `sidecar/asr/parakeet.py`, `sidecar/config/schema.py`, `src/ui/components/SettingsPanel.tsx`  
Acceptance criteria:  
1. Language selection affects model inference language.  
2. Auto language mode supported if model provides it.  
Edge cases and failure modes:  
1. Unsupported language gracefully falls back to default.  

16. Auto-start on Boot  
Complexity: M  
Dependencies: Phase 1 complete  
Files: `src-tauri/src/main.rs`, `src-tauri/tauri.conf.json`  
Acceptance criteria:  
1. Setting toggles system auto-start on supported OS.  
2. App handles auto-start path changes.  
Edge cases and failure modes:  
1. Permission denied yields a clear error and resets setting.  

17. Update Checker  
Complexity: M  
Dependencies: Phase 1 complete  
Files: `src-tauri/src/main.rs`, `src/ui/components/SettingsPanel.tsx`  
Acceptance criteria:  
1. Manual check reports current version and latest.  
2. Failed check reports error without blocking app.  
Edge cases and failure modes:  
1. Offline mode shows graceful error and retry option.  

**Phase 3: Advanced Features (v0.3.0+)**  

18. AI Commands and Voice Commands  
Complexity: L  
Dependencies: Phase 2 complete  
Files: `sidecar/text/commands.py`, `sidecar/ipc/messages.py`, `src/ui/components/SettingsPanel.tsx`  
Acceptance criteria:  
1. Commands are parsed and routed before injection.  
2. Commands are disable-able per user setting.  
Edge cases and failure modes:  
1. Ambiguous command falls back to literal text.  

19. Custom Wake Word  
Complexity: L  
Dependencies: Task 4  
Files: `sidecar/audio/wake_word.py`  
Acceptance criteria:  
1. Wake word triggers recording reliably without hotkey.  
2. Always-listening mode is clearly indicated.  
Edge cases and failure modes:  
1. Wake word model failure disables feature with warning.  

20. Cloud Sync for Replacements  
Complexity: L  
Dependencies: Task 7  
Files: `sidecar/sync/client.py`, `sidecar/config/schema.py`  
Acceptance criteria:  
1. Sync merges remote and local rules with conflict handling.  
2. Offline mode queues changes.  
Edge cases and failure modes:  
1. Sync errors do not block local edits.  

21. Plugin System  
Complexity: L  
Dependencies: Phase 2 complete  
Files: `sidecar/plugins/loader.py`, `sidecar/plugins/schema.py`  
Acceptance criteria:  
1. Plugin discovery loads from `plugins/` directory.  
2. Plugin failures are isolated and logged.  
Edge cases and failure modes:  
1. Version incompatibility prevents load with clear error.  

**Critical Path and Dependencies**  
1. IPC definitions are foundational for all Rust-Python interactions.  
2. Sidecar lifecycle must be stable before hotkeys, tray, and UI can rely on state.  
3. Audio capture precedes ASR inference, which precedes post-processing and replacements.  
4. Text injection depends on post-processing and replacements output.  
5. Hotkey handling gates the end-to-end flow (record → transcribe → inject).  
6. Settings UI depends on IPC to fetch devices, update hotkeys, and persist replacements.

**Parallel Execution Design**  
1. Rust Core team can work on `src-tauri/src/ipc/*`, `src-tauri/src/inject.rs`, `src-tauri/src/hotkeys.rs`, and `src-tauri/src/tray.rs` in parallel, gated by the IPC schema contract.  
2. Python Sidecar team can implement `sidecar/audio/*`, `sidecar/asr/*`, and `sidecar/text/*` simultaneously, using a shared `sidecar/ipc/messages.py` contract.  
3. UI team can build `src/ui/components/*` and state store with mocked IPC responses until Rust endpoints are ready.  
4. Integration team focuses on end-to-end flows and error handling once IPC is stable.

**Risk Mitigation**  
1. ASR model compatibility risk: maintain a CPU fallback path and surface model-load errors to UI.  
2. Audio capture stability risk: implement retry and fallback to default device on failure.  
3. Hotkey conflicts: detect and report conflicts before registration.  
4. Text injection edge cases: serialize injection calls and add small configurable delay.  
5. Sidecar crash loop: exponential backoff and clear UI error state.  
6. Config corruption: keep last known good config and validate JSON schema before apply.