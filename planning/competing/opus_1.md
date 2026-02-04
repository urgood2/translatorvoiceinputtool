# OpenVoicy - Detailed Implementation Plan

**Version:** 0.1.0 (MVP)
**Date:** 2026-02-04
**Status:** Ready for Implementation

---

## 1. Executive Summary (TL;DR)

OpenVoicy is a cross-platform, offline speech-to-text tool that transcribes voice input and injects it into any focused text field. The MVP delivers:

- **Push-to-talk recording** via global hotkey
- **Offline transcription** using NVIDIA Parakeet V3 (0.6B params)
- **Direct text injection** into any application
- **Text replacement engine** for snippets and smart expansions
- **System tray** integration with status indicators
- **Settings UI** for configuration

**Architecture:** Tauri (Rust) shell + React/TypeScript UI + Python sidecar for ML inference.

**Target Platforms:** Windows 10+, macOS 12+, Linux (X11/Wayland)

---

## 2. Architecture Overview

```
┌──────────────────────────────────────────────────────────────────────────┐
│                           OpenVoicy Application                          │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │                     Tauri Shell (Rust)                             │  │
│  │  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌───────────┐  │  │
│  │  │ Hotkey Mgr   │ │ System Tray  │ │ Text Inject  │ │ IPC Bridge│  │  │
│  │  │ (rdev crate) │ │ (tauri-tray) │ │ (enigo)      │ │ (sidecar) │  │  │
│  │  └──────────────┘ └──────────────┘ └──────────────┘ └───────────┘  │  │
│  └────────────────────────────────────────────────────────────────────┘  │
│                              ▲                                           │
│                              │ Tauri Commands (invoke)                   │
│                              ▼                                           │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │                   Web UI (React + TypeScript)                      │  │
│  │  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌───────────┐  │  │
│  │  │ Settings     │ │ Replacements │ │ Status View  │ │ History   │  │  │
│  │  │ Panel        │ │ Manager      │ │ (recording)  │ │ View      │  │  │
│  │  └──────────────┘ └──────────────┘ └──────────────┘ └───────────┘  │  │
│  └────────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐  │
│  │                     Python Sidecar                                 │  │
│  │  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌───────────┐  │  │
│  │  │ Audio        │ │ ASR Engine   │ │ Text Post-   │ │ JSON-RPC  │  │  │
│  │  │ Capture      │ │ (Parakeet)   │ │ Processor    │ │ Server    │  │  │
│  │  │ (sounddevice)│ │ (NeMo/MLX)   │ │              │ │ (stdio)   │  │  │
│  │  └──────────────┘ └──────────────┘ └──────────────┘ └───────────┘  │  │
│  └────────────────────────────────────────────────────────────────────┘  │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

### Data Flow

```
User holds hotkey → Tauri registers keydown → Signals Python sidecar "start_recording"
                                                        │
                                                        ▼
                                              Audio capture begins
                                              (sounddevice streams to buffer)
                                                        │
User releases hotkey → Tauri registers keyup → Signals Python "stop_recording"
                                                        │
                                                        ▼
                                              Parakeet V3 inference on buffer
                                                        │
                                                        ▼
                                              Text post-processing + replacements
                                                        │
                                                        ▼
                                              Returns transcribed text via JSON-RPC
                                                        │
                    Tauri receives text ◄───────────────┘
                            │
                            ▼
                    enigo injects text into focused field
                            │
                            ▼
                    UI updated (history, status)
```

---

## 3. Project Structure

```
openvoicy/
├── src-tauri/                    # Rust/Tauri backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs              # Entry point, app setup
│   │   ├── lib.rs               # Module exports
│   │   ├── hotkey.rs            # Global hotkey management
│   │   ├── tray.rs              # System tray logic
│   │   ├── inject.rs            # Text injection via enigo
│   │   ├── sidecar.rs           # Python process management
│   │   ├── ipc.rs               # JSON-RPC message handling
│   │   ├── config.rs            # Settings persistence
│   │   └── commands.rs          # Tauri command handlers
│   └── icons/                   # App icons
│
├── src/                         # React frontend
│   ├── main.tsx                 # React entry point
│   ├── App.tsx                  # Root component
│   ├── components/
│   │   ├── Settings/
│   │   │   ├── SettingsPanel.tsx
│   │   │   ├── MicrophoneSelect.tsx
│   │   │   ├── HotkeyConfig.tsx
│   │   │   └── GeneralSettings.tsx
│   │   ├── Replacements/
│   │   │   ├── ReplacementList.tsx
│   │   │   ├── ReplacementEditor.tsx
│   │   │   └── SmartExpansions.tsx
│   │   ├── Status/
│   │   │   ├── RecordingIndicator.tsx
│   │   │   └── StatusBar.tsx
│   │   └── History/
│   │       └── HistoryView.tsx
│   ├── hooks/
│   │   ├── useTauriCommand.ts
│   │   ├── useRecordingState.ts
│   │   └── useSettings.ts
│   ├── stores/
│   │   └── appStore.ts          # Zustand store
│   ├── types/
│   │   └── index.ts
│   └── styles/
│       └── globals.css          # Tailwind imports
│
├── sidecar/                     # Python ML backend
│   ├── pyproject.toml           # uv/poetry project
│   ├── src/
│   │   └── openvoicy_sidecar/
│   │       ├── __init__.py
│   │       ├── __main__.py      # Entry point
│   │       ├── server.py        # JSON-RPC stdio server
│   │       ├── audio.py         # Audio capture
│   │       ├── asr.py           # Parakeet inference
│   │       ├── postprocess.py   # Text cleanup
│   │       └── replacements.py  # Replacement engine
│   └── tests/
│
├── package.json                 # Node deps (Vite, React)
├── vite.config.ts
├── tailwind.config.js
├── tsconfig.json
└── README.md
```

---

## 4. Phase 1 Implementation Tasks

### Phase 1A: Foundation (Parallel Track - Infrastructure)

These tasks have no dependencies and can be executed in parallel by separate agents.

---

#### Task 1A.1: Tauri Project Scaffold
**Complexity:** S (Small)
**Assignable:** Yes (independent)

**Description:** Initialize Tauri 2.x project with React frontend template.

**Files to Create:**
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`
- `src-tauri/src/main.rs`
- `src-tauri/src/lib.rs`
- `package.json`
- `vite.config.ts`
- `tsconfig.json`
- `tailwind.config.js`
- `src/main.tsx`
- `src/App.tsx`

**Implementation Details:**
```rust
// src-tauri/Cargo.toml dependencies
[dependencies]
tauri = { version = "2", features = ["tray-icon", "shell-sidecar"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
enigo = "0.2"
rdev = "0.5"
```

**Acceptance Criteria:**
- [ ] `npm run tauri dev` launches app with empty React window
- [ ] Tauri commands can be invoked from React
- [ ] Hot reload works for both Rust and React
- [ ] Tailwind CSS classes render correctly

---

#### Task 1A.2: Python Sidecar Scaffold
**Complexity:** S (Small)
**Assignable:** Yes (independent)

**Description:** Create Python project structure with uv, basic JSON-RPC server.

**Files to Create:**
- `sidecar/pyproject.toml`
- `sidecar/src/openvoicy_sidecar/__init__.py`
- `sidecar/src/openvoicy_sidecar/__main__.py`
- `sidecar/src/openvoicy_sidecar/server.py`

**Implementation Details:**
```python
# server.py - Basic JSON-RPC over stdio
import sys
import json

def handle_request(request: dict) -> dict:
    method = request.get("method")
    params = request.get("params", {})
    request_id = request.get("id")
    
    handlers = {
        "ping": lambda p: {"pong": True},
        "get_microphones": get_microphones,
        "start_recording": start_recording,
        "stop_recording": stop_recording,
    }
    
    if method in handlers:
        result = handlers[method](params)
        return {"jsonrpc": "2.0", "result": result, "id": request_id}
    else:
        return {"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found"}, "id": request_id}

def main():
    for line in sys.stdin:
        request = json.loads(line)
        response = handle_request(request)
        sys.stdout.write(json.dumps(response) + "\n")
        sys.stdout.flush()
```

**Acceptance Criteria:**
- [ ] `uv run python -m openvoicy_sidecar` starts and responds to ping
- [ ] JSON-RPC messages flow correctly via stdio
- [ ] Error responses follow JSON-RPC 2.0 spec
- [ ] Process exits cleanly on EOF

---

#### Task 1A.3: IPC Bridge (Tauri ↔ Python)
**Complexity:** M (Medium)
**Assignable:** Yes (after 1A.1 and 1A.2)
**Dependencies:** 1A.1, 1A.2

**Description:** Implement Tauri sidecar management and JSON-RPC communication.

**Files to Create/Modify:**
- `src-tauri/src/sidecar.rs`
- `src-tauri/src/ipc.rs`
- `src-tauri/src/commands.rs` (add sidecar commands)

**Implementation Details:**
```rust
// sidecar.rs
use tauri::api::process::{Command, CommandEvent};
use tokio::sync::mpsc;

pub struct SidecarManager {
    child: Option<CommandChild>,
    sender: mpsc::Sender<String>,
    receiver: mpsc::Receiver<String>,
}

impl SidecarManager {
    pub async fn spawn() -> Result<Self, Error> {
        let (mut rx, child) = Command::new_sidecar("openvoicy-sidecar")?
            .spawn()?;
        // ... setup channels
    }
    
    pub async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, Error> {
        let request = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": uuid::Uuid::new_v4().to_string()
        });
        // Send to stdin, await response from stdout
    }
}
```

**Acceptance Criteria:**
- [ ] Sidecar spawns automatically on app start
- [ ] `invoke("sidecar_call", {method: "ping"})` returns `{pong: true}`
- [ ] Sidecar restarts automatically if it crashes
- [ ] Clean shutdown: sidecar terminates when app closes
- [ ] Timeout handling for unresponsive sidecar (5s default)

---

#### Task 1A.4: Configuration System
**Complexity:** S (Small)
**Assignable:** Yes (independent)

**Description:** Implement settings persistence using Tauri's app data directory.

**Files to Create:**
- `src-tauri/src/config.rs`
- `src/stores/appStore.ts`
- `src/types/index.ts`

**Data Structures:**
```typescript
// src/types/index.ts
interface AppConfig {
  audio: {
    inputDeviceId: string | null;
    sampleRate: 16000;
  };
  hotkey: {
    modifier: "ctrl" | "alt" | "meta" | "shift";
    key: string; // e.g., "Space"
  };
  injection: {
    delayMs: number; // 0-100ms
  };
  replacements: Replacement[];
}

interface Replacement {
  id: string;
  trigger: string;        // e.g., "brb"
  replacement: string;    // e.g., "be right back"
  type: "snippet" | "smart";
  enabled: boolean;
}
```

```rust
// config.rs
#[derive(Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub audio: AudioConfig,
    pub hotkey: HotkeyConfig,
    pub injection: InjectionConfig,
    pub replacements: Vec<Replacement>,
}

impl AppConfig {
    pub fn load(app_handle: &AppHandle) -> Result<Self, Error> {
        let path = app_handle.path_resolver()
            .app_config_dir()?
            .join("config.json");
        // ...
    }
    
    pub fn save(&self, app_handle: &AppHandle) -> Result<(), Error> {
        // Atomic write with temp file
    }
}
```

**Acceptance Criteria:**
- [ ] Config loads on app start, creates default if missing
- [ ] Config saves atomically (no corruption on crash)
- [ ] React store syncs with Rust config via Tauri commands
- [ ] Config file location: `~/.config/openvoicy/config.json` (Linux), `~/Library/Application Support/openvoicy/` (macOS), `%APPDATA%\openvoicy\` (Windows)

---

### Phase 1B: Audio Pipeline (Sequential - Python Sidecar)

These tasks must be done sequentially as they build upon each other.

---

#### Task 1B.1: Audio Device Enumeration
**Complexity:** S (Small)
**Assignable:** Yes (after 1A.2)
**Dependencies:** 1A.2

**Description:** List available audio input devices.

**Files to Create/Modify:**
- `sidecar/src/openvoicy_sidecar/audio.py`

**Implementation Details:**
```python
# audio.py
import sounddevice as sd
from dataclasses import dataclass

@dataclass
class AudioDevice:
    id: int
    name: str
    channels: int
    sample_rate: float
    is_default: bool

def get_input_devices() -> list[AudioDevice]:
    devices = sd.query_devices()
    default_input = sd.default.device[0]
    
    return [
        AudioDevice(
            id=i,
            name=d["name"],
            channels=d["max_input_channels"],
            sample_rate=d["default_samplerate"],
            is_default=(i == default_input)
        )
        for i, d in enumerate(devices)
        if d["max_input_channels"] > 0
    ]
```

**Acceptance Criteria:**
- [ ] Returns list of input devices with correct metadata
- [ ] Identifies default device correctly
- [ ] Handles systems with no input devices (returns empty list)
- [ ] Works on Windows, macOS, Linux

---

#### Task 1B.2: Audio Capture
**Complexity:** M (Medium)
**Assignable:** Yes (after 1B.1)
**Dependencies:** 1B.1

**Description:** Implement push-to-talk audio recording.

**Files to Modify:**
- `sidecar/src/openvoicy_sidecar/audio.py`

**Implementation Details:**
```python
# audio.py
import numpy as np
import sounddevice as sd
from threading import Event
from queue import Queue

class AudioRecorder:
    def __init__(self, device_id: int | None = None, sample_rate: int = 16000):
        self.device_id = device_id
        self.sample_rate = sample_rate
        self.buffer: list[np.ndarray] = []
        self._recording = Event()
        self._stream: sd.InputStream | None = None
    
    def start(self):
        """Begin recording audio."""
        self.buffer.clear()
        self._recording.set()
        
        def callback(indata, frames, time, status):
            if self._recording.is_set():
                self.buffer.append(indata.copy())
        
        self._stream = sd.InputStream(
            device=self.device_id,
            samplerate=self.sample_rate,
            channels=1,
            dtype=np.float32,
            callback=callback
        )
        self._stream.start()
    
    def stop(self) -> np.ndarray:
        """Stop recording and return audio buffer."""
        self._recording.clear()
        if self._stream:
            self._stream.stop()
            self._stream.close()
            self._stream = None
        
        if not self.buffer:
            return np.array([], dtype=np.float32)
        
        return np.concatenate(self.buffer)
```

**Acceptance Criteria:**
- [ ] `start()` begins capturing audio immediately
- [ ] `stop()` returns numpy array of recorded audio
- [ ] Audio is mono, 16kHz, float32 (Parakeet's expected format)
- [ ] No audio data lost at start/stop boundaries
- [ ] Handles device disconnection gracefully
- [ ] Memory usage stays bounded (max 5 minutes = ~10MB)

---

#### Task 1B.3: Parakeet Model Loading
**Complexity:** L (Large)
**Assignable:** Yes (after 1A.2)
**Dependencies:** 1A.2

**Description:** Load and initialize Parakeet V3 model with GPU/CPU fallback.

**Files to Create:**
- `sidecar/src/openvoicy_sidecar/asr.py`

**Implementation Details:**
```python
# asr.py
import torch
import nemo.collections.asr as nemo_asr
from pathlib import Path
import logging

logger = logging.getLogger(__name__)

class ParakeetASR:
    MODEL_NAME = "nvidia/parakeet-tdt-0.6b-v2"
    
    def __init__(self, device: str | None = None):
        self.device = device or self._detect_device()
        self.model = None
        self._loaded = False
    
    def _detect_device(self) -> str:
        if torch.cuda.is_available():
            logger.info("CUDA available, using GPU")
            return "cuda"
        elif hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            logger.info("MPS available, using Apple Silicon GPU")
            return "mps"
        else:
            logger.info("No GPU available, using CPU")
            return "cpu"
    
    def load(self):
        """Load model (call once at startup)."""
        if self._loaded:
            return
        
        logger.info(f"Loading Parakeet model on {self.device}...")
        self.model = nemo_asr.models.ASRModel.from_pretrained(
            model_name=self.MODEL_NAME
        )
        self.model = self.model.to(self.device)
        self.model.eval()
        self._loaded = True
        logger.info("Model loaded successfully")
    
    def transcribe(self, audio: np.ndarray) -> str:
        """Transcribe audio array to text."""
        if not self._loaded:
            raise RuntimeError("Model not loaded. Call load() first.")
        
        with torch.no_grad():
            # Parakeet expects audio as a list of file paths or numpy arrays
            transcription = self.model.transcribe([audio])
        
        return transcription[0] if transcription else ""
```

**Edge Cases & Error Handling:**
- Empty audio buffer → return empty string
- Audio too short (<0.1s) → return empty string
- Model download failure → raise with helpful message
- OOM on GPU → fall back to CPU with warning

**Acceptance Criteria:**
- [ ] Model downloads automatically on first run (~1.2GB)
- [ ] CUDA acceleration works on NVIDIA GPUs
- [ ] MPS acceleration works on Apple Silicon
- [ ] CPU fallback works (slower but functional)
- [ ] Transcription latency <2s for 10s audio on GPU
- [ ] Model stays loaded between transcriptions (no reload)

---

#### Task 1B.4: Text Post-Processing
**Complexity:** S (Small)
**Assignable:** Yes (independent)
**Dependencies:** None

**Description:** Clean up transcribed text (punctuation, capitalization, formatting).

**Files to Create:**
- `sidecar/src/openvoicy_sidecar/postprocess.py`

**Implementation Details:**
```python
# postprocess.py
import re

def postprocess_transcription(text: str) -> str:
    """Clean up raw transcription output."""
    if not text:
        return ""
    
    # Parakeet usually handles punctuation, but ensure basics
    text = text.strip()
    
    # Ensure first letter is capitalized
    if text and text[0].islower():
        text = text[0].upper() + text[1:]
    
    # Normalize whitespace
    text = re.sub(r'\s+', ' ', text)
    
    # Remove duplicate punctuation
    text = re.sub(r'([.!?])\1+', r'\1', text)
    
    return text
```

**Acceptance Criteria:**
- [ ] Capitalizes first letter of transcription
- [ ] Normalizes multiple spaces to single space
- [ ] Handles empty/whitespace-only input
- [ ] Preserves intentional punctuation from model

---

#### Task 1B.5: Replacement Engine
**Complexity:** M (Medium)
**Assignable:** Yes (independent)
**Dependencies:** None

**Description:** Implement text replacement/expansion system.

**Files to Create:**
- `sidecar/src/openvoicy_sidecar/replacements.py`

**Implementation Details:**
```python
# replacements.py
from dataclasses import dataclass
from datetime import datetime
import re

@dataclass
class Replacement:
    trigger: str
    replacement: str
    type: str  # "snippet" or "smart"
    case_sensitive: bool = False
    whole_word: bool = True

class ReplacementEngine:
    SMART_EXPANSIONS = {
        "@@date": lambda: datetime.now().strftime("%Y-%m-%d"),
        "@@time": lambda: datetime.now().strftime("%H:%M"),
        "@@datetime": lambda: datetime.now().strftime("%Y-%m-%d %H:%M"),
        "@@email": lambda: "",  # Configured per-user
    }
    
    def __init__(self, replacements: list[Replacement] = None):
        self.replacements = replacements or []
        self.user_email = ""
    
    def apply(self, text: str) -> str:
        """Apply all replacements to text."""
        # First, apply smart expansions
        for trigger, expander in self.SMART_EXPANSIONS.items():
            if trigger == "@@email":
                text = text.replace(trigger, self.user_email)
            else:
                text = text.replace(trigger, expander())
        
        # Then, apply user-defined replacements
        for r in self.replacements:
            if r.type == "snippet":
                text = self._apply_snippet(text, r)
        
        return text
    
    def _apply_snippet(self, text: str, r: Replacement) -> str:
        if r.whole_word:
            pattern = r'\b' + re.escape(r.trigger) + r'\b'
            flags = 0 if r.case_sensitive else re.IGNORECASE
            return re.sub(pattern, r.replacement, text, flags=flags)
        else:
            if r.case_sensitive:
                return text.replace(r.trigger, r.replacement)
            else:
                return re.sub(re.escape(r.trigger), r.replacement, text, flags=re.IGNORECASE)
```

**Acceptance Criteria:**
- [ ] Snippet replacement: "brb" → "be right back"
- [ ] Smart expansion: "@@date" → "2026-02-04"
- [ ] Case-insensitive matching by default
- [ ] Whole-word matching prevents "abroad" → "abe right backoad"
- [ ] Multiple replacements in single text
- [ ] Order-independent (no cascade effects)

---

### Phase 1C: Desktop Integration (Parallel Track - Rust)

---

#### Task 1C.1: Global Hotkey System
**Complexity:** M (Medium)
**Assignable:** Yes (after 1A.1)
**Dependencies:** 1A.1

**Description:** Register and handle global hotkeys for push-to-talk.

**Files to Create:**
- `src-tauri/src/hotkey.rs`

**Implementation Details:**
```rust
// hotkey.rs
use rdev::{listen, Event, EventType, Key};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct HotkeyManager {
    modifier: Key,
    key: Key,
    is_pressed: Arc<AtomicBool>,
    tx: mpsc::Sender<HotkeyEvent>,
}

pub enum HotkeyEvent {
    Pressed,
    Released,
}

impl HotkeyManager {
    pub fn new(modifier: Key, key: Key) -> (Self, mpsc::Receiver<HotkeyEvent>) {
        let (tx, rx) = mpsc::channel(32);
        let is_pressed = Arc::new(AtomicBool::new(false));
        
        (Self { modifier, key, is_pressed, tx }, rx)
    }
    
    pub fn start(&self) {
        let modifier = self.modifier;
        let key = self.key;
        let is_pressed = self.is_pressed.clone();
        let tx = self.tx.clone();
        
        std::thread::spawn(move || {
            let mut modifier_down = false;
            
            listen(move |event: Event| {
                match event.event_type {
                    EventType::KeyPress(k) if k == modifier => {
                        modifier_down = true;
                    }
                    EventType::KeyRelease(k) if k == modifier => {
                        modifier_down = false;
                        if is_pressed.swap(false, Ordering::SeqCst) {
                            let _ = tx.blocking_send(HotkeyEvent::Released);
                        }
                    }
                    EventType::KeyPress(k) if k == key && modifier_down => {
                        if !is_pressed.swap(true, Ordering::SeqCst) {
                            let _ = tx.blocking_send(HotkeyEvent::Pressed);
                        }
                    }
                    EventType::KeyRelease(k) if k == key => {
                        if is_pressed.swap(false, Ordering::SeqCst) {
                            let _ = tx.blocking_send(HotkeyEvent::Released);
                        }
                    }
                    _ => {}
                }
            }).unwrap();
        });
    }
}
```

**Platform-Specific Notes:**
- **macOS:** Requires Accessibility permission (prompt user)
- **Linux:** Works on X11; Wayland needs `libxdo` or portal
- **Windows:** Works out of box

**Acceptance Criteria:**
- [ ] Ctrl+Space (default) triggers recording
- [ ] Hold to record, release to stop
- [ ] Hotkey works when app is not focused
- [ ] Configurable modifier (Ctrl/Alt/Meta/Shift)
- [ ] Configurable key
- [ ] Conflict detection (warn if system hotkey)
- [ ] macOS: Prompts for Accessibility permission

---

#### Task 1C.2: Text Injection
**Complexity:** M (Medium)
**Assignable:** Yes (after 1A.1)
**Dependencies:** 1A.1

**Description:** Inject transcribed text into focused text field.

**Files to Create:**
- `src-tauri/src/inject.rs`

**Implementation Details:**
```rust
// inject.rs
use enigo::{Enigo, Key, KeyboardControllable};
use std::time::Duration;
use tokio::time::sleep;

pub struct TextInjector {
    enigo: Enigo,
    delay_ms: u64,
}

impl TextInjector {
    pub fn new(delay_ms: u64) -> Self {
        Self {
            enigo: Enigo::new(),
            delay_ms,
        }
    }
    
    pub async fn inject(&mut self, text: &str) -> Result<(), Error> {
        // Small delay to ensure focus is stable
        if self.delay_ms > 0 {
            sleep(Duration::from_millis(self.delay_ms)).await;
        }
        
        // Use clipboard for complex text (Unicode, special chars)
        // Fall back to key-by-key for simple ASCII
        if text.is_ascii() && text.len() < 100 {
            self.enigo.key_sequence(text);
        } else {
            self.inject_via_clipboard(text)?;
        }
        
        Ok(())
    }
    
    fn inject_via_clipboard(&mut self, text: &str) -> Result<(), Error> {
        // Save current clipboard
        let previous = clipboard::get_contents()?;
        
        // Set text to clipboard
        clipboard::set_contents(text)?;
        
        // Paste
        #[cfg(target_os = "macos")]
        {
            self.enigo.key_down(Key::Meta);
            self.enigo.key_click(Key::Layout('v'));
            self.enigo.key_up(Key::Meta);
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.enigo.key_down(Key::Control);
            self.enigo.key_click(Key::Layout('v'));
            self.enigo.key_up(Key::Control);
        }
        
        // Restore clipboard (optional, configurable)
        clipboard::set_contents(&previous)?;
        
        Ok(())
    }
}
```

**Acceptance Criteria:**
- [ ] ASCII text injects correctly
- [ ] Unicode text (emoji, accented chars) injects correctly
- [ ] Works in all applications (browsers, editors, terminals)
- [ ] Configurable delay (0-100ms)
- [ ] Preserves original clipboard (optional setting)
- [ ] Handles empty text (no-op)

---

#### Task 1C.3: System Tray
**Complexity:** S (Small)
**Assignable:** Yes (after 1A.1)
**Dependencies:** 1A.1

**Description:** Implement system tray icon with status and menu.

**Files to Create:**
- `src-tauri/src/tray.rs`
- `src-tauri/icons/tray-idle.png`
- `src-tauri/icons/tray-recording.png`

**Implementation Details:**
```rust
// tray.rs
use tauri::{
    AppHandle, CustomMenuItem, SystemTray, SystemTrayEvent, SystemTrayMenu,
};

pub fn create_tray() -> SystemTray {
    let menu = SystemTrayMenu::new()
        .add_item(CustomMenuItem::new("show", "Show OpenVoicy"))
        .add_native_item(tauri::SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new("settings", "Settings..."))
        .add_native_item(tauri::SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new("quit", "Quit"));
    
    SystemTray::new()
        .with_menu(menu)
        .with_tooltip("OpenVoicy - Ready")
}

pub fn update_tray_icon(app: &AppHandle, recording: bool) {
    let icon = if recording {
        include_bytes!("../icons/tray-recording.png").to_vec()
    } else {
        include_bytes!("../icons/tray-idle.png").to_vec()
    };
    
    app.tray_handle().set_icon(tauri::Icon::Raw(icon)).unwrap();
    
    let tooltip = if recording {
        "OpenVoicy - Recording..."
    } else {
        "OpenVoicy - Ready"
    };
    app.tray_handle().set_tooltip(tooltip).unwrap();
}
```

**Icon Design:**
- Idle: Microphone outline (gray/white)
- Recording: Microphone filled (red/orange pulse effect)
- Size: 22x22 (macOS), 16x16 (Windows), 24x24 (Linux)

**Acceptance Criteria:**
- [ ] Tray icon visible on all platforms
- [ ] Icon changes when recording starts/stops
- [ ] Tooltip shows current status
- [ ] Menu items work: Show, Settings, Quit
- [ ] Double-click opens main window (Windows/Linux)
- [ ] Single-click opens menu (macOS behavior)

---

### Phase 1D: User Interface (Parallel Track - React)

---

#### Task 1D.1: Status Display Component
**Complexity:** S (Small)
**Assignable:** Yes (after 1A.1)
**Dependencies:** 1A.1

**Description:** Visual indicator showing recording state in main window.

**Files to Create:**
- `src/components/Status/RecordingIndicator.tsx`
- `src/components/Status/StatusBar.tsx`
- `src/hooks/useRecordingState.ts`

**Implementation Details:**
```tsx
// RecordingIndicator.tsx
import { useRecordingState } from "@/hooks/useRecordingState";

export function RecordingIndicator() {
  const { isRecording, lastTranscription } = useRecordingState();
  
  return (
    <div className="flex flex-col items-center gap-4 p-8">
      <div
        className={`
          w-24 h-24 rounded-full flex items-center justify-center
          transition-all duration-200
          ${isRecording 
            ? "bg-red-500 animate-pulse shadow-lg shadow-red-500/50" 
            : "bg-gray-200 dark:bg-gray-700"
          }
        `}
      >
        <MicrophoneIcon 
          className={`w-12 h-12 ${isRecording ? "text-white" : "text-gray-500"}`} 
        />
      </div>
      
      <p className="text-sm text-gray-500">
        {isRecording ? "Recording... Release to transcribe" : "Press Ctrl+Space to record"}
      </p>
      
      {lastTranscription && (
        <div className="mt-4 p-4 bg-gray-100 dark:bg-gray-800 rounded-lg max-w-md">
          <p className="text-sm text-gray-600 dark:text-gray-300">
            {lastTranscription}
          </p>
        </div>
      )}
    </div>
  );
}
```

**Acceptance Criteria:**
- [ ] Shows idle state with hotkey hint
- [ ] Animates when recording (pulse effect)
- [ ] Displays last transcription after completion
- [ ] Responsive to window size
- [ ] Accessible (screen reader friendly)

---

#### Task 1D.2: Settings Panel
**Complexity:** M (Medium)
**Assignable:** Yes (after 1A.4)
**Dependencies:** 1A.4

**Description:** Settings UI for microphone, hotkey, and general options.

**Files to Create:**
- `src/components/Settings/SettingsPanel.tsx`
- `src/components/Settings/MicrophoneSelect.tsx`
- `src/components/Settings/HotkeyConfig.tsx`
- `src/components/Settings/GeneralSettings.tsx`

**Implementation Details:**
```tsx
// MicrophoneSelect.tsx
import { invoke } from "@tauri-apps/api/tauri";
import { useEffect, useState } from "react";

interface AudioDevice {
  id: number;
  name: string;
  isDefault: boolean;
}

export function MicrophoneSelect() {
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  
  useEffect(() => {
    invoke<AudioDevice[]>("get_audio_devices").then(setDevices);
  }, []);
  
  const handleChange = async (deviceId: number) => {
    await invoke("set_audio_device", { deviceId });
    setSelected(deviceId);
  };
  
  return (
    <div className="space-y-2">
      <label className="text-sm font-medium">Microphone</label>
      <select 
        value={selected ?? ""} 
        onChange={(e) => handleChange(Number(e.target.value))}
        className="w-full p-2 border rounded"
      >
        {devices.map((d) => (
          <option key={d.id} value={d.id}>
            {d.name} {d.isDefault && "(Default)"}
          </option>
        ))}
      </select>
    </div>
  );
}
```

```tsx
// HotkeyConfig.tsx
export function HotkeyConfig() {
  const [listening, setListening] = useState(false);
  const [hotkey, setHotkey] = useState({ modifier: "ctrl", key: "Space" });
  
  const handleRecord = () => {
    setListening(true);
    // Listen for next key combination...
  };
  
  return (
    <div className="space-y-2">
      <label className="text-sm font-medium">Push-to-Talk Hotkey</label>
      <button
        onClick={handleRecord}
        className={`
          w-full p-3 border rounded text-left
          ${listening ? "border-blue-500 bg-blue-50" : ""}
        `}
      >
        {listening ? "Press new hotkey..." : `${hotkey.modifier}+${hotkey.key}`}
      </button>
    </div>
  );
}
```

**Acceptance Criteria:**
- [ ] Microphone dropdown populated with available devices
- [ ] Hotkey recorder captures key combinations
- [ ] Settings save immediately on change
- [ ] Validation prevents invalid configurations
- [ ] Reset to defaults option

---

#### Task 1D.3: Replacement Manager
**Complexity:** M (Medium)
**Assignable:** Yes (after 1A.4)
**Dependencies:** 1A.4

**Description:** CRUD interface for text replacements.

**Files to Create:**
- `src/components/Replacements/ReplacementList.tsx`
- `src/components/Replacements/ReplacementEditor.tsx`
- `src/components/Replacements/SmartExpansions.tsx`

**Implementation Details:**
```tsx
// ReplacementList.tsx
interface Replacement {
  id: string;
  trigger: string;
  replacement: string;
  type: "snippet" | "smart";
  enabled: boolean;
}

export function ReplacementList() {
  const [replacements, setReplacements] = useState<Replacement[]>([]);
  const [editing, setEditing] = useState<string | null>(null);
  
  return (
    <div className="space-y-4">
      <div className="flex justify-between items-center">
        <h2 className="text-lg font-semibold">Text Replacements</h2>
        <button 
          onClick={addNew}
          className="px-3 py-1 bg-blue-500 text-white rounded"
        >
          Add New
        </button>
      </div>
      
      <table className="w-full">
        <thead>
          <tr className="text-left text-sm text-gray-500">
            <th className="pb-2">Trigger</th>
            <th className="pb-2">Replacement</th>
            <th className="pb-2">Enabled</th>
            <th className="pb-2"></th>
          </tr>
        </thead>
        <tbody>
          {replacements.map((r) => (
            <ReplacementRow 
              key={r.id} 
              replacement={r} 
              onEdit={() => setEditing(r.id)}
              onDelete={() => deleteReplacement(r.id)}
              onToggle={() => toggleEnabled(r.id)}
            />
          ))}
        </tbody>
      </table>
      
      {editing && (
        <ReplacementEditor
          replacement={replacements.find(r => r.id === editing)!}
          onSave={saveReplacement}
          onCancel={() => setEditing(null)}
        />
      )}
    </div>
  );
}
```

**Default Replacements (ship with app):**
```json
[
  {"trigger": "brb", "replacement": "be right back", "type": "snippet"},
  {"trigger": "ty", "replacement": "thank you", "type": "snippet"},
  {"trigger": "@@date", "replacement": "", "type": "smart"},
  {"trigger": "@@time", "replacement": "", "type": "smart"}
]
```

**Acceptance Criteria:**
- [ ] List view shows all replacements
- [ ] Add new replacement with validation
- [ ] Edit existing replacements inline or modal
- [ ] Delete with confirmation
- [ ] Toggle enabled/disabled per replacement
- [ ] Import/export replacements as JSON
- [ ] Search/filter replacements

---

### Phase 1E: Integration & Testing

---

#### Task 1E.1: End-to-End Flow Integration
**Complexity:** M (Medium)
**Assignable:** Yes (after all previous tasks)
**Dependencies:** 1B.2, 1B.3, 1B.5, 1C.1, 1C.2

**Description:** Wire up complete flow: hotkey → record → transcribe → inject.

**Files to Modify:**
- `src-tauri/src/main.rs`
- `src-tauri/src/commands.rs`

**Implementation Details:**
```rust
// main.rs - Main application loop
#[tokio::main]
async fn main() {
    let (hotkey_mgr, mut hotkey_rx) = HotkeyManager::new(Key::ControlLeft, Key::Space);
    let sidecar = SidecarManager::spawn().await.unwrap();
    let injector = TextInjector::new(50);
    
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle();
            
            // Hotkey listener task
            tokio::spawn(async move {
                while let Some(event) = hotkey_rx.recv().await {
                    match event {
                        HotkeyEvent::Pressed => {
                            update_tray_icon(&app_handle, true);
                            sidecar.call("start_recording", json!({})).await.unwrap();
                        }
                        HotkeyEvent::Released => {
                            update_tray_icon(&app_handle, false);
                            let result = sidecar.call("stop_and_transcribe", json!({})).await.unwrap();
                            let text = result["text"].as_str().unwrap_or("");
                            if !text.is_empty() {
                                injector.inject(text).await.unwrap();
                            }
                        }
                    }
                }
            });
            
            hotkey_mgr.start();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Acceptance Criteria:**
- [ ] Press hotkey → tray icon changes to recording
- [ ] Release hotkey → transcription appears in focused field
- [ ] Replacements applied before injection
- [ ] Works across all target platforms
- [ ] Error handling shows user-friendly messages
- [ ] No crash on rapid press/release

---

#### Task 1E.2: Error Handling & Recovery
**Complexity:** S (Small)
**Assignable:** Yes (after 1E.1)
**Dependencies:** 1E.1

**Description:** Comprehensive error handling throughout the app.

**Error Scenarios:**
| Scenario | Handling |
|----------|----------|
| No microphone | Show settings with error message |
| Sidecar crash | Auto-restart, show notification |
| Model load fail | Retry with progress, fallback to CPU |
| Transcription fail | Show error, allow retry |
| Injection fail | Copy to clipboard instead, notify user |
| Hotkey conflict | Warn user, suggest alternatives |

**Acceptance Criteria:**
- [ ] All error scenarios have user-friendly messages
- [ ] Sidecar auto-restarts on crash (max 3 times, then alert)
- [ ] Fallback behaviors work correctly
- [ ] No unhandled promise rejections or panics

---

#### Task 1E.3: Build & Packaging
**Complexity:** M (Medium)
**Assignable:** Yes (after 1E.1)
**Dependencies:** 1E.1

**Description:** Configure builds for Windows, macOS, Linux.

**Files to Create/Modify:**
- `src-tauri/tauri.conf.json`
- `.github/workflows/build.yml`
- `scripts/bundle-sidecar.sh`

**Build Artifacts:**
| Platform | Format | Notes |
|----------|--------|-------|
| Windows | `.msi`, `.exe` | Bundled Python runtime |
| macOS | `.dmg`, `.app` | Universal binary (Intel + ARM) |
| Linux | `.AppImage`, `.deb` | Python via system or bundled |

**Python Bundling Strategy:**
- Use `pyinstaller` to create standalone sidecar binary
- OR bundle `python3.11-embed` (Windows) / use system Python (Linux/macOS)
- Sidecar binary placed in `resources/` folder

**Acceptance Criteria:**
- [ ] `npm run tauri build` produces installable artifacts
- [ ] Python sidecar bundled correctly on all platforms
- [ ] Installer prompts for Accessibility permission (macOS)
- [ ] App launches without requiring Python installation
- [ ] Code signing configured (placeholder for production)

---

## 5. Critical Path & Dependencies

```
                    1A.1 Tauri Scaffold
                           │
         ┌─────────────────┼─────────────────┐
         │                 │                 │
         ▼                 ▼                 ▼
    1C.1 Hotkey      1C.2 Inject       1C.3 Tray
         │                 │                 │
         └────────┬────────┴────────┬────────┘
                  │                 │
                  ▼                 │
    1A.2 Python Scaffold            │
         │                          │
    ┌────┴────┐                     │
    │         │                     │
    ▼         ▼                     │
 1B.1 Enum  1B.3 ASR                │
    │         │                     │
    ▼         │                     │
 1B.2 Capture │                     │
    │         │                     │
    └────┬────┘                     │
         │                          │
         ▼                          │
    1A.3 IPC Bridge ◄───────────────┘
         │
         ▼
    1A.4 Config ────────► 1D.2 Settings
                              │
    1B.4 Postprocess          │
         │                    │
    1B.5 Replacements ──► 1D.3 Replace UI
         │                    │
         └────────┬───────────┘
                  │
                  ▼
    1D.1 Status Display
                  │
                  ▼
    1E.1 Integration
         │
    ┌────┴────┐
    │         │
    ▼         ▼
 1E.2 Errors 1E.3 Build
```

**Critical Path (longest dependency chain):**
1A.1 → 1A.2 → 1B.1 → 1B.2 → 1A.3 → 1E.1 → 1E.3

**Parallelization Opportunities:**
- 1C.* (Hotkey, Inject, Tray) can run in parallel after 1A.1
- 1B.3 (ASR) can run in parallel with 1B.1/1B.2
- 1B.4, 1B.5 are independent
- 1D.* can run in parallel after their dependencies

---

## 6. Risk Mitigation

### Technical Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Parakeet model too large | Medium | High | Offer smaller model option; lazy loading |
| Hotkey doesn't work on Wayland | High | Medium | Use D-Bus portal; document limitation |
| Text injection blocked by app | Medium | Medium | Clipboard fallback; whitelist apps |
| Python bundling increases size | High | Low | Accept ~50MB overhead; optimize later |
| CUDA not detected | Medium | Medium | Clear messaging; CPU fallback works |

### Platform-Specific Risks

**macOS:**
- Accessibility permission required → Prompt with clear instructions
- Notarization required for distribution → Configure in CI
- Gatekeeper blocks unsigned apps → Document manual override

**Windows:**
- Antivirus may flag keyboard simulation → Sign binary, submit to vendors
- Windows Defender SmartScreen → Sign with EV certificate (future)

**Linux:**
- Wayland hotkey limitations → Support X11 fully, Wayland best-effort
- Audio permissions (PipeWire/PulseAudio) → Document setup

---

## 7. Testing Strategy

### Unit Tests
- `sidecar/tests/test_replacements.py` - Replacement engine
- `sidecar/tests/test_postprocess.py` - Text cleanup
- `src-tauri/src/config.rs` - Config serialization

### Integration Tests
- IPC round-trip (Tauri ↔ Python)
- Audio capture → transcription flow (mock model)
- Hotkey → injection flow (UI automation)

### Manual Testing Checklist
- [ ] Fresh install on each platform
- [ ] Microphone selection works
- [ ] Hotkey triggers recording
- [ ] Transcription appears in various apps (browser, VS Code, terminal)
- [ ] Replacements applied correctly
- [ ] Settings persist across restarts
- [ ] Tray icon updates correctly
- [ ] App quits cleanly

---

## 8. Delivery Milestones

| Milestone | Tasks | Target |
|-----------|-------|--------|
| **M1: Foundation** | 1A.1, 1A.2, 1A.3, 1A.4 | Week 1 |
| **M2: Audio Pipeline** | 1B.1, 1B.2, 1B.3, 1B.4, 1B.5 | Week 2 |
| **M3: Desktop Integration** | 1C.1, 1C.2, 1C.3 | Week 2 (parallel) |
| **M4: UI** | 1D.1, 1D.2, 1D.3 | Week 3 |
| **M5: Integration** | 1E.1, 1E.2, 1E.3 | Week 4 |
| **MVP Release** | All Phase 1 | End of Week 4 |

---

## 9. Task Assignment Matrix

For parallel agent execution, tasks are grouped by independence:

**Group A (No dependencies - start immediately):**
- 1A.1 Tauri Scaffold
- 1A.2 Python Scaffold
- 1B.4 Text Post-Processing
- 1B.5 Replacement Engine

**Group B (After 1A.1):**
- 1C.1 Hotkey System
- 1C.2 Text Injection
- 1C.3 System Tray

**Group C (After 1A.2):**
- 1B.1 Audio Enumeration
- 1B.3 Parakeet Loading

**Group D (After 1A.1 + 1A.2):**
- 1A.3 IPC Bridge

**Group E (After 1A.3 + 1A.4):**
- 1D.1 Status Display
- 1D.2 Settings Panel
- 1D.3 Replacement Manager

**Group F (After all above):**
- 1E.1 Integration
- 1E.2 Error Handling
- 1E.3 Build & Packaging

---

*End of Implementation Plan*
