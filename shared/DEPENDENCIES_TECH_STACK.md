# Dependencies and Tech Stack

This reference captures dependencies and integration points from plan §6.
Use this before adding libraries, changing build pipelines, or extending runtime capabilities.

## Rust Host Dependencies

1. Core framework
- `tauri` (v2): desktop host framework.
- `tauri-plugin-shell`: host shell integration.

2. Input and integration
- `global_hotkey`: system-wide hotkey registration.
- Platform-specific injection backends in `src-tauri/src/injection.rs` and related focus/input modules.

3. Media/runtime features
- `rodio` (required in current manifest): audio cue playback using packaged WAV assets in `src-tauri/sounds/*.wav`.
- `zbus` (Linux target dependency): desktop/session integration on Linux builds.

4. Core runtime libraries
- `serde` and `serde_json`: serialization and contract payload handling.
- `uuid`: session identifier generation.
- `chrono`: timestamp handling.
- `tokio`: async runtime and task orchestration.

## Python Sidecar Dependencies

1. Audio runtime
- `sounddevice`: recording and playback device access.
- `numpy`: signal and buffer processing.
- `scipy`: waveform I/O and processing helpers.

2. ASR/model runtime (current)
- Sidecar runtime dependencies in `sidecar/pyproject.toml` are currently:
  - `sounddevice`
  - `numpy`
  - `scipy`

3. ASR/model runtime (planned/optional, not currently in manifest)
- `faster-whisper` with `ctranslate2`: optional Whisper backend path.
- Additional VAD libraries (e.g. `webrtcvad` or Silero-based dependencies) remain optional future work.

## Contracts Tooling

1. Contracts and schema workflow
- Python scripts under `scripts/*.py` for generation and validation.
- JSON Schema draft-07 as the schema baseline.

2. CI validation
- Contract/schema validators run in Python-based CI steps.

## Frontend Stack

1. Application framework
- React + TypeScript.

2. State and styling
- Zustand for state management.
- Tailwind CSS for utility-first styling.

3. Testing and build
- Vitest + Testing Library for frontend tests.
- Vite for development/build, including multi-page support for overlay entry points.

## Build and Packaging Toolchain

1. JavaScript/TypeScript
- `bun` is used for TypeScript/frontend CI build/test steps.
- `npm` is also used in security scanning (`npm audit`) and `package-lock.json` is tracked.

2. Rust
- `cargo` for host build/test/package workflows.

3. Python
- `pip` for sidecar dependency installation and test tooling.
- `hatchling` as the Python build backend for sidecar packaging.

## Integration Guardrails

- Keep new dependencies minimal and justified by concrete runtime requirements.
- Prefer additive dependency introduction to reduce brownfield breakage risk.
- Any dependency with licensing or attribution impact must update `docs/THIRD_PARTY_NOTICES.md`.
- Model/runtime dependencies that expand attack surface must follow security and privacy policy defaults.

## Drift Checklist

When updating dependency manifests, update this document in the same change:
- Rust manifest changes: `src-tauri/Cargo.toml`
- Sidecar manifest changes: `sidecar/pyproject.toml`
- Frontend/package manager changes: `package.json`, `bun.lock`, `package-lock.json`
