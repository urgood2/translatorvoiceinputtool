# Dependencies and Tech Stack

This reference captures dependencies and integration points from plan §6.
Use this before adding libraries, changing build pipelines, or extending runtime capabilities.

## Rust Host Dependencies

1. Core framework
- `tauri`: desktop host framework (project may target Tauri v1 or v2 based on lock/config).

2. Input and integration
- `global_hotkey`: system-wide hotkey registration.
- Platform-specific injection backends in `src-tauri/src/injection.rs` and related focus/input modules.

3. Optional media/runtime features
- `rodio` (optional): audio cue playback using packaged WAV assets in `src-tauri/sounds/*.wav`.

4. Core runtime libraries
- `serde` and `serde_json`: serialization and contract payload handling.
- `uuid`: session identifier generation.
- `chrono`: timestamp handling.
- `tokio`: async runtime and task orchestration.

## Python Sidecar Dependencies

1. Audio runtime
- `sounddevice`: recording and playback device access.
- `numpy`: signal and buffer processing.

2. ASR and model runtime
- `faster-whisper` with `ctranslate2` (optional): Whisper backend path for Phase 4.

3. Voice activity detection
- Lightweight optional VAD choices (for Phase 3): `webrtcvad` or Silero-based dependencies.

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
- `bun` is the preferred package manager (`bun.lock` present).

2. Rust
- `cargo` for host build/test/package workflows.

3. Python
- `pip` and `uv` for sidecar dependency management and packaging workflows.

## Integration Guardrails

- Keep new dependencies minimal and justified by concrete runtime requirements.
- Prefer additive dependency introduction to reduce brownfield breakage risk.
- Any dependency with licensing or attribution impact must update `docs/THIRD_PARTY_NOTICES.md`.
- Model/runtime dependencies that expand attack surface must follow security and privacy policy defaults.
