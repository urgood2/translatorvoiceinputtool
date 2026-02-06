# QA Report - 2026-02-05

## Build Status
- [ ] Builds successfully OR N/A

**Details:**
- Frontend (TypeScript/Vite): **FAILED** - Type errors in test files
- Tauri Backend (Rust): **SUCCESS** - Builds with 25 warnings (dead code)
- Sidecar (Python): **N/A** - Separate build system (uv/PyInstaller)

## Test Summary
| Component | Total | Passed | Failed | Skipped |
|-----------|-------|--------|--------|---------|
| Frontend (vitest) | 176 | 176 | 0 | 0 |
| Rust backend | 210 | 210 | 0 | 0 |
| Python sidecar | 313 | 312 | 0 | 1 |
| **Total** | **699** | **698** | **0** | **1** |

## UBS Scan
- Critical: 0
- Warnings: 56
- Info: 289
- Notes: No critical issues. Warnings are code quality/maintenance items.

## Open Beads
- Open: 0
- In Progress: 0
- Blocked: 0

## Verdict
[ ] READY TO SYNC - All checks pass
[x] NEEDS FIXES - See issues above

**Blocking Issue:** TypeScript build fails due to outdated type definitions in test files. The types `StateEvent`, `TranscriptEntry`, and `AppConfig` have evolved but test mocks weren't updated.

## Created Beads
- `translatorvoiceinputtool-2pk`: Fix TypeScript test files with outdated type definitions (StateEvent missing timestamp, TranscriptEntry missing fields, AppConfig missing schema_version/presets)
- `translatorvoiceinputtool-4zj`: Fix unused imports in TypeScript files (useEffect, getLevelGradient, vi, act)

## TypeScript Errors Summary
| File | Error | Fix Needed |
|------|-------|------------|
| `useTauriEvents.test.ts:67` | StateEvent missing `timestamp` | Add timestamp to mock |
| `useTauriEvents.test.ts:92` | `processing_duration_ms` doesn't exist on TranscriptEntry | Remove or rename field |
| `appStore.test.ts` (multiple) | TranscriptEntry missing `transcription_duration_ms`, `injection_result` | Add fields to mocks |
| `appStore.test.ts` (multiple) | AppConfig missing `schema_version`, `presets` | Add fields to mocks |
| `appStore.test.ts:182` | `device_uid` type mismatch | Fix type |
| `appStore.test.ts:201,208` | `sample_rate` doesn't exist on AudioConfig | Remove or update field |
| `MicrophoneSelect.tsx:10` | Unused `useEffect` import | Remove import |
| `MicrophoneTest.tsx:45` | Unused `getLevelGradient` | Remove function |
| `Replacements.test.tsx:6` | Unused `act` import | Remove import |
