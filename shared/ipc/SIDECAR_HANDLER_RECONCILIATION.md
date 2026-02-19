# Sidecar Handler Reconciliation vs IPC Protocol v1

Date: 2026-02-19  
Task: `translatorvoiceinputtool-2gj.4.5`

## Scope

- Sidecar dispatch inventory source: `sidecar/src/openvoicy_sidecar/server.py`
- Protocol method inventory source: `shared/ipc/IPC_PROTOCOL_V1.md`
- Host dependency/fallback source: `src-tauri/src/integration.rs`

## Reconciliation Summary

- Implemented JSON-RPC methods in dispatch table: 24
- Documented JSON-RPC methods in `IPC_PROTOCOL_V1.md`: 24
- Implemented but undocumented: 0
- Documented but unimplemented: 0

Command parity check:

- `implemented_not_in_spec`: none
- `spec_not_implemented`: none

## Dispatch Inventory Log

| Method | Handler |
| --- | --- |
| `system.ping` | `handle_system_ping` |
| `system.info` | `handle_system_info` |
| `system.shutdown` | `handle_system_shutdown` |
| `status.get` | `handle_status_get` |
| `audio.list_devices` | `handle_audio_list_devices` |
| `audio.set_device` | `handle_audio_set_device` |
| `audio.meter_start` | `handle_audio_meter_start` |
| `audio.meter_stop` | `handle_audio_meter_stop` |
| `audio.meter_status` | `handle_audio_meter_status` |
| `recording.start` | `handle_recording_start` |
| `recording.stop` | `handle_recording_stop` |
| `recording.cancel` | `handle_recording_cancel` |
| `recording.status` | `handle_recording_status` |
| `replacements.get_rules` | `handle_replacements_get_rules` |
| `replacements.set_rules` | `handle_replacements_set_rules` |
| `replacements.get_presets` | `handle_replacements_get_presets` |
| `replacements.get_preset_rules` | `handle_replacements_get_preset_rules` |
| `replacements.preview` | `handle_replacements_preview` |
| `model.get_status` | `handle_model_get_status` |
| `model.download` | `handle_model_download` |
| `model.purge_cache` | `handle_model_purge_cache` |
| `asr.initialize` | `handle_asr_initialize` |
| `asr.status` | `handle_asr_status` |
| `asr.transcribe` | `handle_asr_transcribe` |

## Previously Undocumented Methods: Required vs Optional

Classification rule used:
- `REQUIRED`: host makes RPC call and does not have fallback for method absence.
- `OPTIONAL`: host does not call method, or host explicitly tolerates missing method.

| Method | Host dependency evidence | Classification | Notes |
| --- | --- | --- | --- |
| `asr.status` | No host RPC call found in `src-tauri/src` | `OPTIONAL` | Diagnostic only. |
| `asr.transcribe` | No host RPC call found in `src-tauri/src` | `OPTIONAL` | Utility/testing path only. |
| `recording.status` | No host RPC call found in `src-tauri/src` | `OPTIONAL` | Host emits `recording:status` events locally, does not query RPC method. |
| `audio.meter_status` | No host RPC call found in `src-tauri/src` | `OPTIONAL` | Meter lifecycle uses start/stop only. |
| `model.download` | Host calls `model.download` at `src-tauri/src/integration.rs:1325`; fallback to `model.install` and tolerated `E_METHOD_NOT_FOUND` at `src-tauri/src/integration.rs:1329` | `OPTIONAL` | Explicit fallback path exists. |
| `replacements.get_rules` | Host calls method at `src-tauri/src/integration.rs:1581`; no `E_METHOD_NOT_FOUND` fallback branch | `REQUIRED` | Required for active rules UI flow. |
| `replacements.get_presets` | Host calls method at `src-tauri/src/integration.rs:1528`; no `E_METHOD_NOT_FOUND` fallback branch | `REQUIRED` | Required for preset listing flow. |
| `replacements.get_preset_rules` | Host calls method at `src-tauri/src/integration.rs:1558`; no `E_METHOD_NOT_FOUND` fallback branch | `REQUIRED` | Required for preset detail flow. |
| `replacements.preview` | No host RPC call found in `src-tauri/src` | `OPTIONAL` | Not currently consumed by host. |

## Shape Mismatches (Implementation vs `IPC_PROTOCOL_V1.md`)

1. `audio.list_devices` sample-rate field name mismatch
- Spec examples use `sample_rate` in `audio.list_devices` response (`shared/ipc/IPC_PROTOCOL_V1.md` under method section).
- Sidecar emits `default_sample_rate` (`sidecar/src/openvoicy_sidecar/audio.py` in `AudioDevice.to_dict`).
- Host currently adapts this by mapping `default_sample_rate -> sample_rate` for frontend (`src-tauri/src/commands.rs:239` and `src-tauri/src/commands.rs:245`).
- Correction candidate: either document sidecar wire as `default_sample_rate`, or emit both fields during compatibility window.

2. `model.get_status` / `model.download` error field naming mismatch
- Spec text/schema describes `error_message`.
- Sidecar status payload uses `error` field (`sidecar/src/openvoicy_sidecar/model_cache.py` in `ModelCacheManager.get_status`).
- Host currently tolerates both (`src-tauri/src/integration.rs:129` has `error_message` and `src-tauri/src/integration.rs:114` has `error`).
- Correction candidate: align protocol text/schema to include `error` as canonical (or dual-field compatibility).

3. `model.*` progress `total` nullability mismatch
- Sidecar emits `progress.total = null` when unknown (`sidecar/src/openvoicy_sidecar/model_cache.py` in `DownloadProgress.to_dict`).
- Spec schema currently types `total` as integer.
- Host already treats `total` as optional (`src-tauri/src/integration.rs:139`).
- Correction candidate: update protocol schema to allow `null` (or omit `total` instead of sending null).

4. `recording.start` parameter strictness mismatch
- Spec marks `session_id` required.
- Sidecar accepts omitted `session_id` and generates one (`sidecar/src/openvoicy_sidecar/recording.py` in `handle_recording_start`).
- Correction candidate: document `session_id` as optional (host-provided preferred) for backward compatibility.

5. `asr.initialize` parameter strictness mismatch
- Spec marks `model_id` and `device_pref` as required.
- Sidecar defaults both when omitted (`sidecar/src/openvoicy_sidecar/asr/__init__.py` in `handle_asr_initialize`).
- Correction candidate: mark both parameters optional-with-default in protocol docs/schema.

## Output for Downstream Tasks

- Method inventory is now reconciled: there are no method-name deltas between dispatch and protocol.
- Remaining work is doc/schema alignment for the shape mismatches above (feeds `translatorvoiceinputtool-2gj.4.4` and IPC doc cleanup in `translatorvoiceinputtool-2gj.1.12`).
