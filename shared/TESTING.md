# Testing Data Conventions

This document defines test-data conventions for Rust, Python sidecar, and frontend tests.
The goal is deterministic tests without repository bloat.

## Core Rules

1. Do not commit large audio/model artifacts.
2. Generate temporary test inputs at runtime whenever possible.
3. Keep committed fixtures small and human-reviewable.
4. Prefer deterministic mock data over network/model-dependent test inputs.

## Size Limits

1. Committed fixture target: under 100 KB per file.
2. Anything larger than 100 KB must be generated during the test run or kept local-only.
3. Never commit model weights or downloaded cache data.

## Audio Test Data

1. Generate synthetic audio in tests (for example with `numpy`, optionally `scipy` when needed), including:
- `sine` for meter and amplitude checks.
- `silence` for no-signal and VAD-related checks.
- `speech_like` noise for robustness checks.
2. Keep generated buffers in memory where possible.
3. If a temporary file is needed, create it under a temp directory and remove it after use.
4. Do not commit recorded microphone samples.

Example helper signature:
- `generate_test_audio(duration_ms, sample_rate, pattern) -> bytes`

## Model Test Data

1. Use mock manifests with deterministic fake SHA256 values.
2. Use tiny placeholder files for install/verify logic tests.
3. Never commit real model files.
4. Real-model smoke tests must be optional and skip when unavailable (exit/skip code 77 behavior).

## Transcription Mock Data

1. Unit tests should mock sidecar transcription responses.
2. Mock output must be deterministic for a given input.
3. Include edge cases:
- Empty transcript
- Very long transcript
- Unicode and special characters

## Config Test Data

Keep small JSON fixtures covering:

1. Minimal valid config
2. Full config with optional fields
3. Legacy config requiring migration
4. Corrupt/invalid config handling

## Fixture Locations

1. Rust: `src-tauri/tests/fixtures/` (or inline in test modules when small)
2. Python sidecar: `sidecar/tests/fixtures/` plus shared helpers in `sidecar/tests/conftest.py` when present
3. Frontend: `src/tests/fixtures/` (or inline fixtures for narrow unit tests)
4. IPC examples: `shared/ipc/examples/IPC_V1_EXAMPLES.jsonl` remains canonical and human-edited

## Generated and Local-Only Data

1. Generated test artifacts must go under ignored directories (see `.gitignore`).
2. Optional local-only whisper smoke fixtures must stay untracked.
3. Logs and debug dumps from tests must stay untracked.

## Validation Checklist

Before landing test changes:

1. No committed fixture exceeds 100 KB.
2. No model caches or weights are staged.
3. Tests pass with generated/mocked fixtures only.

Suggested audit command:

```bash
git ls-files | rg '(^|/)(tests?|fixtures?)/' | xargs -r ls -l | awk '$5 > 102400 {print $0}'
```
