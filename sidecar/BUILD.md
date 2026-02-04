# OpenVoicy Sidecar Build Guide

This document describes how to build the OpenVoicy sidecar as a standalone executable for each platform.

## Quick Start

```bash
# Linux / macOS
./scripts/build-sidecar.sh

# Windows (PowerShell)
.\scripts\build-sidecar.ps1
```

## Build Output

After a successful build:
```
sidecar/dist/
├── openvoicy-sidecar      # Linux/macOS executable
├── openvoicy-sidecar.exe  # Windows executable
└── manifest.json          # Build metadata
```

## Platform Support

| Platform | Architecture | Status | Notes |
|----------|-------------|--------|-------|
| Linux | x64 | ✅ Tested | Requires libportaudio2 system package |
| Linux | arm64 | 🔲 Untested | Should work with arm64 PortAudio |
| macOS | x64 | 🔲 Untested | May trigger Gatekeeper |
| macOS | arm64 | 🔲 Untested | Native Apple Silicon |
| Windows | x64 | 🔲 Untested | VC++ runtime may be needed |

## Prerequisites

### All Platforms
- Python 3.11+ with pip
- Git (for version stamping)

### Linux
```bash
# Debian/Ubuntu
sudo apt-get install libportaudio2

# RHEL/Fedora
sudo dnf install portaudio
```

### macOS
PortAudio is bundled by sounddevice. No additional dependencies required.

If Gatekeeper blocks the binary on first run:
```bash
# Option 1: Right-click → Open in Finder
# Option 2: Remove quarantine attribute
xattr -d com.apple.quarantine ./openvoicy-sidecar
```

### Windows
PortAudio is bundled by sounddevice. May require Visual C++ Redistributable
if not already installed:
- Download from: https://aka.ms/vs/17/release/vc_redist.x64.exe

## Build Options

### Linux / macOS
```bash
./scripts/build-sidecar.sh [--clean] [--no-verify]
```
- `--clean`: Remove build artifacts before building
- `--no-verify`: Skip the binary verification step

### Windows
```powershell
.\scripts\build-sidecar.ps1 [-Clean] [-NoVerify]
```

## Verification

The build script automatically verifies the binary by testing:
1. `system.ping` - Basic JSON-RPC communication
2. `audio.list_devices` - Audio subsystem initialization

Manual verification:
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"system.ping"}' | ./dist/openvoicy-sidecar
# Expected: {"jsonrpc":"2.0","id":1,"result":{"version":"0.1.0","protocol":"v1"}}
```

## Size Targets

| Configuration | Target | Current |
|--------------|--------|---------|
| Audio-only (CPU) | < 100 MB | ~57 MB |
| + ONNX Runtime | < 200 MB | TBD |
| + Model files | < 500 MB | TBD |

## GPU Support

The current build is **CPU-only**. GPU support will be added post-MVP:

| GPU Type | Status | Notes |
|----------|--------|-------|
| None (CPU) | ✅ Default | Works everywhere |
| NVIDIA CUDA | 🔲 Planned | Requires CUDA runtime |
| Apple Metal | 🔲 Planned | Via CoreML/ANE |
| DirectML | 🔲 Planned | Windows GPU fallback |

## Troubleshooting

### "PortAudio library not found"
- **Linux**: Install `libportaudio2` package
- **Windows/macOS**: This shouldn't happen; file an issue

### "Permission denied" (macOS)
```bash
chmod +x ./dist/openvoicy-sidecar
xattr -d com.apple.quarantine ./dist/openvoicy-sidecar
```

### Slow startup (> 5s)
First run may be slower due to extraction. Subsequent runs should be faster.
If consistently slow, check:
- Antivirus scanning the temp directory
- Disk I/O performance

### Missing numpy.core._methods warning
This is a harmless warning from PyInstaller. NumPy 2.x reorganized its internals
and this hidden import no longer exists.

## CI Integration

For CI builds, use `--no-verify` to skip audio device checks in headless environments:
```bash
./scripts/build-sidecar.sh --clean --no-verify
```

The verification step requires audio hardware, which isn't available in most CI runners.

## Manifest Format

```json
{
  "artifact_name": "openvoicy-sidecar",
  "version": "0.1.0",
  "platform": "linux-x64",
  "python_version": "3.13.7",
  "build_timestamp": "2026-02-04T07:38:10Z",
  "git_sha": "72f01bc",
  "binary_size_bytes": 59700672,
  "startup_time_ms": 1519,
  "gpu_support": "none",
  "onnxruntime_version": "not-included",
  "build_time_seconds": 61
}
```

## Development

### Rebuilding After Changes
```bash
./scripts/build-sidecar.sh --clean
```

### Testing Without Packaging
```bash
cd sidecar
python -m openvoicy_sidecar
```

### Adding Dependencies
1. Add to `pyproject.toml` dependencies
2. Add any hidden imports to `openvoicy_sidecar.spec` if PyInstaller misses them
3. Rebuild and verify

## Tauri Bundling

After building the sidecar, bundle it for Tauri:

### Quick Start
```bash
# 1. Build the sidecar binary
./scripts/build-sidecar.sh

# 2. Bundle it for Tauri (copies with target-triple naming)
./scripts/bundle-sidecar.sh

# 3. Build the Tauri app
cd src-tauri && cargo tauri build
```

### How It Works

Tauri's `externalBin` feature bundles external executables with the app. The binary
must be named with the target triple suffix:

```
src-tauri/binaries/
├── openvoicy-sidecar-x86_64-unknown-linux-gnu      # Linux x64
├── openvoicy-sidecar-aarch64-unknown-linux-gnu     # Linux ARM64
├── openvoicy-sidecar-x86_64-apple-darwin           # macOS Intel
├── openvoicy-sidecar-aarch64-apple-darwin          # macOS Apple Silicon
└── openvoicy-sidecar-x86_64-pc-windows-msvc.exe    # Windows x64
```

The `bundle-sidecar.sh` script automates this naming.

### Cross-Platform Builds

Build on each target platform, then collect binaries:

```bash
# On Linux x64
./scripts/build-sidecar.sh
./scripts/bundle-sidecar.sh --target x86_64-unknown-linux-gnu

# On macOS ARM64
./scripts/build-sidecar.sh
./scripts/bundle-sidecar.sh --target aarch64-apple-darwin

# On Windows
.\scripts\build-sidecar.ps1
.\scripts\bundle-sidecar.ps1 -Target x86_64-pc-windows-msvc
```

### Platform-Specific Notes

#### macOS
The bundled binary may trigger Gatekeeper on first run. The Rust code in
`src-tauri/src/sidecar.rs` automatically removes the quarantine attribute:
```bash
xattr -d com.apple.quarantine <binary>
```

#### Linux
Ensure executable permissions are set. The bundle script handles this.

#### Windows
Windows Defender may scan the binary on first run, causing a slight delay.
Consider code signing for production releases.
