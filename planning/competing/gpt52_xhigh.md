## 1) Executive Summary (TL;DR)

Build OpenVoicy v0.1.0 (MVP) as a Tauri 2 desktop app that:
1) captures audio via a Python sidecar, 2) transcribes offline with Parakeet V3, 3) post-processes (punctuation + replacements), and 4) injects text into the currently focused field.  
The critical path is: **IPC protocol → sidecar lifecycle → record/stop → transcribe → inject → global push-to-talk hotkey + tray indicator → settings UI**.

---

## 2) Architecture Overview

### 2.1 Components
- **Tauri Core (Rust)**: global hotkey (press/release), tray + status, config persistence, sidecar management, text injection.
- **Web UI (React/TS)**: settings (mic, hotkey, injection delay, replacements CRUD), status indicator.
- **Python Sidecar**: audio device enumeration + recording, ASR backend selection (CUDA/MLX/CPU), post-processing pipeline, replacement engine, JSON-RPC server over stdin/stdout.

### 2.2 IPC (JSON-RPC 2.0 over NDJSON)
**Transport**: newline-delimited JSON on sidecar stdin/stdout.  
**Framing rule**: one JSON object per line; responses match `id`; notifications have no `id`.

**Shared protocol definition**
- File: `shared/protocol/openvoicy_rpc_v1.json` (single source of truth)
- Generated types:
  - TS: `src/ipc/generated.ts`
  - Rust: `src-tauri/src/ipc/generated.rs`
  - Python: `python_sidecar/openvoicy_sidecar/ipc/generated.py`

**Core RPC methods (v1)**
- `system.ping() -> { version: string }`
- `audio.list_devices() -> { devices: AudioDevice[] }`
- `audio.set_device({ device_id: string | null }) -> { active_device_id: string | null }`
- `recording.start({ sample_rate_hz: number, channels: 1, max_seconds: number }) -> { session_id: string }`
- `recording.stop({ session_id: string }) -> { wav_bytes_b64: string, duration_ms: number }`
- `asr.load_model({ model: "parakeet-v3-0.6b", device_pref: "cuda"|"mlx"|"cpu"|"auto" }) -> { ready: boolean }`
- `asr.transcribe({ wav_bytes_b64: string, language?: string }) -> { text: string, tokens?: any, latency_ms: number }`
- `text.apply_replacements({ text: string }) -> { text: string }`
- `config.set({ config: SidecarConfig }) -> { ok: boolean }`
- `status.get() -> { state: "idle"|"recording"|"transcribing"|"error", detail?: string }`
- Notifications: `event.state_changed`, `event.error`, `event.device_changed`

### 2.3 Data & Config
- **Rust config** (persisted via Tauri path APIs):
  - `AppConfig` fields:
    - `microphone_device_id: Option<String>`
    - `hotkey: HotkeySpec` (see below)
    - `injection: InjectionConfig` (`mode`, `delay_ms`, `restore_clipboard`)
    - `sidecar: SidecarConfig` (`device_pref`, `model`, `max_record_seconds`)
    - `replacements_path: PathBuf`
- **HotkeySpec**
  - `{ modifiers: ["Ctrl","Alt","Shift","Meta"], key: "Space" }` (explicit, cross-platform mapping table)
- **Replacements config (JSON)**
  - Default shipped: `assets/default_replacements.json`
  - User editable: in app data, e.g. `OpenVoicy/replacements.json`
  - Schema:
    - `ReplacementRule`:
      - `{ id, type: "snippet", trigger, replace, word_boundary: bool, case_mode: "preserve"|"lower"|"upper" }`
      - `{ id, type: "macro", trigger, macro: "date"|"time"|"email", format?: string }`
      - `{ id, type: "regex", pattern, replace, flags?: "i"|"m"|"s" }`

---

## 3) Phase Breakdown With Numbered Tasks (with dependencies, acceptance criteria, complexity)

### Phase 0 — Repo scaffolding + protocol (Foundation)

**0.1 Define protocol + generate types (Owner: Agent-Platform) — (M)**
- Files:
  - `shared/protocol/openvoicy_rpc_v1.json`
  - `scripts/gen_ipc_types.ts` (or `scripts/gen_ipc_types.py`)
  - `src/ipc/generated.ts`
  - `src-tauri/src/ipc/generated.rs`
  - `python_sidecar/openvoicy_sidecar/ipc/generated.py`
- Acceptance criteria:
  - Single JSON schema drives all three language bindings.
  - Backward-compatible versioning: `protocol_version` string embedded in `system.ping`.
  - CI/check step fails if generated files are out of date.
- Edge cases:
  - Unknown fields ignored (forward compatibility).
  - Strict validation for required fields.

**0.2 Sidecar packaging contract (Owner: Agent-Platform) — (S)**
- Files:
  - `src-tauri/tauri.conf.json` (sidecar declaration, bundle resources)
  - `python_sidecar/pyproject.toml` (entrypoint console script `openvoicy-sidecar`)
  - `python_sidecar/openvoicy_sidecar/__main__.py`
- Acceptance criteria:
  - Tauri can spawn sidecar on all OS targets (dev mode at minimum).
  - Sidecar responds to `system.ping` within 1s or Rust shows a clear error state.

---

### Phase 1 — Python sidecar (Audio + ASR + post-processing)

**1.1 JSON-RPC server loop + dispatcher (Owner: Agent-Python) — (M)**
- Files:
  - `python_sidecar/openvoicy_sidecar/ipc/server.py` (`read_loop()`, `write_json()`, `dispatch(req)`)
  - `python_sidecar/openvoicy_sidecar/ipc/errors.py` (typed error codes)
- Key functions:
  - `run_server(stdin, stdout)`
  - `handle_request(request: RpcRequest) -> RpcResponse | None`
- Acceptance criteria:
  - Handles concurrent requests safely (serialize or queue; deterministic ordering).
  - Produces valid JSON-RPC errors with stable codes (`E_INVALID_PARAMS`, `E_INTERNAL`, `E_NOT_READY`).
  - Never deadlocks if Rust disconnects; exits cleanly on EOF.

**1.2 Audio device enumeration + selection (Owner: Agent-Python) — (M)**
- Files:
  - `python_sidecar/openvoicy_sidecar/audio/devices.py` (`list_devices()`, `set_active_device(id)`)
  - `python_sidecar/openvoicy_sidecar/audio/types.py` (`AudioDevice`)
- Acceptance criteria:
  - Returns stable `device_id` and human name.
  - If device disappears mid-session, emits `event.device_changed` and falls back to default device.
- Failure modes:
  - No input devices → return empty list; Rust UI shows “No microphone found”.

**1.3 Push-to-talk recording engine (Owner: Agent-Python) — (L)**
- Files:
  - `python_sidecar/openvoicy_sidecar/audio/recorder.py`
- Key structures:
  - `RecordingSession { session_id, sample_rate_hz, channels, started_at, frames: deque[np.ndarray] }`
- Acceptance criteria:
  - `recording.start` begins capturing within 100ms (best-effort).
  - `recording.stop` returns valid WAV bytes (mono, 16-bit PCM) and duration.
  - Enforces `max_seconds` to prevent unbounded memory; on cap hit emits `event.error` and stops.
- Edge cases:
  - Permission denied (macOS mic permission) → explicit error `E_MIC_PERMISSION`.
  - Audio callback overrun → drop frames + warn; still returns best-effort audio.

**1.4 ASR backend interface + Parakeet V3 implementation (Owner: Agent-ML) — (L)**
- Files:
  - `python_sidecar/openvoicy_sidecar/asr/base.py` (`class AsrBackend(Protocol)`)
  - `python_sidecar/openvoicy_sidecar/asr/parakeet_v3.py` (`load_model()`, `transcribe_wav_bytes()`)
  - `python_sidecar/openvoicy_sidecar/asr/device.py` (`select_device(device_pref)`)
- Acceptance criteria:
  - `asr.load_model` downloads/loads model once and caches (configurable path).
  - `asr.transcribe` returns text for a known test WAV fixture.
  - Device selection:
    - Windows/Linux: CUDA if available and chosen.
    - macOS: MLX if implemented; otherwise returns `E_DEVICE_UNSUPPORTED` and falls back to CPU when `auto`.
- Risk mitigation hooks:
  - Implement backend registry so Parakeet can be swapped without touching IPC: `ASR_BACKENDS: dict[str, AsrBackendFactory]`.

**1.5 Post-processing pipeline (punctuation + normalization) (Owner: Agent-Python) — (M)**
- Files:
  - `python_sidecar/openvoicy_sidecar/text/postprocess.py` (`postprocess(text, config)`)
- Acceptance criteria:
  - Guarantees output is valid Unicode, trimmed, no control chars except `\n` and `\t`.
  - Optional “auto capitalization” applied only when confidence heuristic is met (configurable).
- Edge cases:
  - Empty or whitespace-only transcription → return empty; Rust does not inject.

**1.6 Replacement engine (Owner: Agent-Python) — (M)**
- Files:
  - `python_sidecar/openvoicy_sidecar/text/replacements.py`
  - `python_sidecar/openvoicy_sidecar/text/replacements_schema.py` (validation)
- Key functions:
  - `load_rules(path) -> list[ReplacementRule]`
  - `apply_rules(text, rules) -> str`
- Acceptance criteria:
  - Snippet expansion respects word boundaries.
  - Macros `@@date`, `@@time`, `@@email` work with locale-safe formatting and configurable templates.
  - Regex rules are optional and sandboxed (timeouts or max steps) to avoid catastrophic backtracking.

---

### Phase 2 — Rust core (Tauri: hotkeys, tray, injection, sidecar lifecycle)

**2.1 Sidecar manager + RPC client (Owner: Agent-Rust) — (L)**
- Files:
  - `src-tauri/src/sidecar/mod.rs` (`SidecarManager`)
  - `src-tauri/src/sidecar/rpc.rs` (`send_request()`, `read_loop()`, correlation map by `id`)
  - `src-tauri/src/sidecar/types.rs` (mirrors generated types if needed)
- Key structs:
  - `struct SidecarManager { child: Child, pending: HashMap<u64, oneshot::Sender<_>>, state: Arc<RwLock<SidecarState>> }`
- Acceptance criteria:
  - Robust startup/shutdown; if sidecar crashes, Rust transitions to `error` and offers “Restart sidecar”.
  - Request timeout defaults (e.g., 30s for transcription).
  - Backpressure: prevent multiple transcriptions in flight (queue or reject with UI message).

**2.2 Global push-to-talk hotkey with press/release (Owner: Agent-Rust) — (L)**
- Files:
  - `src-tauri/src/hotkeys/mod.rs` (`HotkeyManager`)
  - `src-tauri/src/hotkeys/platform/{windows,macos,linux}.rs`
- Acceptance criteria:
  - Detects key-down to start recording and key-up to stop (true hold-to-talk).
  - Hotkey conflict detection: if registration fails, UI surfaces exact reason and suggests alternatives.
- Edge cases:
  - macOS Accessibility permission missing → show actionable instructions.
  - Focused UI typing does not trigger recording unless configured (avoid accidental activation).

**2.3 Recording orchestration (Rust ↔ sidecar) (Owner: Agent-Rust) — (M)**
- Files:
  - `src-tauri/src/recording/controller.rs` (`start_ptt()`, `stop_ptt()`)
  - `src-tauri/src/state.rs` (`AppState { recording_session_id, status }`)
- Acceptance criteria:
  - Hotkey down → `recording.start`.
  - Hotkey up → `recording.stop` → `asr.transcribe` → `text.apply_replacements` → inject.
  - State machine prevents double-start/double-stop; idempotent stop.
- Failure modes:
  - If stop fails, controller resets to `idle` and reports error.

**2.4 Text injection (unicode-safe) (Owner: Agent-Rust) — (M)**
- Files:
  - `src-tauri/src/injection/mod.rs`
  - `src-tauri/src/injection/clipboard.rs` (paste mode)
  - `src-tauri/src/injection/keystroke.rs` (enigo typing mode)
- Key config:
  - `enum InjectionMode { Paste, Type }`
- Acceptance criteria:
  - Default mode `Paste` guarantees Unicode injection reliably.
  - Optional `Type` uses enigo; for characters not representable, falls back to paste.
  - Injection delay respected (`delay_ms`) and does not freeze UI (async).
- Edge cases:
  - Clipboard restore enabled: save prior clipboard, set text, paste, restore within 500ms (best-effort).
  - Secure fields (password inputs) may block paste/type; show warning instead of looping.

**2.5 System tray + status indicator (Owner: Agent-Rust) — (M)**
- Files:
  - `src-tauri/src/tray.rs`
  - `src-tauri/src/status.rs`
- Acceptance criteria:
  - Tray icon reflects state: idle/recording/transcribing/error.
  - Tray menu: “Open Settings”, “Restart Sidecar”, “Quit”.
  - Tooltip shows current state + mic name.

**2.6 Config persistence + migration (Owner: Agent-Rust) — (S/M)**
- Files:
  - `src-tauri/src/config.rs` (`AppConfig`, `load_config()`, `save_config()`, `migrate_config(v)`)
- Acceptance criteria:
  - On first run, creates defaults and copies `assets/default_replacements.json` into app data.
  - Config changes from UI apply immediately and survive restart.

---

### Phase 3 — Frontend UI (React/TS)

**3.1 IPC bridge (Tauri invoke + events) (Owner: Agent-UI) — (M)**
- Files:
  - `src/ipc/client.ts` (`invokeRpc()`, `subscribeEvents()`)
  - `src/state/appStore.ts` (status, devices, config)
- Acceptance criteria:
  - UI can show sidecar status, devices list, active mic, last error.
  - Event-driven updates (no polling except fallback).

**3.2 Settings UI: microphone selection + hotkey config (Owner: Agent-UI) — (M)**
- Files:
  - `src/pages/Settings.tsx`
  - `src/components/MicrophonePicker.tsx`
  - `src/components/HotkeyRecorder.tsx` (captures key combo in UI, validates)
- Acceptance criteria:
  - Mic dropdown updates active device in sidecar and persists config.
  - Hotkey editor prevents invalid combos (e.g., modifier-only) and warns on common OS conflicts.

**3.3 Replacements CRUD UI (Owner: Agent-UI) — (M)**
- Files:
  - `src/components/ReplacementsEditor.tsx`
  - `src/components/ReplacementRuleForm.tsx`
- Acceptance criteria:
  - Add/edit/delete rules; validate schema client-side; save to replacements JSON path.
  - Quick test box: input text → preview output (calls `text.apply_replacements` via RPC).

**3.4 Status indicator view (Owner: Agent-UI) — (S)**
- Files:
  - `src/components/StatusPill.tsx`
  - `src/App.tsx`
- Acceptance criteria:
  - Always visible recording/transcribing indicator with last action timestamp.
  - Error banner includes “Copy diagnostics” (collects last N logs / state snapshot from Rust).

---

### Phase 4 — QA, packaging, and reliability

**4.1 Cross-platform permissions & manifests (Owner: Agent-Platform) — (M)**
- Files:
  - `src-tauri/tauri.conf.json` (macOS entitlements, Windows settings)
  - macOS plist additions (via Tauri config): `NSMicrophoneUsageDescription`
- Acceptance criteria:
  - App prompts for mic permission where applicable.
  - Documentation page shown when permissions missing (deep link in UI/tray).

**4.2 Logging + diagnostics bundle (Owner: Agent-Rust/Python) — (M)**
- Files:
  - `src-tauri/src/logging.rs`
  - `python_sidecar/openvoicy_sidecar/logging.py`
- Acceptance criteria:
  - Logs written to per-user app data.
  - “Copy diagnostics” exports: app version, protocol version, device list, last error codes.

**4.3 Automated smoke tests (Owner: Agent-QA) — (M)**
- Files:
  - `python_sidecar/tests/test_replacements.py`
  - `python_sidecar/tests/test_ipc_ping.py`
  - `src-tauri/src/injection/tests.rs` (unit tests for encoding/escaping decisions)
- Acceptance criteria:
  - Python tests run offline with a small WAV fixture.
  - Rust unit tests cover injection mode selection and clipboard restore logic (mocked).

---

## 4) Critical Path and Dependencies

### 4.1 Critical path (must land in order)
1. **0.1 Protocol + generated types**
2. **2.1 Rust sidecar manager + Python 1.1 RPC server**
3. **1.2 devices + 1.3 recording**
4. **1.4 model load + transcription**
5. **2.4 injection**
6. **2.2 press/release hotkey**
7. **2.5 tray status + 3.4 UI status**
8. **3.2 settings (mic + hotkey)**
9. **1.6 replacements + 3.3 replacements UI**

### 4.2 Parallelizable workstreams
- **Workstream A (Protocol/Platform)**: 0.1, 0.2, 4.1
- **Workstream B (Python)**: 1.1, 1.2, 1.3, 1.5, 1.6
- **Workstream C (Rust)**: 2.1, 2.3, 2.4, 2.5, 2.6
- **Workstream D (UI)**: 3.1, 3.2, 3.3, 3.4
- **Workstream E (QA/Perf)**: 4.2, 4.3, latency profiling scripts

**Coordination points**
- After **0.1**: all teams lock method names + types.
- After **2.1 + 1.1**: first end-to-end ping and status display.
- After **1.3 + 2.3**: end-to-end record/stop WAV path.
- After **1.4 + 2.4**: full transcribe → inject loop.

---

## 5) Risk Mitigation

- **Hotkey hold-to-talk across OS (High risk)**: implement platform-specific key event hooks behind `HotkeyManager` with a fallback “toggle-to-record” mode (`hotkey.mode = "hold"|"toggle"`) to keep MVP shippable.
- **Parakeet V3 backend feasibility on MLX (High risk)**: keep backend registry; ship CPU fallback guaranteed; make CUDA/MLX optional accelerators with clear UI state and error codes.
- **Python sidecar distribution (High risk)**: treat sidecar as an embedded, versioned artifact; pin dependencies; add `system.ping` + `asr.load_model` health checks and a “Restart sidecar” action.
- **Unicode injection reliability (Medium risk)**: default to clipboard paste with restore; use keystroke typing only as optional mode.
- **Audio glitches/overruns (Medium risk)**: bounded ring buffer, max duration, and explicit overrun warnings; degrade gracefully with partial audio instead of crashing.
- **Permissions (Medium risk)**: detect and surface missing mic/accessibility permissions with step-by-step OS-specific instructions and deep links where possible.
- **Runaway regex replacements (Medium risk)**: restrict regex rules (or off by default) with timeouts/length limits; validate patterns before enabling.