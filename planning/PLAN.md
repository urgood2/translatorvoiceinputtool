# OpenVoicy - Implementation Plan v0

**Version:** 0.1.0 (MVP)
**Date:** 2026-02-02
**Status:** Draft

---

## Executive Summary

OpenVoicy is an open-source, cross-platform speech-to-text application that injects transcribed text directly into any focused text field. It uses NVIDIA's Parakeet V3 model for high-accuracy, offline transcription with a Tauri-based UI and Python sidecar for ML inference.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                      OpenVoicy App                          │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐    ┌─────────────────────────────────┐ │
│  │   Tauri Core    │◄──►│      Web UI (TypeScript)        │ │
│  │   (Rust)        │    │      - Settings panel           │ │
│  │                 │    │      - Status indicator         │ │
│  │  - Hotkey mgmt  │    │      - Text replacements        │ │
│  │  - System tray  │    │      - History view             │ │
│  │  - IPC bridge   │    └─────────────────────────────────┘ │
│  │  - Text inject  │                                        │
│  └────────┬────────┘                                        │
│           │ IPC (JSON-RPC / stdin-stdout)                   │
│           ▼                                                 │
│  ┌─────────────────────────────────────────────────────────┐│
│  │              Python Sidecar                             ││
│  │  - Audio capture (sounddevice/pyaudio)                  ││
│  │  - Parakeet V3 inference (NeMo / MLX)                   ││
│  │  - Text post-processing                                 ││
│  │  - Replacement engine                                   ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

---

## Technology Stack

| Layer | Technology | Rationale |
|-------|------------|-----------|
| **Frontend** | TypeScript + React + Tailwind | Modern, fast UI development |
| **Desktop Shell** | Tauri 2.x | Lightweight (~5MB), secure, Rust-powered |
| **ML Backend** | Python 3.11+ | Best ecosystem for NeMo/MLX integration |
| **ASR Model** | Parakeet V3 (0.6B) | Fastest, most accurate open model |
| **Audio Capture** | sounddevice | Cross-platform, low-latency |
| **Text Injection** | enigo (Rust) | Cross-platform keyboard simulation |
| **IPC** | Tauri sidecar + JSON-RPC | Clean separation of concerns |

---

## Feature Breakdown

### Phase 1: Core MVP (v0.1.0)

#### 1.1 Audio Capture & Recording
- [ ] Microphone selection and enumeration
- [ ] Push-to-talk recording (hotkey-triggered)
- [ ] Audio buffer management
- [ ] Visual recording indicator (system tray + UI)

#### 1.2 Speech-to-Text Engine
- [ ] Parakeet V3 model loading and inference
- [ ] GPU acceleration (CUDA on Windows, MLX on macOS)
- [ ] CPU fallback for systems without GPU
- [ ] Automatic punctuation and capitalization

#### 1.3 Text Injection
- [ ] Cross-platform keyboard simulation
- [ ] Inject text into focused text field
- [ ] Handle special characters and Unicode
- [ ] Configurable injection delay

#### 1.4 Hotkey System
- [ ] Global hotkey registration
- [ ] Configurable key combinations
- [ ] Push-to-talk behavior (hold to record)
- [ ] Conflict detection with system hotkeys

#### 1.5 System Tray
- [ ] Tray icon with status indication
- [ ] Quick access menu (settings, quit)
- [ ] Recording state visualization
- [ ] Tooltip with current status

#### 1.6 Text Replacement Engine
- [ ] Simple snippet expansion (brb → be right back)
- [ ] Smart expansions (@@date, @@time, @@email)
- [ ] Correction rules (case fixes, common errors)
- [ ] JSON-based replacement configuration

#### 1.7 Settings UI
- [ ] Microphone selection
- [ ] Hotkey configuration
- [ ] Text replacement management (CRUD)
- [ ] Model settings (future: model selection)

---

### Phase 2: Enhancements (v0.2.0)

- [ ] Whisper Turbo v3 as alternative model
- [ ] Transcription history with search
- [ ] Multi-language support
- [ ] Auto-start on system boot
- [ ] Update checker

### Phase 3: Advanced Features (v0.3.0+)

- [ ] AI commands (translate, rewrite, summarize)
- [ ] Voice commands (\
- [ ] Voice commands ("delete that", "new line")
- [ ] Custom wake word (optional always-listening)
- [ ] Cloud sync for replacements
- [ ] Plugin system for extensibility

---

*Plan recovered from session transcript on 2026-02-04*
