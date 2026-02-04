# Coordination Gates and Parallel Execution Strategy

This document defines the hard synchronization points that enable safe parallel development across multiple workstreams.

## Overview

OpenVoicy development is structured around **coordination gates** - concrete verification points that must be passed before dependent work can proceed. This enables multiple developers/agents to work in parallel without conflicts.

## Parallel Execution Strategy

### Recommended 4-Agent Split

| Agent | Focus Area | Tasks |
|-------|------------|-------|
| **A** | Rust IPC/sidecar/state/model/watchdog | M2.1-M2.4, M2.8, M2.10 |
| **B** | Rust hotkey/tray/injection/focus/history/config | M2.5-M2.7, M2.9 |
| **C** | Sidecar protocol/audio/preprocess/replacements | M1.1-M1.4, M1.7-M1.8 |
| **D** | ML/model cache/packaging/decision records | M1.5-M1.6, M5.1, docs |

### Key Principles

1. **Contract-First Development**: IPC protocol is locked before implementation starts
2. **Interface Boundaries**: Clear ownership of modules minimizes merge conflicts
3. **Gate Verification**: Each gate has a concrete demo that proves readiness
4. **No Assumptions**: Don't start dependent work until gate is verified

## Coordination Gates

### Gate 1: IPC Contract Locked

**Milestone:** M0 complete
**Status:** PASSED

**Artifacts:**
- [x] `shared/ipc/IPC_PROTOCOL_V1.md` complete
- [x] `shared/ipc/examples/IPC_V1_EXAMPLES.jsonl` validated

**Enables:** M1, M2 can start in parallel

**Verification:**
```bash
./scripts/demo-gate-1.sh
```

---

### Gate 2: Ping + Info + Device List + Meter Demo

**Milestone:** M0 coordination gate
**Status:** PASSED

**Artifacts:**
- [x] Rust spawns sidecar
- [x] `system.ping` returns valid response
- [x] `system.info` returns capabilities
- [x] `audio.list_devices` returns device list
- [x] `audio.meter_start` emits audio level events

**Enables:** Full M1 and M2 work

---

### Gate 3: Record Loop + Focus Guard + Stub Injection Demo

**Milestone:** M2 coordination gate
**Status:** PASSED

**Artifacts:**
- [x] Hotkey triggers recording
- [x] Recording stops on release (or toggle)
- [x] Focus Guard captures/validates signature
- [x] Injection stub places text on clipboard
- [x] Works without UI open (tray only)

**Enables:** M3 UI work, M4 integration

---

### Gate 4: ASR Returns Text Demo

**Milestone:** M1.6 complete
**Status:** PENDING

**Artifacts:**
- [ ] `asr.initialize` completes (with real model)
- [ ] Recording produces audio
- [ ] Transcription returns text
- [ ] Log showing actual transcription output

**Enables:** E2E integration, M4.1

---

### Gate 5: E2E Inject Without UI Demo

**Milestone:** M4.1 complete
**Status:** PENDING

**Artifacts:**
- [ ] Full flow: hotkey -> record -> transcribe -> inject
- [ ] Text appears in target application
- [ ] Works without settings window open
- [ ] Video or screenshot showing flow

**Enables:** Packaging (M5), release prep

## Blocking Relationships

```
Gate 1 (IPC Contract) ─────┬──> All M1 tasks
                           └──> All M2 tasks

Gate 2 (Ping + Demo) ──────┬──> M1.3+ (audio capture)
                           └──> M2.4+ (recording controller)

Gate 3 (Record Loop) ──────┬──> M3 (UI)
                           └──> M4.1 (E2E integration)

Gate 4 (ASR Text) ─────────┬──> M4.1 (E2E integration)
                           └──> Full E2E testing

Gate 5 (E2E Inject) ───────┬──> M5 (packaging)
                           └──> Release preparation
```

## Gate Verification Process

Each gate must have:

1. **Demo Script**: Concrete verification in `scripts/demo-gate-N.sh`
2. **Artifact**: Log, screenshot, or video committed to repo
3. **Sign-off**: Comment on tracking issue confirming passage
4. **Status Update**: Gate status updated in this document

## Current Status Summary

| Gate | Description | Status | Blocker For |
|------|-------------|--------|-------------|
| 1 | IPC Contract Locked | PASSED | M1, M2 |
| 2 | Ping + Info + Devices | PASSED | M1.3+, M2.4+ |
| 3 | Record Loop + Focus Guard | PASSED | M3, M4.1 |
| 4 | ASR Returns Text | PENDING | M4.1, E2E |
| 5 | E2E Inject Without UI | PENDING | M5, Release |

## Milestone Completion Status

| Milestone | Description | Status |
|-----------|-------------|--------|
| M0 | Project + Contract Lock | COMPLETE |
| M1 | Sidecar Core | IN PROGRESS |
| M2 | Rust Core MVP | COMPLETE |
| M3 | Settings UI | NOT STARTED |
| M4 | Integration Testing | NOT STARTED |
| M5 | Packaging + Distribution | NOT STARTED |

## Related Documents

- [IPC_PROTOCOL_V1.md](../shared/ipc/IPC_PROTOCOL_V1.md) - Locked IPC contract
- [DECISIONS/0001-asr-backend.md](./DECISIONS/0001-asr-backend.md) - ASR backend choice
- [KNOWN_LIMITATIONS.md](./KNOWN_LIMITATIONS.md) - Current limitations
