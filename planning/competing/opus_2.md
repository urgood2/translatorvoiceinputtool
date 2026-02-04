# OpenVoicy - Implementation Plan v1

**Version:** 0.1.0 (MVP)
**Date:** 2026-02-04
**Status:** Ready for Implementation

---

## Executive Summary (TL;DR)

Build a cross-platform speech-to-text tool that:
1. Captures audio via push-to-talk hotkey
2. Transcribes using Parakeet V3 (offline)
3. Injects text into any focused field

**MVP Goal:** Working end-to-end flow in ~2 weeks with 4 parallel work streams:
- **Stream A:** Tauri shell (Rust) - hotkeys, tray, text injection
- **Stream B:** Python sidecar - audio capture, ASR inference
- **Stream C:** React UI - settings, status display
- **Stream D:** Integration & IPC - glue everything together

**Critical Path:** Python ASR → IPC Protocol → Text Injection → Integration

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                         OpenVoicy                                │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────┐         ┌─────────────────────────────────────┐│
│  │ System Tray │         │         React Frontend              ││
│  │  (Rust)     │         │  ┌─────────┐ ┌──────────┐ ┌──────┐ ││
│  │             │         │  │Settings │ │ Status   │ │Replac│ ││
│  │ • Icon      │         │  │ Panel   │ │ Display  │ │ments │ ││
│  │ • Menu      │         │  └─────────┘ └──────────┘ └──────┘ ││
│  │ • Tooltip   │         └─────────────────────────────────────┘│
│  └──────┬──────┘                          │                     │
│         │                                 │ Tauri Commands      │
│         ▼                                 ▼                     │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                    Tauri Core (Rust)                       │ │
│  │  ┌──────────────┐ ┌──────────────┐ ┌────────────────────┐  │ │
│  │  │ Hotkey Mgr   │ │ Text Inject  │ │  Sidecar Manager   │  │ │
│  │  │ (global_     │ │ (enigo crate)│ │  (spawn, monitor,  │  │ │
│  │  │ shortcut)    │ │              │ │   restart)         │  │ │
│  │  └──────────────┘ └──────────────┘ └─────────┬──────────┘  │ │
│  └──────────────────────────────────────────────┼─────────────┘ │
│                                                 │               │
│                              JSON-RPC over stdin/stdout         │
│                                                 │               │
│  ┌──────────────────────────────────────────────┴─────────────┐ │
│  │                  Python Sidecar                            │ │
│  │  ┌──────────────┐ ┌──────────────┐ ┌────────────────────┐  │ │
│  │  │ Audio Capture│ │ ASR Engine   │ │ Text Processor     │  │ │
│  │  │ (sounddevice)│ │ (NeMo/MLX)   │ │ (replacements,     │  │ │
│  │  │              │ │ Parakeet V3  │ │  punctuation)      │  │ │
│  │  └──────────────┘ └──────────────┘ └────────────────────┘  │ │
│  └────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

### Directory Structure

```
openvoicy/
├── src-tauri/                    # Rust/Tauri backend
│   ├── src/
│   │   ├── main.rs              # Entry point, app setup
│   │   ├── lib.rs               # Library exports
│   │   ├── hotkey.rs            # Global hotkey management
│   │   ├── tray.rs              # System tray setup
│   │   ├── inject.rs            # Text injection via enigo
│   │   ├── sidecar.rs           # Python process management
│   │   ├── ipc.rs               # JSON-RPC protocol handling
│   │   ├── state.rs             # App state management
│   │   └── commands.rs          # Tauri command handlers
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                          # React frontend
│   ├── App.tsx
│   ├── main.tsx
│   ├── components/
│   │   ├── StatusIndicator.tsx
│   │   ├── SettingsPanel.tsx
│   │   ├── MicrophoneSelect.tsx
│   │   ├── HotkeyConfig.tsx
│   │   └── ReplacementEditor.tsx
│   ├── hooks/
│   │   ├── useTauriEvents.ts
│   │   └── useSettings.ts
│   ├── lib/
│   │   └── tauri.ts             # Tauri IPC wrappers
│   └── styles/
│       └── globals.css
├── sidecar/                      # Python ASR sidecar
│   ├── openvoicy/
│   │   ├── __init__.py
│   │   ├── __main__.py          # Entry point (JSON-RPC server)
│   │   ├── audio.py             # Audio capture
│   │   ├── asr.py               # Parakeet V3 inference
│   │   ├── processor.py         # Text post-processing
│   │   └── protocol.py          # JSON-RPC message handling
│   ├── pyproject.toml
│   └── requirements.txt
├── package.json
├── tsconfig.json
├── vite.config.ts
└── README.md
```

---

## Phase 1: Foundation (Days 1-3)

### Task 1.1: Project Scaffolding [S]
**Owner:** Any agent
**Dependencies:** None
**Files:**
- `package.json` - npm project config
- `tsconfig.json` - TypeScript config
- `vite.config.ts` - Vite bundler config
- `src-tauri/Cargo.toml` - Rust dependencies
- `src-tauri/tauri.conf.json` - Tauri app config
- `sidecar/pyproject.toml` - Python project config

**Implementation:**
```bash
# Commands to run
npm create tauri-app@latest openvoicy -- --template react-ts
cd openvoicy
# Add Python sidecar structure
mkdir -p sidecar/openvoicy
```

**Cargo.toml dependencies:**
```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon", "global-shortcut"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
enigo = "0.2"
```

**Acceptance Criteria:**
- [ ] `npm run tauri dev` launches empty window
- [ ] Python sidecar module importable
- [ ] Directory structure matches spec

---

### Task 1.2: IPC Protocol Definition [S]
**Owner:** Stream D (Integration)
**Dependencies:** None
**Files:**
- `sidecar/openvoicy/protocol.py`
- `src-tauri/src/ipc.rs`
- `docs/IPC_PROTOCOL.md` (optional)

**Protocol Specification:**
```typescript
// JSON-RPC 2.0 over stdin/stdout

// Request: Tauri → Python
interface Request {
  jsonrpc: "2.0";
  id: number;
  method: string;
  params?: object;
}

// Response: Python → Tauri
interface Response {
  jsonrpc: "2.0";
  id: number;
  result?: any;
  error?: { code: number; message: string };
}

// Notification: Python → Tauri (no id, no response expected)
interface Notification {
  jsonrpc: "2.0";
  method: string;
  params: object;
}
```

**Methods (Tauri → Python):**
| Method | Params | Returns | Description |
|--------|--------|---------|-------------|
| `initialize` | `{model_path?: string}` | `{status: "ready"}` | Load ASR model |
| `start_recording` | `{device_id?: int}` | `{status: "recording"}` | Begin capture |
| `stop_recording` | `{}` | `{audio_duration_ms: int}` | End capture, start transcription |
| `cancel_recording` | `{}` | `{status: "cancelled"}` | Abort without transcribing |
| `list_devices` | `{}` | `{devices: [{id, name, is_default}]}` | Enumerate mics |
| `set_replacements` | `{rules: [{from, to}]}` | `{count: int}` | Update replacement rules |

**Notifications (Python → Tauri):**
| Method | Params | Description |
|--------|--------|-------------|
| `transcription_complete` | `{text: string, confidence: float, duration_ms: int}` | Final result |
| `transcription_error` | `{code: string, message: string}` | Error during transcription |
| `status_changed` | `{status: "idle" \| "loading" \| "recording" \| "transcribing"}` | State change |

**Python Implementation (`protocol.py`):**
```python
import json
import sys
from typing import Any, Callable
from dataclasses import dataclass

@dataclass
class JsonRpcMessage:
    jsonrpc: str = "2.0"
    
class JsonRpcHandler:
    def __init__(self):
        self._methods: dict[str, Callable] = {}
        self._next_id = 1
    
    def register(self, method: str, handler: Callable):
        self._methods[method] = handler
    
    def send_notification(self, method: str, params: dict):
        msg = {"jsonrpc": "2.0", "method": method, "params": params}
        print(json.dumps(msg), flush=True)
    
    def handle_line(self, line: str) -> str | None:
        try:
            req = json.loads(line)
            method = req.get("method")
            params = req.get("params", {})
            req_id = req.get("id")
            
            if method not in self._methods:
                return self._error_response(req_id, -32601, f"Method not found: {method}")
            
            result = self._methods[method](**params)
            return json.dumps({"jsonrpc": "2.0", "id": req_id, "result": result})
        except Exception as e:
            return self._error_response(req.get("id"), -32603, str(e))
    
    def _error_response(self, id: int | None, code: int, message: str) -> str:
        return json.dumps({
            "jsonrpc": "2.0", 
            "id": id, 
            "error": {"code": code, "message": message}
        })
```

**Rust Implementation (`ipc.rs`):**
```rust
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::oneshot;
use std::collections::HashMap;
use std::sync::Mutex;

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize)]
struct Request<'a, P: Serialize> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    params: P,
}

#[derive(Deserialize)]
struct Response {
    id: Option<u64>,
    result: Option<serde_json::Value>,
    error: Option<RpcError>,
    method: Option<String>,  // For notifications
    params: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RpcError {
    code: i32,
    message: String,
}

pub struct IpcClient {
    stdin: tokio::sync::Mutex<ChildStdin>,
    pending: Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>,
}

impl IpcClient {
    pub async fn call<P: Serialize>(&self, method: &str, params: P) -> Result<serde_json::Value, String> {
        let id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);
        let req = Request { jsonrpc: "2.0", id, method, params };
        
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        
        let mut stdin = self.stdin.lock().await;
        let line = serde_json::to_string(&req).unwrap();
        stdin.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
        stdin.write_all(b"\n").await.map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())?;
        
        rx.await.map_err(|_| "Channel closed".to_string())
    }
}
```

**Acceptance Criteria:**
- [ ] Python handler correctly parses requests and sends responses
- [ ] Rust client can send requests and receive responses
- [ ] Notifications flow Python → Rust correctly
- [ ] Error cases return proper JSON-RPC error objects

---

### Task 1.3: Python Sidecar Entry Point [S]
**Owner:** Stream B (Python)
**Dependencies:** Task 1.2
**Files:**
- `sidecar/openvoicy/__main__.py`
- `sidecar/openvoicy/__init__.py`

**Implementation (`__main__.py`):**
```python
#!/usr/bin/env python3
"""OpenVoicy ASR Sidecar - JSON-RPC server over stdin/stdout."""
import sys
import signal
from .protocol import JsonRpcHandler
from .audio import AudioCapture
from .asr import ASREngine
from .processor import TextProcessor

def main():
    handler = JsonRpcHandler()
    audio = AudioCapture()
    asr: ASREngine | None = None
    processor = TextProcessor()
    
    @handler.register("initialize")
    def initialize(model_path: str | None = None):
        nonlocal asr
        handler.send_notification("status_changed", {"status": "loading"})
        asr = ASREngine(model_path)
        handler.send_notification("status_changed", {"status": "idle"})
        return {"status": "ready"}
    
    @handler.register("list_devices")
    def list_devices():
        return {"devices": audio.list_devices()}
    
    @handler.register("start_recording")
    def start_recording(device_id: int | None = None):
        audio.start(device_id)
        handler.send_notification("status_changed", {"status": "recording"})
        return {"status": "recording"}
    
    @handler.register("stop_recording")
    def stop_recording():
        audio_data, duration_ms = audio.stop()
        handler.send_notification("status_changed", {"status": "transcribing"})
        
        try:
            text, confidence = asr.transcribe(audio_data)
            text = processor.process(text)
            handler.send_notification("transcription_complete", {
                "text": text,
                "confidence": confidence,
                "duration_ms": duration_ms
            })
        except Exception as e:
            handler.send_notification("transcription_error", {
                "code": "TRANSCRIPTION_FAILED",
                "message": str(e)
            })
        finally:
            handler.send_notification("status_changed", {"status": "idle"})
        
        return {"audio_duration_ms": duration_ms}
    
    @handler.register("cancel_recording")
    def cancel_recording():
        audio.cancel()
        handler.send_notification("status_changed", {"status": "idle"})
        return {"status": "cancelled"}
    
    @handler.register("set_replacements")
    def set_replacements(rules: list[dict]):
        processor.set_rules(rules)
        return {"count": len(rules)}
    
    # Graceful shutdown
    def shutdown(signum, frame):
        sys.exit(0)
    
    signal.signal(signal.SIGTERM, shutdown)
    signal.signal(signal.SIGINT, shutdown)
    
    # Main loop: read stdin line by line
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        response = handler.handle_line(line)
        if response:
            print(response, flush=True)

if __name__ == "__main__":
    main()
```

**Acceptance Criteria:**
- [ ] `python -m openvoicy` starts and waits for stdin input
- [ ] Responds to `{"jsonrpc":"2.0","id":1,"method":"list_devices","params":{}}` with device list
- [ ] Clean shutdown on SIGTERM/SIGINT

---

## Phase 2: Audio & ASR (Days 2-5)

### Task 2.1: Audio Capture Module [M]
**Owner:** Stream B (Python)
**Dependencies:** Task 1.3
**Files:**
- `sidecar/openvoicy/audio.py`

**Implementation:**
```python
"""Audio capture using sounddevice."""
import sounddevice as sd
import numpy as np
from threading import Lock
from typing import Optional
import time

class AudioCapture:
    SAMPLE_RATE = 16000  # Parakeet expects 16kHz
    CHANNELS = 1
    DTYPE = np.float32
    
    def __init__(self):
        self._buffer: list[np.ndarray] = []
        self._lock = Lock()
        self._stream: Optional[sd.InputStream] = None
        self._start_time: float = 0
    
    def list_devices(self) -> list[dict]:
        """Return list of input devices."""
        devices = []
        for i, dev in enumerate(sd.query_devices()):
            if dev['max_input_channels'] > 0:
                devices.append({
                    "id": i,
                    "name": dev['name'],
                    "is_default": i == sd.default.device[0]
                })
        return devices
    
    def start(self, device_id: Optional[int] = None):
        """Start recording audio."""
        with self._lock:
            self._buffer = []
            self._start_time = time.time()
        
        def callback(indata, frames, time_info, status):
            with self._lock:
                self._buffer.append(indata.copy())
        
        self._stream = sd.InputStream(
            device=device_id,
            samplerate=self.SAMPLE_RATE,
            channels=self.CHANNELS,
            dtype=self.DTYPE,
            callback=callback
        )
        self._stream.start()
    
    def stop(self) -> tuple[np.ndarray, int]:
        """Stop recording and return audio data + duration in ms."""
        if self._stream:
            self._stream.stop()
            self._stream.close()
            self._stream = None
        
        with self._lock:
            if not self._buffer:
                return np.array([], dtype=self.DTYPE), 0
            
            audio = np.concatenate(self._buffer, axis=0).flatten()
            duration_ms = int((time.time() - self._start_time) * 1000)
            self._buffer = []
            return audio, duration_ms
    
    def cancel(self):
        """Cancel recording without returning data."""
        if self._stream:
            self._stream.stop()
            self._stream.close()
            self._stream = None
        with self._lock:
            self._buffer = []
```

**Acceptance Criteria:**
- [ ] `list_devices()` returns valid device list
- [ ] Recording captures audio at 16kHz mono
- [ ] Buffer concatenation produces correct waveform
- [ ] Thread-safe buffer access
- [ ] Unit test with mock stream

---

### Task 2.2: Parakeet V3 ASR Engine [L]
**Owner:** Stream B (Python)
**Dependencies:** Task 2.1
**Files:**
- `sidecar/openvoicy/asr.py`

**Critical Design Decisions:**
1. **Model Loading:** Use NVIDIA NeMo for CUDA, MLX for Apple Silicon
2. **Lazy Loading:** Don't load model until `initialize()` called
3. **Memory:** Model ~2.5GB VRAM, need graceful fallback to CPU

**Implementation:**
```python
"""ASR engine using Parakeet V3."""
import numpy as np
from typing import Optional, Tuple
import platform
import os

class ASREngine:
    def __init__(self, model_path: Optional[str] = None):
        self._model = None
        self._backend = self._detect_backend()
        self._load_model(model_path)
    
    def _detect_backend(self) -> str:
        """Detect best available backend."""
        # Check for Apple Silicon
        if platform.system() == "Darwin" and platform.machine() == "arm64":
            try:
                import mlx
                return "mlx"
            except ImportError:
                pass
        
        # Check for CUDA
        try:
            import torch
            if torch.cuda.is_available():
                return "cuda"
        except ImportError:
            pass
        
        return "cpu"
    
    def _load_model(self, model_path: Optional[str] = None):
        """Load Parakeet V3 model."""
        model_name = model_path or "nvidia/parakeet-tdt_ctc-0.6b-v2"
        
        if self._backend == "mlx":
            self._load_mlx(model_name)
        else:
            self._load_nemo(model_name)
    
    def _load_nemo(self, model_name: str):
        """Load model using NVIDIA NeMo."""
        import nemo.collections.asr as nemo_asr
        
        device = "cuda" if self._backend == "cuda" else "cpu"
        self._model = nemo_asr.models.ASRModel.from_pretrained(
            model_name=model_name,
            map_location=device
        )
        self._model.eval()
        if device == "cuda":
            self._model.cuda()
    
    def _load_mlx(self, model_name: str):
        """Load model using MLX (Apple Silicon)."""
        # MLX implementation for Apple Silicon
        # This would use the mlx-whisper or similar port
        # For now, fall back to NeMo CPU
        import nemo.collections.asr as nemo_asr
        self._model = nemo_asr.models.ASRModel.from_pretrained(
            model_name=model_name,
            map_location="cpu"
        )
        self._model.eval()
    
    def transcribe(self, audio: np.ndarray) -> Tuple[str, float]:
        """
        Transcribe audio to text.
        
        Args:
            audio: Float32 audio at 16kHz
            
        Returns:
            (transcribed_text, confidence_score)
        """
        if len(audio) == 0:
            return "", 0.0
        
        # Ensure correct dtype
        audio = audio.astype(np.float32)
        
        # Transcribe
        result = self._model.transcribe([audio])
        
        if isinstance(result, list) and len(result) > 0:
            # NeMo returns list of strings
            text = result[0]
            confidence = 0.95  # NeMo doesn't expose confidence directly
            return text, confidence
        
        return "", 0.0
```

**Model Download Strategy:**
- First run: Download from HuggingFace Hub (~1.2GB)
- Cache in `~/.cache/openvoicy/models/`
- Show progress via status notifications

**Acceptance Criteria:**
- [ ] Model loads successfully on CUDA GPU
- [ ] Model loads successfully on CPU (fallback)
- [ ] Apple Silicon detection works (MLX optional for MVP)
- [ ] Transcription returns text with >90% accuracy on test audio
- [ ] Empty audio returns empty string gracefully
- [ ] Integration test with real audio file

---

### Task 2.3: Text Post-Processor [S]
**Owner:** Stream B (Python)
**Dependencies:** None
**Files:**
- `sidecar/openvoicy/processor.py`

**Implementation:**
```python
"""Text post-processing and replacement engine."""
import re
from typing import List, Dict
from datetime import datetime

class TextProcessor:
    def __init__(self):
        self._rules: List[Dict[str, str]] = []
        self._smart_replacements = {
            "@@date": lambda: datetime.now().strftime("%Y-%m-%d"),
            "@@time": lambda: datetime.now().strftime("%H:%M"),
            "@@datetime": lambda: datetime.now().strftime("%Y-%m-%d %H:%M"),
        }
    
    def set_rules(self, rules: List[Dict[str, str]]):
        """Set replacement rules. Each rule: {from: str, to: str}"""
        self._rules = rules
    
    def process(self, text: str) -> str:
        """Apply all post-processing to transcribed text."""
        text = self._apply_replacements(text)
        text = self._apply_smart_replacements(text)
        text = self._fix_spacing(text)
        return text
    
    def _apply_replacements(self, text: str) -> str:
        """Apply user-defined replacement rules."""
        for rule in self._rules:
            pattern = r'\b' + re.escape(rule['from']) + r'\b'
            text = re.sub(pattern, rule['to'], text, flags=re.IGNORECASE)
        return text
    
    def _apply_smart_replacements(self, text: str) -> str:
        """Apply dynamic replacements like @@date."""
        for pattern, replacement_fn in self._smart_replacements.items():
            if pattern in text:
                text = text.replace(pattern, replacement_fn())
        return text
    
    def _fix_spacing(self, text: str) -> str:
        """Fix common spacing issues."""
        # Remove space before punctuation
        text = re.sub(r'\s+([.,!?;:])', r'\1', text)
        # Ensure space after punctuation
        text = re.sub(r'([.,!?;:])([A-Za-z])', r'\1 \2', text)
        # Collapse multiple spaces
        text = re.sub(r' +', ' ', text)
        return text.strip()
```

**Acceptance Criteria:**
- [ ] Simple replacements work (brb → be right back)
- [ ] Smart replacements expand (@@date → 2026-02-04)
- [ ] Spacing fixes applied correctly
- [ ] Case-insensitive matching
- [ ] Unit tests for all transformations

---

## Phase 3: Tauri Shell (Days 2-5)

### Task 3.1: Sidecar Process Management [M]
**Owner:** Stream A (Rust)
**Dependencies:** Task 1.2, Task 1.3
**Files:**
- `src-tauri/src/sidecar.rs`
- `src-tauri/src/state.rs`

**Implementation (`sidecar.rs`):**
```rust
use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use crate::ipc::IpcClient;

pub struct SidecarManager {
    process: Option<Child>,
    client: Option<IpcClient>,
}

impl SidecarManager {
    pub fn new() -> Self {
        Self { process: None, client: None }
    }
    
    pub async fn start(&mut self, python_path: &str) -> Result<(), String> {
        // Find bundled Python sidecar
        let sidecar_path = Self::find_sidecar_path()?;
        
        let mut child = Command::new(python_path)
            .arg("-m")
            .arg("openvoicy")
            .current_dir(&sidecar_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start sidecar: {}", e))?;
        
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        
        self.client = Some(IpcClient::new(stdin, stdout));
        self.process = Some(child);
        
        Ok(())
    }
    
    pub async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
        self.client
            .as_ref()
            .ok_or("Sidecar not started")?
            .call(method, params)
            .await
    }
    
    pub async fn shutdown(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill().await;
        }
    }
    
    fn find_sidecar_path() -> Result<String, String> {
        // In dev: use ../sidecar
        // In prod: use bundled path from tauri resources
        Ok("../sidecar".to_string())
    }
}
```

**State Management (`state.rs`):**
```rust
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::sidecar::SidecarManager;

#[derive(Clone, Copy, PartialEq)]
pub enum AppStatus {
    Initializing,
    Idle,
    Recording,
    Transcribing,
    Error,
}

pub struct AppState {
    pub sidecar: Arc<Mutex<SidecarManager>>,
    pub status: Arc<Mutex<AppStatus>>,
    pub hotkey_enabled: Arc<Mutex<bool>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sidecar: Arc::new(Mutex::new(SidecarManager::new())),
            status: Arc::new(Mutex::new(AppStatus::Initializing)),
            hotkey_enabled: Arc::new(Mutex::new(true)),
        }
    }
}
```

**Acceptance Criteria:**
- [ ] Sidecar starts successfully
- [ ] JSON-RPC calls work end-to-end
- [ ] Sidecar restarts on crash (stretch goal)
- [ ] Clean shutdown on app exit
- [ ] Bundled correctly in release build

---

### Task 3.2: Global Hotkey System [M]
**Owner:** Stream A (Rust)
**Dependencies:** Task 3.1
**Files:**
- `src-tauri/src/hotkey.rs`
- `src-tauri/src/commands.rs`

**Implementation (`hotkey.rs`):**
```rust
use tauri::{AppHandle, Manager};
use tauri::plugin::global_shortcut::{GlobalShortcut, Shortcut, ShortcutState};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct HotkeyManager {
    shortcut: Option<Shortcut>,
    is_pressed: Arc<Mutex<bool>>,
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            shortcut: None,
            is_pressed: Arc::new(Mutex::new(false)),
        }
    }
    
    pub fn register<R: tauri::Runtime>(
        &mut self, 
        app: &AppHandle<R>,
        accelerator: &str,  // e.g., "CmdOrCtrl+Shift+Space"
    ) -> Result<(), String> {
        let shortcut = app.global_shortcut();
        
        // Parse accelerator
        let parsed: Shortcut = accelerator.parse()
            .map_err(|e| format!("Invalid hotkey: {}", e))?;
        
        let is_pressed = self.is_pressed.clone();
        let app_handle = app.clone();
        
        shortcut.on_shortcut(parsed.clone(), move |_, _, event| {
            let pressed = is_pressed.clone();
            let handle = app_handle.clone();
            
            tauri::async_runtime::spawn(async move {
                match event.state {
                    ShortcutState::Pressed => {
                        let mut p = pressed.lock().await;
                        if !*p {
                            *p = true;
                            // Emit start recording
                            let _ = handle.emit("hotkey-pressed", ());
                        }
                    }
                    ShortcutState::Released => {
                        let mut p = pressed.lock().await;
                        if *p {
                            *p = false;
                            // Emit stop recording
                            let _ = handle.emit("hotkey-released", ());
                        }
                    }
                }
            });
        }).map_err(|e| format!("Failed to register hotkey: {}", e))?;
        
        self.shortcut = Some(parsed);
        Ok(())
    }
    
    pub fn unregister<R: tauri::Runtime>(&mut self, app: &AppHandle<R>) {
        if let Some(shortcut) = self.shortcut.take() {
            let _ = app.global_shortcut().unregister(shortcut);
        }
    }
}
```

**Default Hotkey:** `Ctrl+Shift+Space` (Windows/Linux), `Cmd+Shift+Space` (macOS)

**Acceptance Criteria:**
- [ ] Hotkey registers on all platforms
- [ ] Press triggers `hotkey-pressed` event
- [ ] Release triggers `hotkey-released` event
- [ ] Hotkey can be changed at runtime
- [ ] Conflict detection (stretch goal)

---

### Task 3.3: Text Injection [M]
**Owner:** Stream A (Rust)
**Dependencies:** None
**Files:**
- `src-tauri/src/inject.rs`

**Implementation (`inject.rs`):**
```rust
use enigo::{Enigo, Keyboard, Settings};
use std::thread;
use std::time::Duration;

pub struct TextInjector {
    delay_ms: u64,
}

impl TextInjector {
    pub fn new() -> Self {
        Self { delay_ms: 10 }
    }
    
    pub fn set_delay(&mut self, delay_ms: u64) {
        self.delay_ms = delay_ms;
    }
    
    pub fn inject(&self, text: &str) -> Result<(), String> {
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to create enigo: {}", e))?;
        
        // Small delay to ensure focus is stable
        thread::sleep(Duration::from_millis(50));
        
        // Type the text
        enigo.text(text)
            .map_err(|e| format!("Failed to inject text: {}", e))?;
        
        Ok(())
    }
}

// Alternative: clipboard-based injection for special characters
impl TextInjector {
    pub fn inject_via_clipboard(&self, text: &str) -> Result<(), String> {
        use arboard::Clipboard;
        
        let mut clipboard = Clipboard::new()
            .map_err(|e| format!("Clipboard error: {}", e))?;
        
        // Save current clipboard
        let previous = clipboard.get_text().ok();
        
        // Set our text
        clipboard.set_text(text)
            .map_err(|e| format!("Failed to set clipboard: {}", e))?;
        
        // Simulate Ctrl+V / Cmd+V
        let mut enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Enigo error: {}", e))?;
        
        #[cfg(target_os = "macos")]
        enigo.key(enigo::Key::Meta, enigo::Direction::Press).ok();
        #[cfg(not(target_os = "macos"))]
        enigo.key(enigo::Key::Control, enigo::Direction::Press).ok();
        
        enigo.key(enigo::Key::Unicode('v'), enigo::Direction::Click).ok();
        
        #[cfg(target_os = "macos")]
        enigo.key(enigo::Key::Meta, enigo::Direction::Release).ok();
        #[cfg(not(target_os = "macos"))]
        enigo.key(enigo::Key::Control, enigo::Direction::Release).ok();
        
        // Restore previous clipboard after delay
        thread::sleep(Duration::from_millis(100));
        if let Some(prev) = previous {
            let _ = clipboard.set_text(prev);
        }
        
        Ok(())
    }
}
```

**Acceptance Criteria:**
- [ ] Text injection works on Windows
- [ ] Text injection works on macOS
- [ ] Text injection works on Linux (X11)
- [ ] Unicode characters handled correctly
- [ ] Clipboard fallback for edge cases

---

### Task 3.4: System Tray [M]
**Owner:** Stream A (Rust)
**Dependencies:** Task 3.1
**Files:**
- `src-tauri/src/tray.rs`
- `src-tauri/icons/` (tray icons)

**Implementation (`tray.rs`):**
```rust
use tauri::{
    tray::{TrayIcon, TrayIconBuilder, MouseButton, MouseButtonState},
    menu::{Menu, MenuItem},
    AppHandle, Manager,
};

pub struct TrayManager {
    tray: Option<TrayIcon>,
}

#[derive(Clone, Copy)]
pub enum TrayState {
    Idle,
    Recording,
    Transcribing,
    Error,
}

impl TrayManager {
    pub fn new() -> Self {
        Self { tray: None }
    }
    
    pub fn setup<R: tauri::Runtime>(&mut self, app: &AppHandle<R>) -> Result<(), String> {
        let quit = MenuItem::with_id(app, "quit", "Quit OpenVoicy", true, None::<&str>)
            .map_err(|e| e.to_string())?;
        let settings = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)
            .map_err(|e| e.to_string())?;
        
        let menu = Menu::with_items(app, &[&settings, &quit])
            .map_err(|e| e.to_string())?;
        
        let tray = TrayIconBuilder::new()
            .icon(app.default_window_icon().unwrap().clone())
            .menu(&menu)
            .tooltip("OpenVoicy - Ready")
            .on_menu_event(|app, event| {
                match event.id().as_ref() {
                    "quit" => app.exit(0),
                    "settings" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                }
            })
            .on_tray_icon_event(|tray, event| {
                if let tauri::tray::TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                    let app = tray.app_handle();
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            })
            .build(app)
            .map_err(|e| e.to_string())?;
        
        self.tray = Some(tray);
        Ok(())
    }
    
    pub fn set_state(&self, state: TrayState) {
        if let Some(tray) = &self.tray {
            let (tooltip, icon_name) = match state {
                TrayState::Idle => ("OpenVoicy - Ready", "icon-idle"),
                TrayState::Recording => ("OpenVoicy - Recording...", "icon-recording"),
                TrayState::Transcribing => ("OpenVoicy - Transcribing...", "icon-processing"),
                TrayState::Error => ("OpenVoicy - Error", "icon-error"),
            };
            let _ = tray.set_tooltip(Some(tooltip));
            // Icon switching: load from resources
        }
    }
}
```

**Icons Needed:**
- `icon-idle.png` - Default state (microphone)
- `icon-recording.png` - Red dot or highlighted mic
- `icon-processing.png` - Spinner or processing indicator
- `icon-error.png` - Warning/error state

**Acceptance Criteria:**
- [ ] Tray icon appears on all platforms
- [ ] Right-click shows menu
- [ ] Left-click opens settings window
- [ ] Tooltip updates with status
- [ ] Icon changes based on state (stretch)

---

### Task 3.5: Tauri Commands Integration [M]
**Owner:** Stream D (Integration)
**Dependencies:** Tasks 3.1-3.4
**Files:**
- `src-tauri/src/commands.rs`
- `src-tauri/src/main.rs`
- `src-tauri/src/lib.rs`

**Implementation (`commands.rs`):**
```rust
use tauri::{command, AppHandle, Manager, State, Emitter};
use crate::state::AppState;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone)]
pub struct TranscriptionResult {
    text: String,
    confidence: f32,
    duration_ms: u32,
}

#[derive(Serialize, Clone)]
pub struct DeviceInfo {
    id: i32,
    name: String,
    is_default: bool,
}

#[command]
pub async fn initialize(state: State<'_, AppState>) -> Result<(), String> {
    let sidecar = state.sidecar.lock().await;
    sidecar.call("initialize", serde_json::json!({})).await?;
    Ok(())
}

#[command]
pub async fn list_devices(state: State<'_, AppState>) -> Result<Vec<DeviceInfo>, String> {
    let sidecar = state.sidecar.lock().await;
    let result = sidecar.call("list_devices", serde_json::json!({})).await?;
    
    let devices: Vec<DeviceInfo> = serde_json::from_value(result["devices"].clone())
        .map_err(|e| e.to_string())?;
    Ok(devices)
}

#[command]
pub async fn start_recording(
    state: State<'_, AppState>,
    device_id: Option<i32>,
) -> Result<(), String> {
    let sidecar = state.sidecar.lock().await;
    sidecar.call("start_recording", serde_json::json!({ "device_id": device_id })).await?;
    *state.status.lock().await = crate::state::AppStatus::Recording;
    Ok(())
}

#[command]
pub async fn stop_recording(state: State<'_, AppState>) -> Result<(), String> {
    let sidecar = state.sidecar.lock().await;
    sidecar.call("stop_recording", serde_json::json!({})).await?;
    *state.status.lock().await = crate::state::AppStatus::Transcribing;
    Ok(())
}

#[command]
pub async fn cancel_recording(state: State<'_, AppState>) -> Result<(), String> {
    let sidecar = state.sidecar.lock().await;
    sidecar.call("cancel_recording", serde_json::json!({})).await?;
    *state.status.lock().await = crate::state::AppStatus::Idle;
    Ok(())
}

#[command]
pub async fn set_hotkey(
    app: AppHandle,
    state: State<'_, AppState>,
    accelerator: String,
) -> Result<(), String> {
    // Implementation in hotkey.rs
    Ok(())
}

#[command]
pub async fn set_replacements(
    state: State<'_, AppState>,
    rules: Vec<serde_json::Value>,
) -> Result<(), String> {
    let sidecar = state.sidecar.lock().await;
    sidecar.call("set_replacements", serde_json::json!({ "rules": rules })).await?;
    Ok(())
}

#[command]
pub async fn inject_text(text: String) -> Result<(), String> {
    let injector = crate::inject::TextInjector::new();
    injector.inject(&text)
}
```

**Main Entry Point (`main.rs`):**
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod hotkey;
mod inject;
mod ipc;
mod sidecar;
mod state;
mod tray;

use state::AppState;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let state = AppState::new();
            app.manage(state.clone());
            
            // Setup tray
            let mut tray = tray::TrayManager::new();
            tray.setup(app.handle())?;
            
            // Start sidecar
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = app_handle.state::<AppState>();
                let mut sidecar = state.sidecar.lock().await;
                if let Err(e) = sidecar.start("python3").await {
                    eprintln!("Failed to start sidecar: {}", e);
                }
            });
            
            // Register default hotkey
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut hotkey = hotkey::HotkeyManager::new();
                if let Err(e) = hotkey.register(&app_handle, "CmdOrCtrl+Shift+Space") {
                    eprintln!("Failed to register hotkey: {}", e);
                }
            });
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::initialize,
            commands::list_devices,
            commands::start_recording,
            commands::stop_recording,
            commands::cancel_recording,
            commands::set_hotkey,
            commands::set_replacements,
            commands::inject_text,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Acceptance Criteria:**
- [ ] All commands callable from frontend
- [ ] State updates propagate correctly
- [ ] Events emitted to frontend
- [ ] Error handling returns user-friendly messages

---

## Phase 4: React Frontend (Days 3-6)

### Task 4.1: Frontend Scaffolding [S]
**Owner:** Stream C (Frontend)
**Dependencies:** Task 1.1
**Files:**
- `src/main.tsx`
- `src/App.tsx`
- `src/styles/globals.css`
- `tailwind.config.js`

**Implementation (`App.tsx`):**
```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { StatusIndicator } from "./components/StatusIndicator";
import { SettingsPanel } from "./components/SettingsPanel";

type AppStatus = "initializing" | "idle" | "recording" | "transcribing" | "error";

export default function App() {
  const [status, setStatus] = useState<AppStatus>("initializing");
  const [lastTranscription, setLastTranscription] = useState<string>("");
  const [showSettings, setShowSettings] = useState(false);

  useEffect(() => {
    // Initialize on mount
    invoke("initialize").catch(console.error);

    // Listen for status changes
    const unlistenStatus = listen<{ status: AppStatus }>("status-changed", (event) => {
      setStatus(event.payload.status);
    });

    // Listen for transcription results
    const unlistenTranscription = listen<{ text: string }>("transcription-complete", (event) => {
      setLastTranscription(event.payload.text);
      // Auto-inject text
      invoke("inject_text", { text: event.payload.text });
    });

    // Listen for hotkey events
    const unlistenPressed = listen("hotkey-pressed", () => {
      invoke("start_recording");
    });

    const unlistenReleased = listen("hotkey-released", () => {
      invoke("stop_recording");
    });

    return () => {
      unlistenStatus.then((f) => f());
      unlistenTranscription.then((f) => f());
      unlistenPressed.then((f) => f());
      unlistenReleased.then((f) => f());
    };
  }, []);

  return (
    <div className="min-h-screen bg-gray-900 text-white p-4">
      <header className="flex justify-between items-center mb-6">
        <h1 className="text-xl font-bold">OpenVoicy</h1>
        <button
          onClick={() => setShowSettings(!showSettings)}
          className="p-2 hover:bg-gray-700 rounded"
        >
          ⚙️
        </button>
      </header>

      <StatusIndicator status={status} />

      {lastTranscription && (
        <div className="mt-4 p-3 bg-gray-800 rounded">
          <p className="text-sm text-gray-400">Last transcription:</p>
          <p className="mt-1">{lastTranscription}</p>
        </div>
      )}

      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
    </div>
  );
}
```

**Acceptance Criteria:**
- [ ] App renders without errors
- [ ] Tailwind CSS working
- [ ] Tauri IPC connected
- [ ] Hot reload working in dev

---

### Task 4.2: Status Indicator Component [S]
**Owner:** Stream C (Frontend)
**Dependencies:** Task 4.1
**Files:**
- `src/components/StatusIndicator.tsx`

**Implementation:**
```tsx
import { cn } from "../lib/utils";

type Status = "initializing" | "idle" | "recording" | "transcribing" | "error";

interface StatusIndicatorProps {
  status: Status;
}

const statusConfig: Record<Status, { label: string; color: string; animation?: string }> = {
  initializing: { label: "Loading model...", color: "bg-yellow-500", animation: "animate-pulse" },
  idle: { label: "Ready", color: "bg-green-500" },
  recording: { label: "Recording", color: "bg-red-500", animation: "animate-pulse" },
  transcribing: { label: "Transcribing...", color: "bg-blue-500", animation: "animate-spin" },
  error: { label: "Error", color: "bg-red-700" },
};

export function StatusIndicator({ status }: StatusIndicatorProps) {
  const config = statusConfig[status];

  return (
    <div className="flex items-center gap-3 p-4 bg-gray-800 rounded-lg">
      <div
        className={cn(
          "w-4 h-4 rounded-full",
          config.color,
          config.animation
        )}
      />
      <span className="font-medium">{config.label}</span>
      
      {status === "recording" && (
        <span className="ml-auto text-sm text-gray-400">
          Press hotkey to stop
        </span>
      )}
    </div>
  );
}
```

**Acceptance Criteria:**
- [ ] Shows correct status for all states
- [ ] Animation plays for recording/loading states
- [ ] Accessible (ARIA labels)

---

### Task 4.3: Settings Panel [M]
**Owner:** Stream C (Frontend)
**Dependencies:** Task 4.1
**Files:**
- `src/components/SettingsPanel.tsx`
- `src/components/MicrophoneSelect.tsx`
- `src/components/HotkeyConfig.tsx`
- `src/hooks/useSettings.ts`

**Implementation (`SettingsPanel.tsx`):**
```tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MicrophoneSelect } from "./MicrophoneSelect";
import { HotkeyConfig } from "./HotkeyConfig";
import { ReplacementEditor } from "./ReplacementEditor";

interface SettingsPanelProps {
  onClose: () => void;
}

type Tab = "general" | "hotkey" | "replacements";

export function SettingsPanel({ onClose }: SettingsPanelProps) {
  const [activeTab, setActiveTab] = useState<Tab>("general");

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-gray-800 rounded-lg w-full max-w-lg max-h-[80vh] overflow-hidden">
        <header className="flex justify-between items-center p-4 border-b border-gray-700">
          <h2 className="text-lg font-semibold">Settings</h2>
          <button onClick={onClose} className="text-gray-400 hover:text-white">
            ✕
          </button>
        </header>

        <nav className="flex border-b border-gray-700">
          {(["general", "hotkey", "replacements"] as Tab[]).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`px-4 py-2 capitalize ${
                activeTab === tab ? "bg-gray-700 text-white" : "text-gray-400"
              }`}
            >
              {tab}
            </button>
          ))}
        </nav>

        <div className="p-4 overflow-y-auto max-h-[60vh]">
          {activeTab === "general" && <MicrophoneSelect />}
          {activeTab === "hotkey" && <HotkeyConfig />}
          {activeTab === "replacements" && <ReplacementEditor />}
        </div>
      </div>
    </div>
  );
}
```

**Implementation (`MicrophoneSelect.tsx`):**
```tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Device {
  id: number;
  name: string;
  is_default: boolean;
}

export function MicrophoneSelect() {
  const [devices, setDevices] = useState<Device[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    invoke<Device[]>("list_devices")
      .then((devs) => {
        setDevices(devs);
        const defaultDev = devs.find((d) => d.is_default);
        if (defaultDev) setSelected(defaultDev.id);
      })
      .finally(() => setLoading(false));
  }, []);

  const handleChange = (deviceId: number) => {
    setSelected(deviceId);
    // Save to settings
  };

  if (loading) return <p>Loading devices...</p>;

  return (
    <div>
      <label className="block text-sm font-medium mb-2">Microphone</label>
      <select
        value={selected ?? ""}
        onChange={(e) => handleChange(Number(e.target.value))}
        className="w-full bg-gray-700 border border-gray-600 rounded px-3 py-2"
      >
        {devices.map((device) => (
          <option key={device.id} value={device.id}>
            {device.name} {device.is_default ? "(Default)" : ""}
          </option>
        ))}
      </select>
    </div>
  );
}
```

**Acceptance Criteria:**
- [ ] Tab navigation works
- [ ] Microphone list populates
- [ ] Settings persist (localStorage for MVP)
- [ ] Modal closes on backdrop click

---

### Task 4.4: Replacement Editor [M]
**Owner:** Stream C (Frontend)
**Dependencies:** Task 4.3
**Files:**
- `src/components/ReplacementEditor.tsx`

**Implementation:**
```tsx
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Rule {
  id: string;
  from: string;
  to: string;
}

export function ReplacementEditor() {
  const [rules, setRules] = useState<Rule[]>([]);
  const [newFrom, setNewFrom] = useState("");
  const [newTo, setNewTo] = useState("");

  // Load saved rules
  useEffect(() => {
    const saved = localStorage.getItem("replacementRules");
    if (saved) {
      const parsed = JSON.parse(saved);
      setRules(parsed);
      invoke("set_replacements", { rules: parsed });
    }
  }, []);

  const saveRules = (updatedRules: Rule[]) => {
    setRules(updatedRules);
    localStorage.setItem("replacementRules", JSON.stringify(updatedRules));
    invoke("set_replacements", { rules: updatedRules });
  };

  const addRule = () => {
    if (!newFrom.trim() || !newTo.trim()) return;
    
    const rule: Rule = {
      id: crypto.randomUUID(),
      from: newFrom.trim(),
      to: newTo.trim(),
    };
    
    saveRules([...rules, rule]);
    setNewFrom("");
    setNewTo("");
  };

  const deleteRule = (id: string) => {
    saveRules(rules.filter((r) => r.id !== id));
  };

  return (
    <div>
      <h3 className="font-medium mb-3">Text Replacements</h3>
      <p className="text-sm text-gray-400 mb-4">
        Automatically expand shortcuts in transcriptions.
      </p>

      {/* Existing rules */}
      <div className="space-y-2 mb-4">
        {rules.map((rule) => (
          <div key={rule.id} className="flex items-center gap-2 bg-gray-700 p-2 rounded">
            <span className="font-mono text-sm">{rule.from}</span>
            <span className="text-gray-400">→</span>
            <span className="flex-1">{rule.to}</span>
            <button
              onClick={() => deleteRule(rule.id)}
              className="text-red-400 hover:text-red-300"
            >
              ✕
            </button>
          </div>
        ))}
        {rules.length === 0 && (
          <p className="text-gray-500 text-sm">No replacements configured.</p>
        )}
      </div>

      {/* Add new rule */}
      <div className="flex gap-2">
        <input
          type="text"
          placeholder="brb"
          value={newFrom}
          onChange={(e) => setNewFrom(e.target.value)}
          className="flex-1 bg-gray-700 border border-gray-600 rounded px-2 py-1 text-sm"
        />
        <span className="text-gray-400 self-center">→</span>
        <input
          type="text"
          placeholder="be right back"
          value={newTo}
          onChange={(e) => setNewTo(e.target.value)}
          className="flex-1 bg-gray-700 border border-gray-600 rounded px-2 py-1 text-sm"
        />
        <button
          onClick={addRule}
          className="bg-blue-600 hover:bg-blue-500 px-3 py-1 rounded text-sm"
        >
          Add
        </button>
      </div>

      {/* Smart replacements info */}
      <div className="mt-4 p-3 bg-gray-700/50 rounded text-sm">
        <p className="font-medium mb-1">Smart expansions:</p>
        <ul className="text-gray-400 space-y-1">
          <li><code>@@date</code> → Current date (2026-02-04)</li>
          <li><code>@@time</code> → Current time (14:30)</li>
          <li><code>@@datetime</code> → Date and time</li>
        </ul>
      </div>
    </div>
  );
}
```

**Acceptance Criteria:**
- [ ] Can add new replacement rules
- [ ] Can delete existing rules
- [ ] Rules persist in localStorage
- [ ] Rules sync to Python sidecar
- [ ] Smart replacements documented

---

## Phase 5: Integration & Testing (Days 5-8)

### Task 5.1: End-to-End Flow Integration [L]
**Owner:** Stream D (Integration)
**Dependencies:** All previous tasks
**Files:**
- `src-tauri/src/lib.rs` (event wiring)
- `src/App.tsx` (event handling)

**Flow to Implement:**
```
User presses hotkey
    ↓
Tauri receives global shortcut event
    ↓
Tauri calls sidecar.start_recording()
    ↓
UI shows "Recording" status
    ↓
User releases hotkey
    ↓
Tauri calls sidecar.stop_recording()
    ↓
Python captures audio, runs ASR
    ↓
Python sends transcription_complete notification
    ↓
Tauri receives notification, emits to frontend
    ↓
Frontend calls inject_text command
    ↓
Rust injects text into focused field
    ↓
UI shows "Ready" status
```

**Event Wiring (`lib.rs`):**
```rust
// Setup notification listener from sidecar
pub async fn setup_notification_handler(app: AppHandle, state: AppState) {
    let sidecar = state.sidecar.lock().await;
    
    // Start background task to read notifications
    let app_handle = app.clone();
    tokio::spawn(async move {
        // Read from sidecar stdout and emit events
        // This would be implemented in the IpcClient
    });
}
```

**Acceptance Criteria:**
- [ ] Full flow works end-to-end
- [ ] Latency < 500ms from release to injection
- [ ] No race conditions in state
- [ ] Error states handled gracefully

---

### Task 5.2: Error Handling & Recovery [M]
**Owner:** Stream D (Integration)
**Dependencies:** Task 5.1
**Files:**
- `src-tauri/src/sidecar.rs` (restart logic)
- `src/components/ErrorBoundary.tsx`

**Error Scenarios to Handle:**

| Error | Detection | Recovery |
|-------|-----------|----------|
| Sidecar crash | Process exit | Auto-restart, notify user |
| Model load fail | IPC error | Show error, offer retry |
| Recording fail | IPC error | Cancel, show message |
| Injection fail | enigo error | Copy to clipboard instead |
| Hotkey conflict | Registration fail | Prompt for new hotkey |

**Implementation (sidecar restart):**
```rust
impl SidecarManager {
    pub async fn ensure_running(&mut self) -> Result<(), String> {
        if let Some(ref mut process) = self.process {
            match process.try_wait() {
                Ok(Some(_)) => {
                    // Process exited, restart
                    self.process = None;
                    self.client = None;
                    self.start("python3").await?;
                }
                Ok(None) => {
                    // Still running
                }
                Err(e) => {
                    return Err(format!("Failed to check process: {}", e));
                }
            }
        }
        Ok(())
    }
}
```

**Acceptance Criteria:**
- [ ] Sidecar auto-restarts on crash
- [ ] User sees clear error messages
- [ ] App doesn't freeze on errors
- [ ] Fallback to clipboard works

---

### Task 5.3: Manual Testing & Bug Fixes [M]
**Owner:** All streams
**Dependencies:** Task 5.1, 5.2

**Test Cases:**

| # | Test Case | Steps | Expected Result |
|---|-----------|-------|-----------------|
| 1 | Basic transcription | Hold hotkey, speak "hello world", release | "Hello world" appears in focused field |
| 2 | Text replacement | Configure "brb" → "be right back", speak "brb" | "Be right back" appears |
| 3 | Smart replacement | Speak "at at date" | Current date appears |
| 4 | Cancel recording | Hold hotkey, press Escape | No transcription, returns to idle |
| 5 | Microphone switch | Change mic in settings, record | Audio from new mic used |
| 6 | Long recording | Record for 30 seconds | Full transcription appears |
| 7 | Unicode handling | Speak sentence with accents | Correct characters injected |
| 8 | App restart | Close and reopen app | Settings preserved, hotkey works |
| 9 | System tray | Click tray icon | Window opens/focuses |
| 10 | Error recovery | Kill Python process | App shows error, restarts sidecar |

**Acceptance Criteria:**
- [ ] All 10 test cases pass
- [ ] No crashes during testing
- [ ] Performance acceptable (< 2s total latency)

---

## Phase 6: Packaging & Distribution (Days 7-9)

### Task 6.1: Python Bundling [M]
**Owner:** Stream B (Python)
**Dependencies:** Task 5.3
**Files:**
- `sidecar/build.py`
- `src-tauri/tauri.conf.json` (sidecar config)

**Strategy:** Bundle Python as standalone executable using PyInstaller

```python
# build.py
import PyInstaller.__main__

PyInstaller.__main__.run([
    'openvoicy/__main__.py',
    '--name=openvoicy-sidecar',
    '--onefile',
    '--hidden-import=sounddevice',
    '--hidden-import=nemo_toolkit',
    # Add all NeMo dependencies
])
```

**Alternative:** Use `uv` to create standalone Python environment

**Acceptance Criteria:**
- [ ] Single executable for sidecar
- [ ] Works without system Python
- [ ] Model downloads on first run
- [ ] Size < 500MB (excluding model)

---

### Task 6.2: Tauri Bundling [M]
**Owner:** Stream A (Rust)
**Dependencies:** Task 6.1
**Files:**
- `src-tauri/tauri.conf.json`
- `.github/workflows/build.yml`

**tauri.conf.json updates:**
```json
{
  "bundle": {
    "identifier": "com.openvoicy.app",
    "icon": ["icons/icon.png"],
    "resources": ["../sidecar/dist/*"],
    "externalBin": ["sidecar/openvoicy-sidecar"]
  }
}
```

**Acceptance Criteria:**
- [ ] Windows .msi installer works
- [ ] macOS .dmg works
- [ ] Linux .AppImage works
- [ ] App size < 100MB (excluding model)

---

## Critical Path

```
Day 1:  [1.1] Scaffolding ─────────────────────────────┐
        [1.2] IPC Protocol ────────────────────────────┤
        [1.3] Python Entry Point ──────────────────────┤
                                                       │
Day 2:  [2.1] Audio Capture ───────────────────────────┼──► Integration begins
        [3.1] Sidecar Management ──────────────────────┤
        [4.1] Frontend Scaffolding ────────────────────┘
                                                       
Day 3:  [2.2] ASR Engine (CRITICAL PATH) ──────────────┐
        [3.2] Hotkey System ───────────────────────────┤
        [3.3] Text Injection ──────────────────────────┤
        [4.2] Status Indicator ────────────────────────┘
                                                       
Day 4:  [2.2] ASR Engine (continued) ──────────────────┐
        [3.4] System Tray ─────────────────────────────┤
        [4.3] Settings Panel ──────────────────────────┘
                                                       
Day 5:  [2.3] Text Processor ──────────────────────────┐
        [3.5] Tauri Commands ──────────────────────────┤
        [4.4] Replacement Editor ──────────────────────┤
        [5.1] E2E Integration (START) ─────────────────┘
                                                       
Day 6:  [5.1] E2E Integration (COMPLETE) ──────────────┐
        [5.2] Error Handling ──────────────────────────┘
                                                       
Day 7:  [5.3] Manual Testing ──────────────────────────┐
        [6.1] Python Bundling ─────────────────────────┘
                                                       
Day 8:  [5.3] Bug Fixes ───────────────────────────────┐
        [6.2] Tauri Bundling ──────────────────────────┘
                                                       
Day 9:  Final testing & release ───────────────────────►
```

**Critical Path:** 2.2 (ASR Engine) → 5.1 (Integration) → 5.3 (Testing)

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| NeMo installation issues | High | High | Document exact versions; provide Docker fallback |
| Text injection fails on some apps | Medium | Medium | Implement clipboard fallback |
| Model download bandwidth | Medium | Low | Progressive download; cache in user directory |
| Hotkey conflicts | Medium | Low | Detection + user configuration |
| Audio latency | Low | Medium | Buffer tuning; stream directly to model |
| macOS permissions | High | Medium | Clear onboarding flow for mic/accessibility |

---

## Parallel Work Allocation

**Stream A (Rust/Tauri):** Tasks 3.1, 3.2, 3.3, 3.4, 6.2
**Stream B (Python/ASR):** Tasks 2.1, 2.2, 2.3, 6.1
**Stream C (React/Frontend):** Tasks 4.1, 4.2, 4.3, 4.4
**Stream D (Integration):** Tasks 1.2, 3.5, 5.1, 5.2, 5.3

---

## Success Metrics (MVP)

- [ ] **Functional:** User can record speech and see text injected
- [ ] **Accuracy:** >95% WER on clear English speech
- [ ] **Latency:** <2 seconds from hotkey release to text injection
- [ ] **Reliability:** No crashes during 1-hour usage session
- [ ] **Size:** Installer <100MB (model downloaded separately)

---

*Plan version: 1.0 | Last updated: 2026-02-04*
