# Voice Input Tool

A desktop application for voice-to-text transcription using local ASR (Automatic Speech Recognition).

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) (or Node.js 18+)
- Platform-specific requirements (see below)

### Linux

```bash
# Debian/Ubuntu
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
    build-essential \
    curl \
    wget \
    file \
    libxdo-dev \
    libssl-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev

# Fedora
sudo dnf install webkit2gtk4.1-devel \
    openssl-devel \
    curl \
    wget \
    file \
    libxdo-devel \
    librsvg2-devel

# Arch
sudo pacman -S webkit2gtk-4.1 \
    base-devel \
    curl \
    wget \
    file \
    openssl \
    appmenu-gtk-module \
    libxdo \
    librsvg
```

### macOS

```bash
# Xcode Command Line Tools (if not already installed)
xcode-select --install
```

**Note:** macOS requires granting Microphone and Accessibility permissions when prompted.

### Windows

- [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (usually pre-installed on Windows 10/11)

## Quick Start (Smoke Test)

```bash
# 1. Install dependencies
bun install

# 2. Run in development mode
bun run tauri dev

# The app should:
# - Launch a window titled "Voice Input Tool"
# - Show a text input and "Call Rust Echo Command" button
# - Type a message and click the button - it should echo back from Rust
# - Edit src/App.tsx and save - the UI should hot reload immediately
```

## Project Structure

```
.
├── src/                    # React frontend
│   ├── App.tsx            # Main React component
│   ├── main.tsx           # React entry point
│   └── index.css          # Tailwind CSS styles
├── src-tauri/             # Rust backend
│   ├── src/
│   │   ├── lib.rs         # Tauri commands and setup
│   │   └── main.rs        # Entry point
│   ├── Cargo.toml         # Rust dependencies
│   └── tauri.conf.json    # Tauri configuration
├── shared/                # Shared contracts
│   ├── ipc/               # IPC protocol definitions
│   └── model/             # Model manifest
└── docs/                  # Documentation
```

## Development Commands

```bash
# Start development server with hot reload
bun run tauri dev

# Build for production
bun run tauri build

# Run frontend only (no Tauri)
bun run dev

# Type check
bun run build

# Lint
bun run lint
```

## Platform Permissions

### macOS

The app requires:
- **Microphone**: To record audio for transcription
- **Accessibility**: To type transcribed text into other applications

These permissions are requested via `Info.plist` entries and will prompt the user on first use.

### Linux

Audio capture typically works out of the box via PulseAudio/PipeWire. For typing into other applications, `libxdo` is used.

### Windows

No special permissions typically required. The app may prompt for microphone access on first use.

## License

See [THIRD_PARTY_NOTICES.md](docs/THIRD_PARTY_NOTICES.md) for third-party licenses.
