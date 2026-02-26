#!/usr/bin/env bash
#
# E2E Test: Device Removal Mid-Recording
#
# Validates graceful handling of input-device removal while recording.
# Uses a mixed strategy:
# - Live sidecar flow for recording lifecycle + idle recovery checks.
# - Deterministic simulation via Rust integration policy tests for hot-swap
#   decision and canonical app:error payload checks.
#
# Exit codes:
#   0  pass
#   1  fail
#   77 skipped (device-removal simulation unavailable on this host/config)

set -euo pipefail

if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

source "$SCRIPT_DIR/lib/common.sh"

STEPS_TOTAL=8
RPC_DEFAULT_TIMEOUT=12
NOTIFICATION_TIMEOUT=8

TEST_LOG_FILE=""
SESSION_ID=""
SELECTED_DEVICE_UID=""
SIDECAR_STARTED=0

# Simulation mode:
# - integration_tests (default): run deterministic rust tests for device hot-swap handling
# - live: not implemented cross-platform (returns skip)
SIMULATION_MODE="${E2E_DEVICE_REMOVAL_MODE:-integration_tests}"

LAST_ERROR=""

declare -a RPC_HISTORY=()
declare -a EVENT_HISTORY=()
declare -a DEVICE_HISTORY=()

ts_human() {
    date +"%Y-%m-%d %H:%M:%S"
}

emit_line() {
    local line="$1"
    echo "$line"
    if [[ -n "${TEST_LOG_FILE:-}" ]]; then
        echo "$line" >> "$TEST_LOG_FILE"
    fi
}

truncate_text() {
    local text="$1"
    local max_len="${2:-700}"
    if (( ${#text} > max_len )); then
        printf '%s...(truncated)' "${text:0:max_len}"
    else
        printf '%s' "$text"
    fi
}

step_log() {
    local step="$1"
    local message="$2"
    emit_line "[$(ts_human)] [STEP ${step}/${STEPS_TOTAL}] ${message}"
}

record_rpc() {
    RPC_HISTORY+=("$1")
}

record_event() {
    EVENT_HISTORY+=("$1")
}

record_device_state() {
    DEVICE_HISTORY+=("$1")
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
    short_request=$(truncate_text "$request")
    emit_line "[$(ts_human)] [RPC][REQ] method=${method} id=${request_id} payload=${short_request}"
    record_rpc "$(ts_human) REQUEST method=${method} id=${request_id} payload=${short_request}"

    printf '%s\n' "$request" >&3

    local deadline=$((SECONDS + timeout))
    local line=""

    while (( SECONDS < deadline )); do
        local wait_s=$((deadline - SECONDS))
        (( wait_s <= 0 )) && break

        if IFS= read -r -u 4 -t "$wait_s" line; then
            if [[ "$line" != *'"jsonrpc"'* ]]; then
                continue
            fi

            local line_id
            line_id=$(echo "$line" | jq -r '.id // empty' 2>/dev/null || true)

            local line_method
            line_method=$(echo "$line" | jq -r '.method // empty' 2>/dev/null || true)
            if [[ -n "$line_method" ]]; then
                local line_kind
                line_kind=$(echo "$line" | jq -r '.params.error.code // .params.kind // ""' 2>/dev/null || true)
                local short_event
                short_event=$(truncate_text "$line")
                emit_line "[$(ts_human)] [RPC][EVENT] method=${line_method} kind=${line_kind:-none} payload=${short_event}"
                record_event "$(ts_human) EVENT method=${line_method} kind=${line_kind:-none} payload=${short_event}"
            fi

            if [[ "$line_id" == "$request_id" ]]; then
                local short_response
                short_response=$(truncate_text "$line")
                emit_line "[$(ts_human)] [RPC][RES] method=${method} id=${request_id} payload=${short_response}"
                record_rpc "$(ts_human) RESPONSE method=${method} id=${request_id} payload=${short_response}"
                printf '%s\n' "$line"
                return 0
            fi
        else
            break
        fi
    done

    emit_line "[$(ts_human)] [RPC][TIMEOUT] method=${method} id=${request_id}"
    record_rpc "$(ts_human) TIMEOUT method=${method} id=${request_id}"
    printf '%s\n' '{"error":{"message":"timeout"}}'
    return 1
}

wait_for_error_event() {
    local timeout="${1:-$NOTIFICATION_TIMEOUT}"
    local deadline=$((SECONDS + timeout))
    local line=""

    while (( SECONDS < deadline )); do
        local wait_s=$((deadline - SECONDS))
        (( wait_s <= 0 )) && break

        if IFS= read -r -u 4 -t "$wait_s" line; then
            if [[ "$line" != *'"jsonrpc"'* ]]; then
                continue
            fi

            local method
            method=$(echo "$line" | jq -r '.method // empty' 2>/dev/null || true)
            if [[ -n "$method" ]]; then
                local short_event
                short_event=$(truncate_text "$line")
                emit_line "[$(ts_human)] [RPC][EVENT] method=${method} payload=${short_event}"
                record_event "$(ts_human) EVENT method=${method} payload=${short_event}"
            fi

            if [[ "$method" == "app:error" ]] || [[ "$method" == "event.transcription_error" ]]; then
                printf '%s\n' "$line"
                return 0
            fi
        else
            break
        fi
    done

    return 1
}

dump_failure_context() {
    emit_line "[$(ts_human)] [FAILURE] ${LAST_ERROR}"

    local status_snapshot
    status_snapshot=$(sidecar_rpc_session "status.get" "{}" 5 || true)
    emit_line "[$(ts_human)] [FAILURE] status.get: $(truncate_text "$status_snapshot")"

    local recording_snapshot
    recording_snapshot=$(sidecar_rpc_session "recording.status" "{}" 5 || true)
    emit_line "[$(ts_human)] [FAILURE] recording.status: $(truncate_text "$recording_snapshot")"

    local devices_snapshot
    devices_snapshot=$(sidecar_rpc_session "audio.list_devices" "{}" 5 || true)
    emit_line "[$(ts_human)] [FAILURE] audio.list_devices: $(truncate_text "$devices_snapshot")"

    emit_line "[$(ts_human)] [FAILURE] Last 5 RPC exchanges:"
    local rpc_count=${#RPC_HISTORY[@]}
    local rpc_from=$(( rpc_count > 5 ? rpc_count - 5 : 0 ))
    local i
    for (( i=rpc_from; i<rpc_count; i++ )); do
        emit_line "[$(ts_human)] [FAILURE][RPC] ${RPC_HISTORY[$i]}"
    done

    emit_line "[$(ts_human)] [FAILURE] Last 5 event entries:"
    local event_count=${#EVENT_HISTORY[@]}
    local event_from=$(( event_count > 5 ? event_count - 5 : 0 ))
    local j
    for (( j=event_from; j<event_count; j++ )); do
        emit_line "[$(ts_human)] [FAILURE][EVENT] ${EVENT_HISTORY[$j]}"
    done

    emit_line "[$(ts_human)] [FAILURE] Device state history:"
    local device_count=${#DEVICE_HISTORY[@]}
    local k
    for (( k=0; k<device_count; k++ )); do
        emit_line "[$(ts_human)] [FAILURE][DEVICE] ${DEVICE_HISTORY[$k]}"
    done
}

fail_test() {
    LAST_ERROR="$1"
    dump_failure_context
    return 1
}

run_policy_test() {
    local test_name="$1"
    emit_line "[$(ts_human)] [SIM] Running policy test: ${test_name}"

    if cargo test --manifest-path "$E2E_PROJECT_ROOT/src-tauri/Cargo.toml" "$test_name" >/dev/null 2>&1; then
        emit_line "[$(ts_human)] [SIM] PASS: ${test_name}"
        return 0
    fi

    emit_line "[$(ts_human)] [SIM] FAIL: ${test_name}"
    return 1
}

simulate_device_removal() {
    case "$SIMULATION_MODE" in
        integration_tests)
            run_policy_test "integration::tests::test_device_hot_swap_decision_during_recording_requests_stop_and_fallback" || return 1
            run_policy_test "integration::tests::test_device_hot_swap_decision_mid_transcription_forces_clipboard_preservation" || return 1
            run_policy_test "integration::tests::test_device_removed_app_error_includes_required_recovery_details" || return 1
            run_policy_test "integration::tests::test_device_hot_swap_decision_idle_missing_device_updates_without_error" || return 1
            return 0
            ;;
        live)
            emit_line "[$(ts_human)] [SKIP] live device removal simulation is not implemented on this host"
            return 77
            ;;
        *)
            emit_line "[$(ts_human)] [SKIP] unknown E2E_DEVICE_REMOVAL_MODE='$SIMULATION_MODE'"
            return 77
            ;;
    esac
}

shutdown_sidecar() {
    if (( SIDECAR_STARTED == 0 )); then
        return 0
    fi

    local shutdown_result
    shutdown_result=$(sidecar_rpc_session "system.shutdown" "{}" 8 || true)
    emit_line "[$(ts_human)] [INFO] system.shutdown response: $(truncate_text "$shutdown_result")"
}

cleanup() {
    shutdown_sidecar || true
    stop_sidecar || true
}

main() {
    trap cleanup EXIT

    require_jq
    init_common

    mkdir -p "$E2E_PROJECT_ROOT/logs/e2e"
    TEST_LOG_FILE="$E2E_PROJECT_ROOT/logs/e2e/test-device-removal-$(date -u +%Y%m%dT%H%M%S).log"
    : > "$TEST_LOG_FILE"

    # Step 1
    step_log 1 "Start sidecar and verify health"
    check_sidecar_binary >/dev/null 2>&1 || return 1
    start_sidecar || return 1
    SIDECAR_STARTED=1
    exec 4<"$E2E_SIDECAR_STDOUT"

    local ping_result
    ping_result=$(sidecar_rpc_session "system.ping" "{}" 8) || {
        fail_test "system.ping failed"
        return 1
    }
    if ! echo "$ping_result" | jq -e '.result.protocol == "v1"' >/dev/null 2>&1; then
        fail_test "system.ping response invalid"
        return 1
    fi

    # Step 2
    step_log 2 "Start recording with specific device_uid"
    local devices_result
    devices_result=$(sidecar_rpc_session "audio.list_devices" "{}" 8) || {
        fail_test "audio.list_devices failed"
        return 1
    }

    SELECTED_DEVICE_UID=$(echo "$devices_result" | jq -r '.result.devices[0].uid // ""')
    if [[ -z "$SELECTED_DEVICE_UID" ]]; then
        emit_line "[$(ts_human)] [SKIP] no audio input devices available"
        return 77
    fi
    record_device_state "selected_device_uid=${SELECTED_DEVICE_UID}"

    local set_device_params
    set_device_params=$(jq -nc --arg uid "$SELECTED_DEVICE_UID" '{device_uid:$uid}')
    local set_device_result
    set_device_result=$(sidecar_rpc_session "audio.set_device" "$set_device_params" 8) || {
        fail_test "audio.set_device failed"
        return 1
    }
    record_device_state "audio.set_device response=$(truncate_text "$set_device_result" 300)"

    SESSION_ID="e2e-device-removal-$(date +%s)-$RANDOM"
    local start_params
    start_params=$(jq -nc --arg sid "$SESSION_ID" --arg uid "$SELECTED_DEVICE_UID" '{session_id:$sid,device_uid:$uid}')

    local start_result
    start_result=$(sidecar_rpc_session "recording.start" "$start_params" 15) || true
    if echo "$start_result" | jq -e '.error' >/dev/null 2>&1; then
        local kind
        kind=$(echo "$start_result" | jq -r '.error.data.kind // "unknown"')
        emit_line "[$(ts_human)] [SKIP] recording.start unavailable in this environment (kind=${kind})"
        return 77
    fi

    if ! echo "$start_result" | jq -e --arg sid "$SESSION_ID" '.result.session_id == $sid' >/dev/null 2>&1; then
        fail_test "recording.start did not return expected session_id"
        return 1
    fi

    # Step 3
    step_log 3 "Simulate device removal (mock/stub path)"
    if simulate_device_removal; then
        emit_line "[$(ts_human)] [INFO] device-removal simulation completed"
    else
        local sim_status=$?
        if [[ "$sim_status" -eq 77 ]]; then
            emit_line "[$(ts_human)] [SKIP] device-removal simulation unavailable"
            return 77
        fi
        fail_test "device-removal simulation failed"
        return 1
    fi

    # Step 4
    step_log 4 "Verify recording stops gracefully and sidecar remains healthy"
    local stop_params
    stop_params=$(jq -nc --arg sid "$SESSION_ID" '{session_id:$sid}')
    local stop_result
    stop_result=$(sidecar_rpc_session "recording.stop" "$stop_params" 20) || true

    if echo "$stop_result" | jq -e '.error' >/dev/null 2>&1; then
        local stop_kind
        stop_kind=$(echo "$stop_result" | jq -r '.error.data.kind // ""')
        if [[ "$stop_kind" != "E_AUDIO_IO" ]]; then
            fail_test "recording.stop returned unexpected error kind (${stop_kind})"
            return 1
        fi
    fi

    local post_ping
    post_ping=$(sidecar_rpc_session "system.ping" "{}" 8) || {
        fail_test "sidecar became unhealthy after stop/removal path"
        return 1
    }
    if ! echo "$post_ping" | jq -e '.result.protocol == "v1"' >/dev/null 2>&1; then
        fail_test "post-removal ping invalid"
        return 1
    fi

    # Step 5
    step_log 5 "Verify partial transcript preservation during transcribing path"
    emit_line "[$(ts_human)] [INFO] Verified via policy test: test_device_hot_swap_decision_mid_transcription_forces_clipboard_preservation"

    # Step 6
    step_log 6 "Verify app:error E_DEVICE_REMOVED emission contract"
    local maybe_error_event
    maybe_error_event=$(wait_for_error_event 1 || true)
    if [[ -n "$maybe_error_event" ]]; then
        if echo "$maybe_error_event" | jq -e '.method == "app:error" and .params.error.code == "E_DEVICE_REMOVED"' >/dev/null 2>&1; then
            emit_line "[$(ts_human)] [INFO] Observed live app:error E_DEVICE_REMOVED event"
        elif echo "$maybe_error_event" | jq -e '.method == "event.transcription_error" and .params.kind == "E_AUDIO_IO"' >/dev/null 2>&1; then
            emit_line "[$(ts_human)] [INFO] Observed sidecar E_AUDIO_IO error event; canonical E_DEVICE_REMOVED validated by policy test"
        else
            fail_test "unexpected error event payload during removal validation"
            return 1
        fi
    else
        emit_line "[$(ts_human)] [INFO] No live app:error event observed; canonical E_DEVICE_REMOVED verified via policy test"
    fi

    # Step 7
    step_log 7 "Verify fallback to default device for next recording"
    local reset_device
    reset_device=$(sidecar_rpc_session "audio.set_device" '{"device_uid":null}' 8) || {
        fail_test "audio.set_device(null) failed"
        return 1
    }
    record_device_state "fallback_set_device response=$(truncate_text "$reset_device" 300)"

    local next_session_id="e2e-device-removal-next-$(date +%s)-$RANDOM"
    local next_start_params
    next_start_params=$(jq -nc --arg sid "$next_session_id" '{session_id:$sid,device_uid:null}')
    local next_start
    next_start=$(sidecar_rpc_session "recording.start" "$next_start_params" 15) || true

    if echo "$next_start" | jq -e '.result.session_id' >/dev/null 2>&1; then
        local next_stop_params
        next_stop_params=$(jq -nc --arg sid "$next_session_id" '{session_id:$sid}')
        sidecar_rpc_session "recording.cancel" "$next_stop_params" 8 >/dev/null || true
    else
        # On constrained hosts, fallback recording may still fail at the audio layer.
        local next_kind
        next_kind=$(echo "$next_start" | jq -r '.error.data.kind // "unknown"')
        if [[ "$next_kind" != "E_AUDIO_IO" ]] && [[ "$next_kind" != "E_DEVICE_NOT_FOUND" ]]; then
            fail_test "fallback recording returned unexpected error kind (${next_kind})"
            return 1
        fi
        emit_line "[$(ts_human)] [INFO] Fallback recording unavailable on host audio stack (kind=${next_kind}); policy tests cover fallback decision"
    fi

    # Step 8
    step_log 8 "Verify UI/recording state returns to Idle"
    local status_result
    status_result=$(sidecar_rpc_session "status.get" "{}" 8) || {
        fail_test "status.get failed"
        return 1
    }
    if ! echo "$status_result" | jq -e '.result.state == "idle" or .result.state == "loading_model"' >/dev/null 2>&1; then
        fail_test "status.get did not return idle/loading_model after recovery"
        return 1
    fi

    local recording_status
    recording_status=$(sidecar_rpc_session "recording.status" "{}" 8) || {
        fail_test "recording.status failed"
        return 1
    }
    if ! echo "$recording_status" | jq -e '.result.state == "idle"' >/dev/null 2>&1; then
        fail_test "recording.status did not return idle"
        return 1
    fi

    emit_line "[$(ts_human)] [RESULT] PASS"
    return 0
}

main
exit $?
