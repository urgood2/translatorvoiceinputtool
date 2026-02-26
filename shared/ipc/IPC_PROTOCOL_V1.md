# IPC Protocol v1 Specification

**Version:** 1.0
**Status:** LOCKED
**Last Updated:** 2026-02-26

This document defines the authoritative IPC contract between the Rust host and Python sidecar. Both implementations MUST conform to this specification.

---

## Transport Layer

### Wire Format
- **Protocol:** NDJSON (Newline-Delimited JSON) over stdin/stdout
- **Framing:** One JSON object per line, terminated by `\n`
- **Encoding:** UTF-8
- **Flushing:** Flush stdout after each message

### Safety Limits
- **Max inbound line length:** 1 MiB (1,048,576 bytes)
- **Oversize handling:** Fatal error; close connection immediately
- **Both sides** MUST enforce this limit

---

## Message Types

### JSON-RPC 2.0 Compliance

All messages follow [JSON-RPC 2.0](https://www.jsonrpc.org/specification).

### Request

```json
{
  "jsonrpc": "2.0",
  "id": <string|number>,
  "method": "<namespace>.<action>",
  "params": { ... }
}
```

- `id`: Correlation identifier (string or number, must be unique per pending request)
- `method`: Dot-namespaced method name
- `params`: Optional object with method parameters

### Response (Success)

```json
{
  "jsonrpc": "2.0",
  "id": <same as request>,
  "result": { ... }
}
```

### Response (Error)

```json
{
  "jsonrpc": "2.0",
  "id": <same as request>,
  "error": {
    "code": <number>,
    "message": "<human-readable>",
    "data": {
      "kind": "<E_*>",
      "details": <any>
    }
  }
}
```

### Notification (Server → Client)

```json
{
  "jsonrpc": "2.0",
  "method": "event.<type>",
  "params": { ... }
}
```

Notifications have no `id` field and expect no response.

---

## Error Codes

### Standard JSON-RPC 2.0 Codes

| Code | Name | Description |
|------|------|-------------|
| -32700 | Parse error | Invalid JSON received |
| -32600 | Invalid Request | JSON is not a valid Request object |
| -32601 | Method not found | Method does not exist |
| -32602 | Invalid params | Invalid method parameters |
| -32603 | Internal error | Internal JSON-RPC error |

### Application-Specific Codes (Server Error Range: -32000 to -32099)

| Code | Kind | Description |
|------|------|-------------|
| -32001 | E_NOT_READY | Service not initialized or busy |
| -32002 | E_MIC_PERMISSION | Microphone permission denied |
| -32003 | E_DEVICE_NOT_FOUND | Audio device not found |
| -32004 | E_AUDIO_IO | Audio I/O error |
| -32005 | E_NETWORK | Network error (model download) |
| -32006 | E_DISK_FULL | Insufficient disk space |
| -32007 | E_CACHE_CORRUPT | Model cache corrupted |
| -32008 | E_MODEL_LOAD | Failed to load model |
| -32009 | E_TRANSCRIBE | Transcription failed |
| -32603 | E_INTERNAL | Internal error |
| -32601 | E_METHOD_NOT_FOUND | Method not found |
| -32602 | E_INVALID_PARAMS | Invalid parameters |

### Error Kind Strings

All error responses MUST include `error.data.kind` with one of these stable strings:

- `E_METHOD_NOT_FOUND`
- `E_INVALID_PARAMS`
- `E_NOT_READY`
- `E_MIC_PERMISSION`
- `E_DEVICE_NOT_FOUND`
- `E_AUDIO_IO`
- `E_NETWORK`
- `E_DISK_FULL`
- `E_CACHE_CORRUPT`
- `E_MODEL_LOAD`
- `E_TRANSCRIBE`
- `E_INTERNAL`

---

## Methods

### System Methods

#### `system.ping`

Health check endpoint.

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "system.ping" }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "version": "0.1.0",
    "protocol": "v1"
  }
}
```

**Timeout:** 1 second

---

#### `system.info`

Detailed system information.

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "system.info" }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "version": "0.1.0",
    "protocol": "v1",
    "capabilities": ["asr", "replacements", "meter"],
    "runtime": {
      "python_version": "3.11.0",
      "platform": "linux",
      "cuda_available": true
    }
  }
}
```

**Timeout:** 2 seconds

---

#### `system.shutdown`

Request graceful shutdown.

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "system.shutdown", "params": { "reason": "user_requested" } }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "status": "shutting_down" }
}
```

**Timeout:** 2 seconds

---

### Audio Methods

#### `audio.list_devices`

List available audio input devices.

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "audio.list_devices" }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "devices": [
      {
        "uid": "device-uuid-1",
        "name": "Built-in Microphone",
        "is_default": true,
        "sample_rate": 48000,
        "channels": 1
      },
      {
        "uid": "device-uuid-2",
        "name": "USB Headset",
        "is_default": false,
        "sample_rate": 44100,
        "channels": 1
      }
    ]
  }
}
```

**Timeout:** 2 seconds

---

#### `audio.set_device`

Set the active audio input device.

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "audio.set_device", "params": { "device_uid": "device-uuid-2" } }
```

Use `null` for system default:
```json
{ "jsonrpc": "2.0", "id": 1, "method": "audio.set_device", "params": { "device_uid": null } }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "active_device_uid": "device-uuid-2" }
}
```

**Timeout:** 2 seconds

**Errors:**
- `E_DEVICE_NOT_FOUND`: Device UID not found
- `E_MIC_PERMISSION`: Microphone permission denied

---

#### `audio.meter_start`

Start audio level metering.

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "audio.meter_start", "params": { "device_uid": null, "interval_ms": 80 } }
```

**Parameters:**
- `device_uid` (optional): Device to meter, `null` for active device
- `interval_ms` (optional): Update interval in milliseconds
  - Default: 80ms
  - **Clamped to range:** 30-250ms

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "running": true, "interval_ms": 80 }
}
```

**Timeout:** 2 seconds

While metering is active, `event.audio_level` notifications are emitted.

---

#### `audio.meter_stop`

Stop audio level metering.

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "audio.meter_stop" }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "stopped": true }
}
```

**Timeout:** 2 seconds

---

#### `audio.meter_status`

Get current audio meter status.

**Availability:** Optional (feature-detect via `system.info.capabilities` and tolerate `E_METHOD_NOT_FOUND`)

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "audio.meter_status" }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "running": true,
    "interval_ms": 80
  }
}
```

**Parameters Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "properties": {},
  "additionalProperties": false
}
```

**Result Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["running"],
  "properties": {
    "running": { "type": "boolean" },
    "interval_ms": { "type": "integer", "minimum": 1 }
  },
  "additionalProperties": true
}
```

**Timeout:** 2 seconds

---

### Model Methods

#### `model.get_status`

Get current model status.

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "model.get_status" }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "model_id": "parakeet-tdt-0.6b-v3",
    "revision": "main",
    "status": "ready",
    "cache_path": "/home/user/.cache/huggingface/hub/models--nvidia--parakeet-tdt-0.6b-v3"
  }
}
```

**Status values:**
- `"missing"`: Model not downloaded
- `"downloading"`: Download in progress
- `"verifying"`: Verifying model integrity
- `"ready"`: Model loaded and ready
- `"error"`: Error state

**Additional fields by status:**
- `downloading`: includes `progress` object
- `error`: includes `error_message`

**Timeout:** 2 seconds

---

#### `model.download`

Download model artifacts and return current model status.

**Availability:** Optional (host MUST handle `E_METHOD_NOT_FOUND`)

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "model.download", "params": { "model_id": "parakeet-tdt-0.6b-v3" } }
```

`model_id` is currently accepted for forward compatibility; current sidecar implementation resolves the model from manifest defaults.

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "model_id": "parakeet-tdt-0.6b-v3",
    "revision": "main",
    "status": "ready",
    "cache_path": "/home/user/.cache/huggingface/hub/models--nvidia--parakeet-tdt-0.6b-v3"
  }
}
```

**Parameters Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "properties": {
    "model_id": { "type": "string", "minLength": 1 }
  },
  "additionalProperties": true
}
```

**Result Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["status"],
  "properties": {
    "model_id": { "type": "string" },
    "revision": { "type": "string" },
    "status": { "type": "string", "enum": ["missing", "downloading", "verifying", "ready", "error"] },
    "cache_path": { "type": "string" },
    "progress": {
      "type": "object",
      "properties": {
        "current": { "type": "integer", "minimum": 0 },
        "total": { "type": "integer", "minimum": 0 },
        "unit": { "type": "string" }
      },
      "required": ["current", "total", "unit"],
      "additionalProperties": true
    },
    "error_message": { "type": "string" }
  },
  "additionalProperties": true
}
```

**Timeout:** 20 minutes

---

#### `model.purge_cache`

Purge model cache.

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "model.purge_cache", "params": { "model_id": "parakeet-tdt-0.6b-v3" } }
```

Omit `model_id` to purge all cached models:
```json
{ "jsonrpc": "2.0", "id": 1, "method": "model.purge_cache" }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "purged": true }
}
```

**Timeout:** 10 seconds

**Errors:**
- `E_NOT_READY`: Model is currently in use

---

### ASR Methods

#### `asr.initialize`

Initialize ASR engine with specified model.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "asr.initialize",
  "params": {
    "model_id": "parakeet-tdt-0.6b-v3",
    "device_pref": "auto",
    "language": "auto"
  }
}
```

**Parameters:**
- `model_id` (required): Model identifier
- `device_pref` (required): One of `"auto"`, `"cuda"`, `"cpu"`
- `language` (optional, additive): Language preference for ASR decoding
  - `null`: Use backend default behavior
  - `"auto"`: Enable automatic language detection
  - ISO 639-1 code (for example `"en"`, `"es"`): Force decoding for that specific language

`language` is an additive optional field for compatibility. Implementations that do not support it MUST ignore it.

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "status": "ready",
    "model_id": "parakeet-tdt-0.6b-v3",
    "device": "cuda"
  }
}
```

**Behavior:**
- **Idempotent:** If already initialized with same model, returns quickly (<250ms)
- **First-run:** May trigger model download (progress via `event.status_changed`)

**Timeout:** 20 minutes (for first-run download)

**Timeout handling:**
- Timeout is **fatal**: sidecar enters error state
- **Remediation:** Restart sidecar

---

#### `asr.status`

Get current ASR engine status.

**Availability:** Optional diagnostic method (host MUST handle `E_METHOD_NOT_FOUND`)

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "asr.status" }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "state": "ready",
    "model_id": "parakeet-tdt-0.6b-v3",
    "device": "cuda",
    "ready": true
  }
}
```

**Parameters Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "properties": {},
  "additionalProperties": false
}
```

**Result Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["state", "ready"],
  "properties": {
    "state": { "type": "string", "enum": ["uninitialized", "downloading", "loading", "ready", "error"] },
    "model_id": { "type": ["string", "null"] },
    "device": { "type": "string" },
    "ready": { "type": "boolean" }
  },
  "additionalProperties": true
}
```

**Timeout:** 2 seconds

---

#### `asr.transcribe`

Transcribe a single audio file path.

**Availability:** Optional utility method (host MUST handle `E_METHOD_NOT_FOUND`)

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "asr.transcribe",
  "params": {
    "audio_path": "/tmp/input.wav",
    "session_id": "550e8400-e29b-41d4-a716-446655440000",
    "language": "en-US"
  }
}
```

`session_id` and `language` are additive optional fields for compatibility; current sidecar implementation requires `audio_path` and ignores unknown extra params.

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "text": "hello world",
    "duration_ms": 1234
  }
}
```

**Parameters Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["audio_path"],
  "properties": {
    "audio_path": { "type": "string", "minLength": 1 },
    "session_id": { "type": "string" },
    "language": { "type": "string" }
  },
  "additionalProperties": true
}
```

**Result Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["text"],
  "properties": {
    "text": { "type": "string" },
    "language": { "type": "string" },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "duration_ms": { "type": "integer", "minimum": 0 }
  },
  "additionalProperties": true
}
```

**Timeout:** 30 seconds

---

### Recording Methods

#### `recording.start`

Start recording audio.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "recording.start",
  "params": {
    "session_id": "550e8400-e29b-41d4-a716-446655440000",
    "device_uid": null
  }
}
```

**Parameters:**
- `session_id` (required): UUID v4 generated by Rust host
- `device_uid` (optional): Device to record from, `null` for active device

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "session_id": "550e8400-e29b-41d4-a716-446655440000" }
}
```

**Timeout:** 2 seconds

**Errors:**
- `E_NOT_READY`: ASR not initialized
- `E_DEVICE_NOT_FOUND`: Device not found
- `E_MIC_PERMISSION`: Microphone permission denied

---

#### `recording.stop`

Stop recording and begin transcription.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "recording.stop",
  "params": { "session_id": "550e8400-e29b-41d4-a716-446655440000" }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "audio_duration_ms": 3250,
    "sample_rate": 16000,
    "channels": 1,
    "session_id": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

**Behavior:**
- Returns quickly (<250ms)
- Transcription happens asynchronously
- Result delivered via `event.transcription_complete` notification

**Timeout:** 2 seconds

---

#### `recording.cancel`

Cancel recording without transcription.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "recording.cancel",
  "params": { "session_id": "550e8400-e29b-41d4-a716-446655440000" }
}
```

**Availability:** Required

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "cancelled": true,
    "session_id": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

**Parameters Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["session_id"],
  "properties": {
    "session_id": { "type": "string", "minLength": 1 }
  },
  "additionalProperties": false
}
```

**Result Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["cancelled", "session_id"],
  "properties": {
    "cancelled": { "type": "boolean" },
    "session_id": { "type": "string" }
  },
  "additionalProperties": true
}
```

**Behavior:**
- Discards recorded audio
- **MUST NOT** emit `event.transcription_complete`

**Timeout:** 2 seconds

---

#### `recording.status`

Get current recording state.

**Availability:** Optional diagnostic method (host MUST handle `E_METHOD_NOT_FOUND`)

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "recording.status" }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "state": "recording",
    "session_id": "550e8400-e29b-41d4-a716-446655440000",
    "duration_ms": 850,
    "elapsed_sec": 0.85
  }
}
```

**Parameters Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "properties": {},
  "additionalProperties": false
}
```

**Result Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["state", "session_id"],
  "properties": {
    "state": { "type": "string", "enum": ["idle", "recording", "stopping"] },
    "session_id": { "type": ["string", "null"] },
    "duration_ms": { "type": "integer", "minimum": 0 },
    "elapsed_sec": { "type": "number", "minimum": 0 }
  },
  "additionalProperties": true
}
```

**Timeout:** 2 seconds

---

### Replacement Methods

#### `replacements.set_rules`

Set text replacement rules.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "replacements.set_rules",
  "params": {
    "rules": [
      {
        "id": "123e4567-e89b-12d3-a456-426614174000",
        "enabled": true,
        "kind": "literal",
        "pattern": "gonna",
        "replacement": "going to",
        "word_boundary": true,
        "case_sensitive": false,
        "description": "Expand informal contractions",
        "origin": "preset"
      },
      {
        "id": "987fcdeb-51a2-3bc4-d567-890123456789",
        "enabled": true,
        "kind": "regex",
        "pattern": "\\buh+\\b",
        "replacement": "",
        "word_boundary": false,
        "case_sensitive": false,
        "description": "Remove filler words",
        "origin": "user"
      }
    ]
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { "count": 2 }
}
```

**Timeout:** 2 seconds

---

#### `replacements.get_rules`

Get currently active replacement rules.

**Availability:** Optional (host MUST handle `E_METHOD_NOT_FOUND`)

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "replacements.get_rules" }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "rules": [
      {
        "id": "user:1",
        "enabled": true,
        "kind": "literal",
        "pattern": "gonna",
        "replacement": "going to",
        "word_boundary": true,
        "case_sensitive": false
      }
    ]
  }
}
```

**Parameters Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "properties": {},
  "additionalProperties": false
}
```

**Result Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["rules"],
  "properties": {
    "rules": {
      "type": "array",
      "items": { "$ref": "#/definitions/ReplacementRule" }
    }
  },
  "additionalProperties": true
}
```

**Timeout:** 2 seconds

---

#### `replacements.get_presets`

List available replacement presets (metadata only).

**Availability:** Optional (host MUST handle `E_METHOD_NOT_FOUND`)

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "replacements.get_presets" }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "presets": [
      {
        "id": "punctuation",
        "name": "Punctuation",
        "description": "Restore punctuation",
        "rule_count": 12
      }
    ]
  }
}
```

**Parameters Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "properties": {},
  "additionalProperties": false
}
```

**Result Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["presets"],
  "properties": {
    "presets": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["id", "name", "description", "rule_count"],
        "properties": {
          "id": { "type": "string" },
          "name": { "type": "string" },
          "description": { "type": "string" },
          "rule_count": { "type": "integer", "minimum": 0 }
        },
        "additionalProperties": true
      }
    }
  },
  "additionalProperties": true
}
```

**Timeout:** 2 seconds

---

#### `replacements.get_preset_rules`

Get all rules for a preset.

**Availability:** Optional (host MUST handle `E_METHOD_NOT_FOUND`)

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "replacements.get_preset_rules",
  "params": { "preset_id": "punctuation" }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "preset": {
      "id": "punctuation",
      "name": "Punctuation",
      "description": "Restore punctuation",
      "rule_count": 12
    },
    "rules": [
      {
        "id": "punctuation:r1",
        "enabled": true,
        "kind": "literal",
        "pattern": "period",
        "replacement": ".",
        "word_boundary": true,
        "case_sensitive": false,
        "origin": "preset"
      }
    ]
  }
}
```

**Parameters Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["preset_id"],
  "properties": {
    "preset_id": { "type": "string", "minLength": 1 }
  },
  "additionalProperties": false
}
```

**Result Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["preset", "rules"],
  "properties": {
    "preset": {
      "type": "object",
      "required": ["id", "name", "description", "rule_count"],
      "properties": {
        "id": { "type": "string" },
        "name": { "type": "string" },
        "description": { "type": "string" },
        "rule_count": { "type": "integer", "minimum": 0 }
      },
      "additionalProperties": true
    },
    "rules": {
      "type": "array",
      "items": { "$ref": "#/definitions/ReplacementRule" }
    }
  },
  "additionalProperties": true
}
```

**Timeout:** 2 seconds

---

#### `replacements.preview`

Preview replacement processing without saving active rules.

**Availability:** Optional (host MUST handle `E_METHOD_NOT_FOUND`)

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "replacements.preview",
  "params": {
    "text": "insert date {{date}}",
    "rules": [],
    "skip_normalize": false,
    "skip_macros": false
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "result": "insert date 2026-02-18",
    "truncated": false,
    "applied_rules_count": 0
  }
}
```

**Parameters Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "properties": {
    "text": { "type": "string" },
    "rules": {
      "type": "array",
      "items": { "$ref": "#/definitions/ReplacementRule" }
    },
    "skip_normalize": { "type": "boolean" },
    "skip_macros": { "type": "boolean" }
  },
  "additionalProperties": false
}
```

**Result Schema (JSON Schema fragment):**
```json
{
  "type": "object",
  "required": ["result", "truncated", "applied_rules_count"],
  "properties": {
    "result": { "type": "string" },
    "truncated": { "type": "boolean" },
    "applied_rules_count": { "type": "integer", "minimum": 0 }
  },
  "additionalProperties": true
}
```

**Timeout:** 2 seconds

---

### Status Methods

#### `status.get`

Get current sidecar status.

**Request:**
```json
{ "jsonrpc": "2.0", "id": 1, "method": "status.get" }
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "state": "idle",
    "model": {
      "model_id": "parakeet-tdt-0.6b-v3",
      "status": "ready"
    }
  }
}
```

**State values:**
- `"idle"`: Ready and waiting
- `"loading_model"`: Loading/downloading model
- `"recording"`: Recording in progress
- `"transcribing"`: Transcription in progress
- `"error"`: Error state

**Optional fields:**
- `detail`: Human-readable status message
- `model`: Current model status object

`idle` may also be reported before any model is loaded:
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "state": "idle"
  }
}
```

**Timeout:** 2 seconds

---

## Notifications

### `event.status_changed`

Emitted when sidecar state changes.

```json
{
  "jsonrpc": "2.0",
  "method": "event.status_changed",
  "params": {
    "state": "loading_model",
    "detail": "Downloading model...",
    "progress": {
      "current": 1073741824,
      "total": 3221225472,
      "unit": "bytes"
    },
    "model": {
      "model_id": "parakeet-tdt-0.6b-v3",
      "status": "downloading"
    }
  }
}
```

**Fields:**
- `state` (required): Current state string
- `detail` (optional): Human-readable message
- `progress` (optional): Progress object for downloads
  - Uses cumulative bytes when `unit: "bytes"`
- `model` (optional): Model status object

---

### `event.audio_level`

Emitted during metering or recording.

```json
{
  "jsonrpc": "2.0",
  "method": "event.audio_level",
  "params": {
    "source": "meter",
    "rms": 0.15,
    "peak": 0.42
  }
}
```

During recording:
```json
{
  "jsonrpc": "2.0",
  "method": "event.audio_level",
  "params": {
    "source": "recording",
    "session_id": "550e8400-e29b-41d4-a716-446655440000",
    "rms": 0.23,
    "peak": 0.67
  }
}
```

**Fields:**
- `source` (required): `"meter"` or `"recording"`
- `session_id` (conditional): Present when `source` is `"recording"`
- `rms` (required): RMS level, normalized float 0.0-1.0
- `peak` (required): Peak level, normalized float 0.0-1.0

---

### `event.transcription_complete`

Emitted when transcription finishes successfully.

```json
{
  "jsonrpc": "2.0",
  "method": "event.transcription_complete",
  "params": {
    "session_id": "550e8400-e29b-41d4-a716-446655440000",
    "text": "Hello, this is a transcribed message.",
    "confidence": 0.95,
    "duration_ms": 1234
  }
}
```

**Fields:**
- `session_id` (required): Session UUID
- `text` (required): Final transcribed text (post-processed, replacements applied)
- `confidence` (optional): Confidence score 0.0-1.0
- `duration_ms` (required): Transcription compute time in milliseconds (NOT audio duration)

---

### `event.transcription_error`

Emitted when transcription fails.

```json
{
  "jsonrpc": "2.0",
  "method": "event.transcription_error",
  "params": {
    "session_id": "550e8400-e29b-41d4-a716-446655440000",
    "kind": "E_TRANSCRIBE",
    "message": "Model inference failed: CUDA out of memory"
  }
}
```

**Fields:**
- `session_id` (required): Session UUID
- `kind` (required): Error kind string (E_*)
- `message` (required): Human-readable error message

---

## ReplacementRule Schema

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | UUID v4 identifier |
| `enabled` | boolean | Whether rule is active |
| `kind` | string | `"literal"` or `"regex"` |
| `pattern` | string | Pattern to match (non-empty) |
| `replacement` | string | Replacement text |
| `word_boundary` | boolean | Match at word boundaries only |
| `case_sensitive` | boolean | Case-sensitive matching |

### Optional Fields

| Field | Type | Description |
|-------|------|-------------|
| `description` | string | Human-readable description |
| `origin` | string | `"user"` or `"preset"` |

### Constraints

| Constraint | Value |
|------------|-------|
| Max rules | 500 |
| Max pattern length | 256 characters |
| Max replacement length | 1,024 characters |
| Max output length | 50,000 characters (truncate with warning) |

### Processing Semantics

**Pipeline order (locked):**
1. Post-process normalization
2. Macro expansion
3. Replacements (apply all in order)

**Execution:**
- Single pass, no recursion
- Rules applied in array order

### Macros (MVP Set)

All macros are deterministic and use local timezone.

| Macro | Output Format | Example |
|-------|---------------|---------|
| `{{date}}` | ISO date | `2026-02-04` |
| `{{time}}` | 24-hour time | `14:30:45` |
| `{{datetime}}` | Date and time | `2026-02-04 14:30:45` |

---

## Timeout Policy

### Default Timeouts (Rust Host)

| Method | Timeout | Recovery |
|--------|---------|----------|
| `system.ping` | 1s | 1 retry |
| `system.info` | 2s | 1 retry |
| `system.shutdown` | 2s | - |
| `audio.list_devices` | 2s | 1 retry |
| `audio.set_device` | 2s | 1 retry |
| `audio.meter_start` | 2s | 1 retry |
| `audio.meter_stop` | 2s | 1 retry |
| `audio.meter_status` *(optional)* | 2s | 1 retry |
| `model.get_status` | 2s | 1 retry |
| `model.download` *(optional)* | 20 min | Fatal |
| `model.purge_cache` | 10s | - |
| `asr.initialize` | 20 min | Fatal |
| `asr.status` *(optional)* | 2s | 1 retry |
| `asr.transcribe` *(optional)* | 30s | 1 retry |
| `recording.start` | 2s | 1 retry |
| `recording.stop` | 2s | 1 retry |
| `recording.cancel` | 2s | 1 retry |
| `recording.status` *(optional)* | 2s | 1 retry |
| `replacements.set_rules` | 2s | 1 retry |
| `replacements.get_rules` *(optional)* | 2s | 1 retry |
| `replacements.get_presets` *(optional)* | 2s | 1 retry |
| `replacements.get_preset_rules` *(optional)* | 2s | 1 retry |
| `replacements.preview` *(optional)* | 2s | 1 retry |
| `status.get` | 2s | 1 retry |

### Timeout Handling

- **Short methods (≤10s):** Recoverable with 1 retry
- **`asr.initialize` timeout:** Fatal - sidecar enters error state
  - **Remediation:** Restart sidecar process

---

## Test Requirements

1. **Parse tests:** Unit tests MUST parse all examples in `IPC_V1_EXAMPLES.jsonl`
2. **Fuzz tests:** JSON-RPC parser MUST be fuzz-tested with malformed inputs
3. **Error code validation:** Verify all error codes are in valid ranges
4. **Timeout tests:** Verify timeout handling behavior

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-02-04 | Initial locked specification |
