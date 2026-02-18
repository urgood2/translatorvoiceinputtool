# Privacy Policy

OpenVoicy is designed with privacy as a core principle. All processing happens locally on your device with no data sent to external servers.

---

## Summary

| Data Type | Stored | Sent to Cloud |
|-----------|--------|---------------|
| Voice recordings | ❌ No | ❌ No |
| Transcripts | ❌ No | ❌ No |
| Configuration | ✅ Local only | ❌ No |
| ASR Model | ✅ Local cache | ❌ Downloaded once |

---

## What Data is Stored

### Configuration File
- **Location**: OS-specific config directory (see Data Locations below)
- **Contains**:
  - Microphone device selection
  - Hotkey preferences
  - Injection settings (suffix, Focus Guard, etc.)
  - Replacement rules
- **Format**: JSON file
- **Retention**: Until manually deleted or app uninstalled

### Model Cache
- **Location**: OS-specific cache directory
- **Contains**: NVIDIA Parakeet ASR model files (~2.5 GB)
- **Source**: Downloaded from Hugging Face on first run
- **Retention**: Cached indefinitely for offline use

### Logs (Optional)
- **Location**: OS-specific log directory
- **Contains**: Application events, errors, timing information
- **Does NOT contain**: Transcript text or audio data
- **Retention**: Ring buffer, bounded size, not persisted by default
- **Purpose**: Diagnostics and troubleshooting only

---

## What Data is NOT Stored

### Voice Recordings
Audio captured from your microphone is:
- Processed in memory for transcription
- Immediately discarded after processing
- Never saved to disk
- Never sent to any server

### Transcripts
Transcribed text is:
- Held in memory for the current session (history feature)
- Cleared when the app closes
- Never saved to disk
- Never sent to any server

### Usage Analytics
OpenVoicy does not collect:
- Usage statistics
- Crash reports
- Telemetry of any kind
- Device identifiers

---

## Network Usage

### Model Download
- **When**: First launch (if model not cached)
- **What**: ASR model files from Hugging Face
- **Connection**: HTTPS to huggingface.co
- **After download**: No further network required

### During Transcription
- **Network**: Not used
- **All processing**: Local, on your CPU/GPU
- **Verification**: Works completely offline after model download

### No Phone-Home
- No update checks (unless you enable them)
- No license verification
- No analytics or telemetry
- No cloud sync

---

## Data Locations

### Configuration
| Platform | Location |
|----------|----------|
| Windows | `%APPDATA%\OpenVoicy\config.json` |
| macOS | `~/Library/Application Support/OpenVoicy/config.json` |
| Linux | `~/.config/OpenVoicy/config.json` |

### Model Cache
| Platform | Location |
|----------|----------|
| Windows | `%LOCALAPPDATA%\openvoicy\cache\` |
| macOS | `~/Library/Caches/openvoicy/` |
| Linux | `~/.cache/openvoicy/` |

### Logs (if enabled)
| Platform | Location |
|----------|----------|
| Windows | `%LOCALAPPDATA%\openvoicy\logs\` |
| macOS | `~/Library/Logs/openvoicy/` |
| Linux | `~/.local/share/openvoicy/logs/` |

---

## How to Clear All Data

### Quick Clear (from App)
1. Open Settings → Advanced → Clear Data
2. Select what to clear:
   - Configuration (resets settings)
   - Model cache (re-download required)
   - Logs

### Complete Removal

#### Windows
```powershell
# Remove config
Remove-Item -Recurse "$env:APPDATA\OpenVoicy"
# Remove cache
Remove-Item -Recurse "$env:LOCALAPPDATA\openvoicy"
```

#### macOS
```bash
# Remove config
rm -rf ~/Library/Application\ Support/OpenVoicy
# Remove cache
rm -rf ~/Library/Caches/openvoicy
# Remove logs
rm -rf ~/Library/Logs/openvoicy
```

#### Linux
```bash
# Remove all OpenVoicy data
rm -rf ~/.config/OpenVoicy
rm -rf ~/.cache/openvoicy
rm -rf ~/.local/share/openvoicy
```

---

## Third-Party Model License

OpenVoicy uses the NVIDIA Parakeet TDT 0.6B model, which is licensed under [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/).

- The model is downloaded from NVIDIA's public Hugging Face repository
- No NVIDIA account required
- Attribution provided in [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)

---

## Permissions Required

### Microphone
- **Why**: To capture voice for transcription
- **When**: Only while actively recording
- **Access**: Granted via OS permission prompt

### Accessibility (macOS only)
- **Why**: To inject text into other applications
- **When**: Only during text injection
- **Alternative**: Clipboard-only mode if permission denied

### Input Monitoring (if applicable)
- **Why**: Global hotkey detection
- **When**: While app is running
- **Alternative**: Toggle mode via tray menu

---

## Questions or Concerns

OpenVoicy is open source. You can:
- Review the code: [GitHub Repository]
- Report privacy issues: [GitHub Issues]
- Build from source for maximum assurance

---

*Last updated: 2026-02-05*
