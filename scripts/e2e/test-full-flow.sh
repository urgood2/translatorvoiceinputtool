#!/usr/bin/env bash
#
# E2E Test: Full Dictation Flow
#
# Required sequence:
# 1) Verify sidecar binary/script availability
# 2) Start sidecar subprocess
# 3) system.ping
# 4) asr.initialize (skip=77 if default model unavailable)
# 5) recording.start with generated session_id
# 6) Provide short synthetic audio (generated sine wave playback)
# 7) recording.stop and wait for event.transcription_complete
# 8) Verify event session_id matches
# 9) Verify event text is non-empty string
# 10) Verify end-to-end time < 30s
# 11) system.shutdown
#
# Output:
# - Human log to stdout and logs/e2e/test-full-flow-TIMESTAMP.log
# - Exit 0 success, 1 failure, 77 skipped (no model available)

set -euo pipefail

if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

source "$SCRIPT_DIR/lib/common.sh"

STEPS_TOTAL=11
FLOW_TIMEOUT_MS=30000
RPC_DEFAULT_TIMEOUT=12
NOTIFICATION_TIMEOUT=20

FULL_FLOW_LOG_FILE=""
FLOW_START_MS=0
SESSION_ID=""
SYNTH_AUDIO_FILE=""
SIDEcar_STARTED=0
SHUTDOWN_ATTEMPTED=0

LAST_ERROR_MESSAGE=""

declare -a RPC_HISTORY=()
declare -a EVENT_HISTORY=()
declare -a RAW_HISTORY=()

timestamp_human() {
    date +"%Y-%m-%d %H:%M:%S"
}

now_ms() {
    date +%s%3N
}

emit_line() {
    local line="$1"
    echo "$line"
    if [[ -n "$FULL_FLOW_LOG_FILE" ]]; then
        echo "$line" >> "$FULL_FLOW_LOG_FILE"
    fi
}

truncate_text() {
    local text="$1"
    local max_len="${2:-600}"
    if (( ${#text} > max_len )); then
        printf '%s...(truncated)' "${text:0:max_len}"
    else
        printf '%s' "$text"
    fi
}

step_log() {
    local step="$1"
    local message="$2"
    emit_line "[$(timestamp_human)] [STEP ${step}/${STEPS_TOTAL}] ${message}"
}

record_history() {
    local entry="$1"
    RPC_HISTORY+=("$entry")
}

record_event() {
    local entry="$1"
    EVENT_HISTORY+=("$entry")
}

record_raw() {
    local entry="$1"
    RAW_HISTORY+=("$entry")
}

should_skip_no_model() {
    local response="$1"
    local error_kind
    local error_message

    error_kind=$(echo "$response" | jq -r '.error.data.kind // ""' 2>/dev/null | tr '[:upper:]' '[:lower:]' || true)
    error_message=$(echo "$response" | jq -r '.error.message // ""' 2>/dev/null | tr '[:upper:]' '[:lower:]' || true)

    if [[ "$error_kind" == "e_model_not_found" ]] || [[ "$error_kind" == "e_model_load" ]] || [[ "$error_kind" == "e_not_ready" ]]; then
        return 0
    fi

    if [[ "$error_message" == *"model"* ]] && (
        [[ "$error_message" == *"not found"* ]] ||
        [[ "$error_message" == *"missing"* ]] ||
        [[ "$error_message" == *"not initialized"* ]]
    ); then
        return 0
    fi

    return 1
}

sidecar_rpc_session() {
    local method="$1"
    local params="$2"
    [[ -z "$params" ]] && params='{}'
    local timeout="${3:-$RPC_DEFAULT_TIMEOUT}"

    local request_id
    request_id=$((RANDOM * RANDOM))

    local request
    request=$(jq -nc \
        --arg method "$method" \
        --argjson params "$params" \
        --argjson id "$request_id" \
        '{jsonrpc:"2.0",id:$id,method:$method,params:$params}')

    local short_request
    short_request=$(truncate_text "$request" 700)
    emit_line "[$(timestamp_human)] [RPC][REQ] method=${method} id=${request_id} payload=${short_request}"

    record_history "$(timestamp_human) REQUEST method=${method} id=${request_id} payload=${short_request}"

    printf '%s\n' "$request" >&3

    local deadline=$((SECONDS + timeout))
    local line=""

    while (( SECONDS < deadline )); do
        local wait_s=$((deadline - SECONDS))
        (( wait_s <= 0 )) && break

        if IFS= read -r -u 4 -t "$wait_s" line; then
            if [[ "$line" != *'"jsonrpc"'* ]]; then
                local short_raw
                short_raw=$(truncate_text "$line" 300)
                record_raw "$(timestamp_human) RAW ${short_raw}"
                continue
            fi

            local line_id
            local line_method
            line_id=$(echo "$line" | jq -r '.id // empty' 2>/dev/null || true)
            line_method=$(echo "$line" | jq -r '.method // empty' 2>/dev/null || true)

            if [[ -n "$line_method" ]]; then
                local line_session
                line_session=$(echo "$line" | jq -r '.params.session_id // empty' 2>/dev/null || true)
                local short_event
                short_event=$(truncate_text "$line" 700)
                emit_line "[$(timestamp_human)] [RPC][NOTIFY] method=${line_method} session_id=${line_session:-none} payload=${short_event}"
                record_event "$(timestamp_human) EVENT method=${line_method} session_id=${line_session:-none} payload=${short_event}"
            fi

            if [[ "$line_id" == "$request_id" ]]; then
                local short_response
                short_response=$(truncate_text "$line" 700)
                emit_line "[$(timestamp_human)] [RPC][RES] method=${method} id=${request_id} payload=${short_response}"
                record_history "$(timestamp_human) RESPONSE method=${method} id=${request_id} payload=${short_response}"
                printf '%s\n' "$line"
                return 0
            fi
        else
            break
        fi
    done

    local timeout_response='{"error":{"message":"timeout"}}'
    emit_line "[$(timestamp_human)] [RPC][TIMEOUT] method=${method} id=${request_id} timeout_s=${timeout}"
    record_history "$(timestamp_human) TIMEOUT method=${method} id=${request_id} timeout_s=${timeout}"
    printf '%s\n' "$timeout_response"
    return 1
}

wait_for_transcription_notification() {
    local session_id="$1"
    local timeout="$2"

    local deadline=$((SECONDS + timeout))
    local line=""

    while (( SECONDS < deadline )); do
        local wait_s=$((deadline - SECONDS))
        (( wait_s <= 0 )) && break

        if IFS= read -r -u 4 -t "$wait_s" line; then
            if [[ "$line" != *'"jsonrpc"'* ]]; then
                local short_raw
                short_raw=$(truncate_text "$line" 300)
                record_raw "$(timestamp_human) RAW ${short_raw}"
                continue
            fi

            local line_method
            local line_session
            line_method=$(echo "$line" | jq -r '.method // empty' 2>/dev/null || true)
            line_session=$(echo "$line" | jq -r '.params.session_id // empty' 2>/dev/null || true)

            if [[ -n "$line_method" ]]; then
                local short_event
                short_event=$(truncate_text "$line" 700)
                emit_line "[$(timestamp_human)] [RPC][NOTIFY] method=${line_method} session_id=${line_session:-none} payload=${short_event}"
                record_event "$(timestamp_human) EVENT method=${line_method} session_id=${line_session:-none} payload=${short_event}"
            fi

            if [[ "$line_method" == "event.transcription_complete" ]] && [[ "$line_session" == "$session_id" ]]; then
                printf '%s\n' "$line"
                return 0
            fi

            if [[ "$line_method" == "event.transcription_error" ]] && [[ "$line_session" == "$session_id" ]]; then
                printf '%s\n' "$line"
                return 2
            fi
        else
            break
        fi
    done

    return 1
}

dump_failure_context() {
    emit_line "[$(timestamp_human)] [FAILURE] ${LAST_ERROR_MESSAGE}"

    emit_line "[$(timestamp_human)] [FAILURE] Last 5 JSON-RPC exchanges:"
    local total_rpc=${#RPC_HISTORY[@]}
    if (( total_rpc == 0 )); then
        emit_line "[$(timestamp_human)] [FAILURE] (none)"
    else
        local from_rpc=$(( total_rpc > 5 ? total_rpc - 5 : 0 ))
        local i
        for (( i=from_rpc; i<total_rpc; i++ )); do
            emit_line "[$(timestamp_human)] [FAILURE][RPC] ${RPC_HISTORY[$i]}"
        done
    fi

    emit_line "[$(timestamp_human)] [FAILURE] Last 5 notification events:"
    local total_events=${#EVENT_HISTORY[@]}
    if (( total_events == 0 )); then
        emit_line "[$(timestamp_human)] [FAILURE] (none)"
    else
        local from_events=$(( total_events > 5 ? total_events - 5 : 0 ))
        local j
        for (( j=from_events; j<total_events; j++ )); do
            emit_line "[$(timestamp_human)] [FAILURE][EVENT] ${EVENT_HISTORY[$j]}"
        done
    fi

    if (( SIDEcar_STARTED == 1 )); then
        local status_snapshot
        status_snapshot=$(sidecar_rpc_session "status.get" "{}" 5 || true)
        emit_line "[$(timestamp_human)] [FAILURE] Current sidecar status snapshot: $(truncate_text "$status_snapshot" 700)"
    fi
}

fail_test() {
    LAST_ERROR_MESSAGE="$1"
    dump_failure_context
    return 1
}

generate_and_play_synthetic_audio() {
    local output_wav="$1"

    python3 - "$output_wav" <<'PY'
import math
import sys
import wave

import numpy as np

output_path = sys.argv[1]
sample_rate = 16000
seconds = 1.2
frequency_hz = 440.0
amplitude = 0.35

sample_count = int(sample_rate * seconds)
t = np.arange(sample_count, dtype=np.float32) / sample_rate
audio = (amplitude * np.sin(2.0 * math.pi * frequency_hz * t)).astype(np.float32)

pcm16 = np.clip(audio * 32767.0, -32768, 32767).astype(np.int16)
with wave.open(output_path, "wb") as wav:
    wav.setnchannels(1)
    wav.setsampwidth(2)
    wav.setframerate(sample_rate)
    wav.writeframes(pcm16.tobytes())

# Attempt playback to feed microphone path in end-to-end environments.
try:
    import sounddevice as sd
except Exception as exc:
    print(f"sounddevice unavailable: {exc}", file=sys.stderr)
    sys.exit(2)

try:
    sd.play(audio, sample_rate)
    sd.wait()
except Exception as exc:
    print(f"audio playback failed: {exc}", file=sys.stderr)
    sys.exit(3)

print(output_path)
PY
}

cleanup() {
    # Prefer clean shutdown via step 11 first; stop_sidecar is best-effort fallback.
    stop_sidecar || true

    if [[ -n "${SYNTH_AUDIO_FILE:-}" ]]; then
        rm -f "$SYNTH_AUDIO_FILE" || true
    fi
}

main() {
    trap cleanup EXIT

    require_jq
    init_common

    mkdir -p "$E2E_PROJECT_ROOT/logs/e2e"
    local timestamp
    timestamp=$(date -u +"%Y%m%dT%H%M%S")
    FULL_FLOW_LOG_FILE="$E2E_PROJECT_ROOT/logs/e2e/test-full-flow-${timestamp}.log"
    : > "$FULL_FLOW_LOG_FILE"

    FLOW_START_MS=$(now_ms)

    # STEP 1/11: Verify sidecar availability
    step_log 1 "Checking sidecar availability"
    if ! check_sidecar_binary >/dev/null 2>&1; then
        emit_line "[$(timestamp_human)] [ERROR] Sidecar binary unavailable at $E2E_SIDECAR_BIN"
        return 1
    fi

    # STEP 2/11: Start sidecar subprocess
    step_log 2 "Starting sidecar subprocess"
    if ! start_sidecar; then
        return 1
    fi
    SIDEcar_STARTED=1
    exec 4<"$E2E_SIDECAR_STDOUT"

    # STEP 3/11: system.ping
    step_log 3 "Sending system.ping"
    local ping_response
    ping_response=$(sidecar_rpc_session "system.ping" "{}" "$RPC_DEFAULT_TIMEOUT") || {
        fail_test "system.ping request failed"
        return 1
    }
    if ! echo "$ping_response" | jq -e '(.result.protocol == "v1")' >/dev/null 2>&1; then
        fail_test "system.ping response invalid"
        return 1
    fi

    # STEP 4/11: asr.initialize with default model
    step_log 4 "Initializing ASR with default model"
    local init_response
    init_response=$(sidecar_rpc_session "asr.initialize" "{}" 20) || true
    if echo "$init_response" | jq -e '.result.status == "ready"' >/dev/null 2>&1; then
        emit_line "[$(timestamp_human)] [INFO] ASR initialized successfully"
    elif echo "$init_response" | jq -e '.error' >/dev/null 2>&1; then
        if should_skip_no_model "$init_response"; then
            emit_line "[$(timestamp_human)] [SKIP] Default model unavailable for asr.initialize; skipping full dictation flow"

            step_log 5 "Skipped: recording.start (no model available)"
            step_log 6 "Skipped: synthetic audio playback (no model available)"
            step_log 7 "Skipped: recording.stop + wait transcription_complete (no model available)"
            step_log 8 "Skipped: session_id verification (no model available)"
            step_log 9 "Skipped: transcript non-empty verification (no model available)"
            step_log 10 "Skipped: duration budget check (no model available)"

            # STEP 11/11: system.shutdown
            step_log 11 "Sending system.shutdown"
            SHUTDOWN_ATTEMPTED=1
            sidecar_rpc_session "system.shutdown" "{}" 8 >/dev/null || true
            emit_line "[$(timestamp_human)] [RESULT] SKIPPED (exit 77)"
            return 77
        fi

        fail_test "asr.initialize returned non-skip error"
        return 1
    else
        fail_test "asr.initialize returned invalid response"
        return 1
    fi

    # STEP 5/11: recording.start with generated session_id
    step_log 5 "Starting recording session"
    SESSION_ID="e2e-dictation-$(date +%s)-$RANDOM"
    local start_params
    start_params=$(jq -nc --arg sid "$SESSION_ID" '{session_id:$sid,device_uid:null}')

    local start_response
    start_response=$(sidecar_rpc_session "recording.start" "$start_params" 15) || {
        fail_test "recording.start request failed"
        return 1
    }

    if ! echo "$start_response" | jq -e --arg sid "$SESSION_ID" '.result.session_id == $sid' >/dev/null 2>&1; then
        fail_test "recording.start response missing expected session_id"
        return 1
    fi

    # STEP 6/11: provide synthetic audio
    step_log 6 "Generating and playing synthetic sine-wave audio"
    SYNTH_AUDIO_FILE="$E2E_PROJECT_ROOT/logs/e2e/test-full-flow-${SESSION_ID}.wav"
    if ! generate_and_play_synthetic_audio "$SYNTH_AUDIO_FILE" >/dev/null; then
        fail_test "failed to generate/play synthetic audio"
        return 1
    fi

    # STEP 7/11: recording.stop and wait for event.transcription_complete
    step_log 7 "Stopping recording and waiting for transcription_complete"
    local stop_params
    stop_params=$(jq -nc --arg sid "$SESSION_ID" '{session_id:$sid}')

    local stop_response
    stop_response=$(sidecar_rpc_session "recording.stop" "$stop_params" 20) || {
        fail_test "recording.stop request failed"
        return 1
    }

    if ! echo "$stop_response" | jq -e --arg sid "$SESSION_ID" '.result.session_id == $sid' >/dev/null 2>&1; then
        fail_test "recording.stop response missing expected session_id"
        return 1
    fi

    local notification
    if notification=$(wait_for_transcription_notification "$SESSION_ID" "$NOTIFICATION_TIMEOUT"); then
        :
    else
        local wait_code=$?
        if [[ "$wait_code" -eq 2 ]]; then
            fail_test "received event.transcription_error for session ${SESSION_ID}"
            return 1
        fi
        fail_test "timed out waiting for event.transcription_complete for session ${SESSION_ID}"
        return 1
    fi

    # STEP 8/11: verify session_id matches
    step_log 8 "Validating transcription_complete session_id"
    if ! echo "$notification" | jq -e --arg sid "$SESSION_ID" '.params.session_id == $sid' >/dev/null 2>&1; then
        fail_test "event.transcription_complete session_id mismatch"
        return 1
    fi

    # STEP 9/11: verify non-empty text field
    step_log 9 "Validating transcription_complete text field"
    if ! echo "$notification" | jq -e '(.params.text | type == "string") and ((.params.text | length) > 0)' >/dev/null 2>&1; then
        fail_test "event.transcription_complete text is empty or not a string"
        return 1
    fi

    # STEP 10/11: verify full flow duration < 30s
    step_log 10 "Checking end-to-end duration budget (<30s)"
    local elapsed_ms
    elapsed_ms=$(( $(now_ms) - FLOW_START_MS ))
    if (( elapsed_ms >= FLOW_TIMEOUT_MS )); then
        fail_test "full dictation flow exceeded ${FLOW_TIMEOUT_MS}ms (${elapsed_ms}ms)"
        return 1
    fi
    emit_line "[$(timestamp_human)] [INFO] Full flow completed in ${elapsed_ms}ms"

    # STEP 11/11: system.shutdown
    step_log 11 "Sending system.shutdown"
    SHUTDOWN_ATTEMPTED=1
    local shutdown_response
    shutdown_response=$(sidecar_rpc_session "system.shutdown" "{}" 10) || {
        fail_test "system.shutdown request failed"
        return 1
    }

    if ! echo "$shutdown_response" | jq -e '.result.status == "shutting_down" or .result.ok == true' >/dev/null 2>&1; then
        fail_test "system.shutdown returned invalid response"
        return 1
    fi

    emit_line "[$(timestamp_human)] [RESULT] PASS"
    return 0
}

main
exit $?
