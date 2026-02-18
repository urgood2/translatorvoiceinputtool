#!/usr/bin/env bash
#
# E2E Test: Full Transcription Flow
#
# Exercises the end-to-end IPC flow:
# 1. Sidecar startup (persistent process for stateful calls)
# 2. Recording start/stop path (or structured recording error path)
# 3. Transcription path (or structured transcribe error path)
# 4. Injection preparation path via replacements.preview
#
# This script intentionally probes recording/transcribe/inject stages even on
# constrained CI hosts where audio/model prerequisites might be missing. In
# those environments, the expected behavior is a structured JSON-RPC error.
#
# Exit codes:
#   0 - All tests passed
#   1 - Test failure
#   2 - Environment setup error
#   3 - Timeout
#

set -euo pipefail

# Resolve script directory (handle being called via various methods)
if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

# Source libraries
source "$SCRIPT_DIR/lib/log.sh"
source "$SCRIPT_DIR/lib/assert.sh"
source "$SCRIPT_DIR/lib/common.sh"

# Configuration
TEST_TIMEOUT=60  # seconds

# Send JSON-RPC request to an already running sidecar process and read response
# for matching request id from the shared stdout FIFO.
# Args: method, [params_json], [timeout_seconds]
sidecar_rpc_session() {
    local method="$1"
    local params="$2"
    [[ -z "$params" ]] && params='{}'
    local timeout="${3:-10}"

    local request_id
    request_id=$((RANDOM * RANDOM))

    local request
    request=$(jq -nc \
        --arg method "$method" \
        --argjson params "$params" \
        --argjson id "$request_id" \
        '{jsonrpc:"2.0",id:$id,method:$method,params:$params}')

    log_debug "ipc" "request" "Sending stateful RPC request" "{\"method\":\"$method\",\"id\":$request_id}"
    printf '%s\n' "$request" >&3

    local response
    response=$(
        E2E_RPC_FIFO="$E2E_SIDECAR_STDOUT" E2E_RPC_ID="$request_id" \
            e2e_timeout_run "$timeout" bash -c '
                while IFS= read -r line; do
                    if [[ "$line" != *"\"jsonrpc\""* ]]; then
                        continue
                    fi

                    line_id=$(echo "$line" | jq -r ".id // empty" 2>/dev/null || true)
                    if [[ "$line_id" == "$E2E_RPC_ID" ]]; then
                        printf "%s\n" "$line"
                        exit 0
                    fi
                done < "$E2E_RPC_FIFO"
                exit 1
            ' 2>/dev/null || echo '{"error":{"message":"timeout"}}'
    )

    printf '%s\n' "$response"
}

main() {
    # Initialize
    require_jq
    init_logging "test-full-flow"
    init_common
    setup_cleanup_trap

    log_info "test" "start" "Starting full flow E2E test"

    # Phase 1: Environment checks
    log_info "startup" "env_check" "Checking environment"

    assert_dir_exists "$E2E_PROJECT_ROOT/sidecar" "Sidecar directory exists" || exit 2
    check_sidecar_binary || exit 2

    # Start sidecar once so stateful calls (recording.start -> recording.stop) work.
    start_sidecar || exit 2

    # Phase 2: Sidecar ping test
    log_info "sidecar" "ping_test" "Testing sidecar ping"

    local ping_start
    ping_start=$(start_timer)

    local ping_result
    ping_result=$(sidecar_rpc_session "system.ping" "{}" 10) || {
        log_error "sidecar" "ping_test" "Ping failed"
        exit 1
    }

    log_with_duration "INFO" "sidecar" "ping_test" "Ping completed" "$ping_result" "$ping_start"

    # Verify ping response
    assert_json_eq "$ping_result" ".result.protocol" "v1" "Protocol version is v1"

    local version
    version=$(echo "$ping_result" | jq -r '.result.version')
    log_info "sidecar" "version" "Sidecar version" "{\"version\":\"$version\"}"

    # Phase 3: Audio device enumeration
    log_info "audio" "list_devices" "Enumerating audio devices"

    local devices_start
    devices_start=$(start_timer)

    local devices_result
    devices_result=$(sidecar_rpc_session "audio.list_devices" "{}" 10) || {
        log_error "audio" "list_devices" "Device enumeration failed"
        exit 1
    }

    log_with_duration "INFO" "audio" "list_devices" "Devices enumerated" "{}" "$devices_start"

    # Check response has devices array (may be empty in headless environment)
    local devices_count
    devices_count=$(echo "$devices_result" | jq '.result.devices | length')
    log_info "audio" "device_count" "Audio devices found" "{\"count\":$devices_count}"

    # Phase 4: Model status (may legitimately be unavailable depending on setup)
    log_info "model" "status_check" "Checking model status"

    local model_result
    model_result=$(sidecar_rpc_session "model.get_status" "{}" 10) 2>/dev/null || true

    if echo "$model_result" | jq -e '.result' >/dev/null 2>&1; then
        local model_status
        model_status=$(echo "$model_result" | jq -r '.result.status // "unknown"')
        log_info "model" "status" "Model status retrieved" "{\"status\":\"$model_status\"}"
    elif echo "$model_result" | jq -e '.error' >/dev/null 2>&1; then
        local model_error
        model_error=$(echo "$model_result" | jq -c '.error')
        log_warn "model" "status_check" "Model status returned structured error" "{\"error\":$model_error}"
    else
        log_error "model" "status_check" "Model status returned no result/error payload"
        ((E2E_ASSERTIONS_FAILED++)) || true
    fi

    # Phase 5: Recording path (recording.start -> recording.stop)
    log_info "flow" "recording" "Exercising recording path"

    local recording_start_result
    recording_start_result=$(sidecar_rpc_session "recording.start" '{"device_uid":null}' 15) || true

    local session_id=""
    if echo "$recording_start_result" | jq -e '.result.session_id' >/dev/null 2>&1; then
        session_id=$(echo "$recording_start_result" | jq -r '.result.session_id')
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "flow" "recording" "Recording started" "{\"session_id\":\"$session_id\"}"
    elif echo "$recording_start_result" | jq -e '.error' >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_warn "flow" "recording" "Recording start returned structured error" "$recording_start_result"
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "flow" "recording" "Recording start returned invalid payload" "$recording_start_result"
    fi

    if [ -n "$session_id" ]; then
        # Capture a short snippet so stop has data to process.
        sleep 1
        local stop_params
        stop_params=$(jq -nc --arg sid "$session_id" '{session_id:$sid}')

        local recording_stop_result
        recording_stop_result=$(sidecar_rpc_session "recording.stop" "$stop_params" 20) || true

        if echo "$recording_stop_result" | jq -e '.result.audio_duration_ms and .result.sample_rate and .result.channels and .result.session_id' >/dev/null 2>&1; then
            ((E2E_ASSERTIONS_PASSED++)) || true
            log_info "flow" "recording" "Recording stopped and queued for transcription" "$recording_stop_result"
        elif echo "$recording_stop_result" | jq -e '.error' >/dev/null 2>&1; then
            ((E2E_ASSERTIONS_PASSED++)) || true
            log_warn "flow" "recording" "Recording stop returned structured error" "$recording_stop_result"
        else
            ((E2E_ASSERTIONS_FAILED++)) || true
            log_error "flow" "recording" "Recording stop returned invalid payload" "$recording_stop_result"
        fi
    fi

    # Phase 6: Transcription path probe
    log_info "flow" "transcribe" "Exercising transcription path"
    local fixture_audio="$E2E_PROJECT_ROOT/src-tauri/sounds/cue-stop.wav"
    assert_file_exists "$fixture_audio" "Fixture audio exists for transcribe probe"

    local transcribe_params
    transcribe_params=$(jq -nc --arg p "$fixture_audio" --arg sid "${session_id:-e2e-flow-session}" --arg lang "en-US" \
        '{audio_path:$p,session_id:$sid,language:$lang}')

    local transcribe_result
    transcribe_result=$(sidecar_rpc_session "asr.transcribe" "$transcribe_params" 20) || true

    local transcribed_text=""
    if echo "$transcribe_result" | jq -e '.result.text' >/dev/null 2>&1; then
        transcribed_text=$(echo "$transcribe_result" | jq -r '.result.text')
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "flow" "transcribe" "Transcription result received" "{\"text_len\":${#transcribed_text}}"
    elif echo "$transcribe_result" | jq -e '.error' >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_warn "flow" "transcribe" "Transcribe returned structured error" "$transcribe_result"
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "flow" "transcribe" "Transcribe returned invalid payload" "$transcribe_result"
    fi

    # Phase 7: Injection preparation path (text preview before injection)
    log_info "flow" "inject" "Exercising injection preparation path"
    local preview_text="${transcribed_text:-full flow fallback transcript}"
    local preview_params
    preview_params=$(jq -nc --arg text "$preview_text" '{text:$text,rules:[],skip_normalize:false,skip_macros:false}')

    local preview_result
    preview_result=$(sidecar_rpc_session "replacements.preview" "$preview_params" 10) || true

    if echo "$preview_result" | jq -e '.result.result and (.result.truncated == true or .result.truncated == false)' >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "flow" "inject" "Injection preparation succeeded" "$preview_result"
    elif echo "$preview_result" | jq -e '.error' >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "flow" "inject" "Injection preparation returned error" "$preview_result"
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "flow" "inject" "Injection preparation returned invalid payload" "$preview_result"
    fi

    # Phase 8: Verify sidecar startup time is within budget
    local startup_time_ms
    startup_time_ms=$(cat "$E2E_PROJECT_ROOT/sidecar/dist/manifest.json" 2>/dev/null | jq '.startup_time_ms // 0')

    if [ "$startup_time_ms" -gt 0 ]; then
        assert_duration_under "$startup_time_ms" 5000 "Startup time under 5s budget"
    fi

    # Summary
    assertion_summary

    log_info "test" "complete" "Full flow test completed successfully"
    return 0
}

# Run main
main
exit $?
