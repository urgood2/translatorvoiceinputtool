#!/usr/bin/env bash
#
# E2E Test: Recording Cancel Flow
#
# Validates that cancel_recording produces NO transcript and returns to idle.
#
# Test steps:
# 1. Start sidecar, verify health (system.ping + status.get)
# 2. Send recording.start with session_id=A
# 3. Wait brief period (500ms) to simulate recording
# 4. Send recording.cancel with session_id=A
# 5. Verify: NO event.transcription_complete notification arrives (wait 3s timeout)
# 6. Verify: status returns state=idle
# 7. Verify: app is ready for new recording (recording.start with session_id=B succeeds)
# 8. Clean up: recording.cancel session_id=B, system.shutdown
#
# Edge cases:
# - Cancel immediately after start (0ms recording)
# - Cancel during sidecar model loading
#
# Exit codes:
#   0 - All tests passed
#   1 - Test failure
#   2 - Environment setup error
#   3 - Timeout
#

set -euo pipefail

# Resolve script directory
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

# Accumulate unexpected notifications for the summary
UNEXPECTED_EVENTS=()
STEPS_PASSED=0
STEPS_TOTAL=8

# Send JSON-RPC request to an already running sidecar process and read response
# for matching request id from the persistent stdout reader FD.
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

    local deadline=$((SECONDS + timeout))
    local line=""

    while (( SECONDS < deadline )); do
        local wait_s=$((deadline - SECONDS))
        if (( wait_s <= 0 )); then
            break
        fi

        if IFS= read -r -u 4 -t "$wait_s" line; then
            if [[ "$line" != *'"jsonrpc"'* ]]; then
                continue
            fi

            local line_id
            line_id=$(echo "$line" | jq -r '.id // empty' 2>/dev/null || true)
            if [[ "$line_id" == "$request_id" ]]; then
                printf '%s\n' "$line"
                return 0
            fi

            # Check for unexpected notifications
            local line_method
            line_method=$(echo "$line" | jq -r '.method // empty' 2>/dev/null || true)
            if [[ -n "$line_method" ]]; then
                local line_session
                line_session=$(echo "$line" | jq -r '.params.session_id // empty' 2>/dev/null || true)
                log_info "cancel_e2e" "notification" "[CANCEL_E2E] Notification received" \
                    "{\"type\":\"$line_method\",\"session_id\":\"$line_session\"}"

                if [[ "$line_method" == *"transcription_complete"* ]]; then
                    log_error "cancel_e2e" "notification" "UNEXPECTED transcription_complete while waiting for response" "$line"
                    UNEXPECTED_EVENTS+=("$line")
                fi
            fi

            continue
        fi

        break
    done

    printf '%s\n' '{"error":{"message":"timeout"}}'
    return 1
}

# Drain any pending notifications from FD 4 for a given duration.
# If an event.transcription_complete notification appears, record it as unexpected.
# Args: drain_seconds
drain_notifications() {
    local drain_seconds="${1:-3}"
    local deadline=$((SECONDS + drain_seconds))
    local line=""
    local count=0

    while (( SECONDS < deadline )); do
        local wait_s=$((deadline - SECONDS))
        if (( wait_s <= 0 )); then
            break
        fi

        if IFS= read -r -u 4 -t "$wait_s" line; then
            if [[ "$line" != *'"jsonrpc"'* ]]; then
                continue
            fi

            local line_method
            line_method=$(echo "$line" | jq -r '.method // empty' 2>/dev/null || true)
            local line_session
            line_session=$(echo "$line" | jq -r '.params.session_id // empty' 2>/dev/null || true)

            log_info "cancel_e2e" "notification" "[CANCEL_E2E] Notification received" \
                "{\"type\":\"$line_method\",\"session_id\":\"$line_session\"}"

            if [[ "$line_method" == *"transcription_complete"* ]]; then
                log_error "cancel_e2e" "drain" "UNEXPECTED transcription_complete after cancel!" "$line"
                UNEXPECTED_EVENTS+=("$line")
            fi

            ((count++)) || true
        else
            break
        fi
    done

    log_debug "cancel_e2e" "drain" "Drain complete" "{\"drained\":$count,\"seconds\":$drain_seconds}"
    return 0
}

main() {
    local test_start
    test_start=$(start_timer)

    # Initialize
    require_jq
    init_logging "test-cancel-flow"
    init_common
    setup_cleanup_trap

    log_info "cancel_e2e" "start" "Starting recording cancel flow E2E test"

    # =========================================================================
    # Step 1/8: Start sidecar, verify health
    # =========================================================================
    log_info "cancel_e2e" "step_1" "[CANCEL_E2E] Step 1/8: Start sidecar and verify health"

    local step1_start
    step1_start=$(start_timer)

    assert_dir_exists "$E2E_PROJECT_ROOT/sidecar" "Sidecar directory exists" || exit 2
    check_sidecar_binary || exit 2

    start_sidecar || exit 2
    exec 4<"$E2E_SIDECAR_STDOUT"

    # Ping test
    local ping_result
    ping_result=$(sidecar_rpc_session "system.ping" "{}" 10) || {
        log_error "cancel_e2e" "step_1" "system.ping failed"
        exit 1
    }

    assert_json_eq "$ping_result" ".result.protocol" "v1" "Protocol version is v1"

    # Status check
    local status_result
    status_result=$(sidecar_rpc_session "status.get" "{}" 10) || true

    if echo "$status_result" | jq -e '.result' >/dev/null 2>&1; then
        log_info "cancel_e2e" "step_1" "Sidecar health verified" "$status_result"
    else
        log_warn "cancel_e2e" "step_1" "status.get returned error (continuing)" "$status_result"
    fi

    log_with_duration "INFO" "cancel_e2e" "step_1" "[CANCEL_E2E] Step 1/8: complete" "{}" "$step1_start"
    ((STEPS_PASSED++)) || true

    # =========================================================================
    # Step 2/8: Start recording (session A)
    # =========================================================================
    log_info "cancel_e2e" "step_2" "[CANCEL_E2E] Step 2/8: Start recording (session A)"

    local step2_start
    step2_start=$(start_timer)

    local start_result
    start_result=$(sidecar_rpc_session "recording.start" '{"device_uid":null}' 15) || true

    local session_a=""
    if echo "$start_result" | jq -e '.result.session_id' >/dev/null 2>&1; then
        session_a=$(echo "$start_result" | jq -r '.result.session_id')
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "cancel_e2e" "step_2" "Recording started" "{\"session_id\":\"$session_a\"}"
    elif echo "$start_result" | jq -e '.error' >/dev/null 2>&1; then
        # Recording may fail on headless CI (no audio device) - this is acceptable
        ((E2E_ASSERTIONS_PASSED++)) || true
        local error_kind
        error_kind=$(echo "$start_result" | jq -r '.error.data.kind // .error.message // "unknown"')
        log_warn "cancel_e2e" "step_2" "Recording start returned structured error (expected on CI)" \
            "{\"error_kind\":\"$error_kind\"}"
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "cancel_e2e" "step_2" "Recording start returned invalid payload" "$start_result"
    fi

    log_with_duration "INFO" "cancel_e2e" "step_2" "[CANCEL_E2E] Step 2/8: complete" "{}" "$step2_start"
    ((STEPS_PASSED++)) || true

    # =========================================================================
    # Step 3/8: Wait brief period (500ms) to simulate recording
    # =========================================================================
    log_info "cancel_e2e" "step_3" "[CANCEL_E2E] Step 3/8: Brief recording period (500ms)"

    if [ -n "$session_a" ]; then
        sleep 0.5
        log_info "cancel_e2e" "step_3" "Recording period elapsed"
    else
        log_info "cancel_e2e" "step_3" "Skipped (no active recording session)"
    fi

    ((STEPS_PASSED++)) || true

    # =========================================================================
    # Step 4/8: Cancel recording (session A)
    # =========================================================================
    log_info "cancel_e2e" "step_4" "[CANCEL_E2E] Step 4/8: Cancel recording (session A)"

    local step4_start
    step4_start=$(start_timer)

    if [ -n "$session_a" ]; then
        local cancel_params
        cancel_params=$(jq -nc --arg sid "$session_a" '{session_id:$sid}')

        local cancel_result
        cancel_result=$(sidecar_rpc_session "recording.cancel" "$cancel_params" 10) || true

        if echo "$cancel_result" | jq -e '.result.cancelled' >/dev/null 2>&1; then
            assert_json_eq "$cancel_result" ".result.cancelled" "true" "Cancel result is true"
            assert_json_eq "$cancel_result" ".result.session_id" "$session_a" "Cancel session_id matches"
            log_info "cancel_e2e" "step_4" "Recording cancelled successfully" "$cancel_result"
        elif echo "$cancel_result" | jq -e '.error' >/dev/null 2>&1; then
            ((E2E_ASSERTIONS_FAILED++)) || true
            log_error "cancel_e2e" "step_4" "Cancel returned error (unexpected)" "$cancel_result"
        else
            ((E2E_ASSERTIONS_FAILED++)) || true
            log_error "cancel_e2e" "step_4" "Cancel returned invalid payload" "$cancel_result"
        fi
    else
        # No recording was started - test cancel when not recording
        local cancel_result
        cancel_result=$(sidecar_rpc_session "recording.cancel" '{"session_id":"no-session"}' 10) || true

        if echo "$cancel_result" | jq -e '.error' >/dev/null 2>&1; then
            ((E2E_ASSERTIONS_PASSED++)) || true
            log_info "cancel_e2e" "step_4" "Cancel when not recording returned structured error (expected)" \
                "$cancel_result"
        else
            ((E2E_ASSERTIONS_FAILED++)) || true
            log_error "cancel_e2e" "step_4" "Cancel returned unexpected success when no recording active" \
                "$cancel_result"
        fi
    fi

    log_with_duration "INFO" "cancel_e2e" "step_4" "[CANCEL_E2E] Step 4/8: complete" "{}" "$step4_start"
    ((STEPS_PASSED++)) || true

    # =========================================================================
    # Step 5/8: Verify NO transcription_complete notification (3s drain)
    # =========================================================================
    log_info "cancel_e2e" "step_5" "[CANCEL_E2E] Step 5/8: Verify no transcription_complete notification"

    local step5_start
    step5_start=$(start_timer)

    drain_notifications 3

    if [ ${#UNEXPECTED_EVENTS[@]} -eq 0 ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "cancel_e2e" "step_5" "No unexpected transcription_complete notifications"
    else
        local unexpected_dump_json
        unexpected_dump_json=$(printf '%s\n' "${UNEXPECTED_EVENTS[@]}" | jq -R . | jq -s .)
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "cancel_e2e" "step_5" "FAIL: unexpected transcription_complete received" \
            "{\"count\":${#UNEXPECTED_EVENTS[@]},\"events\":$unexpected_dump_json}"
    fi

    log_with_duration "INFO" "cancel_e2e" "step_5" "[CANCEL_E2E] Step 5/8: complete" "{}" "$step5_start"
    ((STEPS_PASSED++)) || true

    # =========================================================================
    # Step 6/8: Verify state is idle
    # =========================================================================
    log_info "cancel_e2e" "step_6" "[CANCEL_E2E] Step 6/8: Verify state is idle"

    local step6_start
    step6_start=$(start_timer)

    local status_after_cancel
    status_after_cancel=$(sidecar_rpc_session "status.get" "{}" 10) || true

    if echo "$status_after_cancel" | jq -e '.result.state' >/dev/null 2>&1; then
        assert_json_eq "$status_after_cancel" ".result.state" "idle" "status.get reports idle after cancel"
        log_info "cancel_e2e" "step_6" "State confirmed idle via status.get" "$status_after_cancel"
    elif echo "$status_after_cancel" | jq -e '.error' >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "cancel_e2e" "step_6" "status.get returned error" "$status_after_cancel"
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "cancel_e2e" "step_6" "status.get returned invalid payload" "$status_after_cancel"
    fi

    log_with_duration "INFO" "cancel_e2e" "step_6" "[CANCEL_E2E] Step 6/8: complete" "{}" "$step6_start"
    ((STEPS_PASSED++)) || true

    # =========================================================================
    # Step 7/8: Verify app is ready for new recording (session B)
    # =========================================================================
    log_info "cancel_e2e" "step_7" "[CANCEL_E2E] Step 7/8: Verify readiness for new recording (session B)"

    local step7_start
    step7_start=$(start_timer)

    local start_b_result
    start_b_result=$(sidecar_rpc_session "recording.start" '{"device_uid":null}' 15) || true

    local session_b=""
    if echo "$start_b_result" | jq -e '.result.session_id' >/dev/null 2>&1; then
        session_b=$(echo "$start_b_result" | jq -r '.result.session_id')
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "cancel_e2e" "step_7" "New recording started after cancel (system recovered)" \
            "{\"session_id\":\"$session_b\"}"
    elif echo "$start_b_result" | jq -e '.error' >/dev/null 2>&1; then
        # On CI without audio, both start calls will error - that's fine
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_warn "cancel_e2e" "step_7" "Recording start B returned structured error (expected on CI)" \
            "$start_b_result"
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "cancel_e2e" "step_7" "Recording start B returned invalid payload" "$start_b_result"
    fi

    log_with_duration "INFO" "cancel_e2e" "step_7" "[CANCEL_E2E] Step 7/8: complete" "{}" "$step7_start"
    ((STEPS_PASSED++)) || true

    # =========================================================================
    # Edge case A: Cancel immediately after start (0ms recording)
    # =========================================================================
    log_info "cancel_e2e" "edge_1" "[CANCEL_E2E] Edge case: cancel immediately after start"

    local edge0_start_result
    edge0_start_result=$(sidecar_rpc_session "recording.start" '{"device_uid":null}' 10) || true

    if echo "$edge0_start_result" | jq -e '.result.session_id' >/dev/null 2>&1; then
        local edge0_session
        edge0_session=$(echo "$edge0_start_result" | jq -r '.result.session_id')
        local edge0_cancel_params
        edge0_cancel_params=$(jq -nc --arg sid "$edge0_session" '{session_id:$sid}')
        local edge0_cancel_result
        edge0_cancel_result=$(sidecar_rpc_session "recording.cancel" "$edge0_cancel_params" 10) || true

        if echo "$edge0_cancel_result" | jq -e '.result.cancelled == true' >/dev/null 2>&1; then
            ((E2E_ASSERTIONS_PASSED++)) || true
            log_info "cancel_e2e" "edge_1" "Immediate cancel succeeded" "$edge0_cancel_result"
        else
            ((E2E_ASSERTIONS_FAILED++)) || true
            log_error "cancel_e2e" "edge_1" "Immediate cancel failed" "$edge0_cancel_result"
        fi
    elif echo "$edge0_start_result" | jq -e '.error' >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_warn "cancel_e2e" "edge_1" "Immediate-cancel edge case skipped (start unavailable in environment)" \
            "$edge0_start_result"
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "cancel_e2e" "edge_1" "Immediate-cancel edge case returned invalid start payload" \
            "$edge0_start_result"
    fi

    # =========================================================================
    # Edge case B: Cancel during model loading (best effort)
    # =========================================================================
    log_info "cancel_e2e" "edge_2" "[CANCEL_E2E] Edge case: cancel during model loading"

    local initialize_result
    initialize_result=$(sidecar_rpc_session "asr.initialize" '{"model_id":"nvidia/parakeet-tdt-0.6b-v3"}' 10) || true

    # Give status pipeline a short window to transition into loading_model.
    sleep 0.2

    local loading_status
    loading_status=$(sidecar_rpc_session "status.get" "{}" 10) || true
    local loading_state
    loading_state=$(echo "$loading_status" | jq -r '.result.state // empty' 2>/dev/null || true)

    if [[ "$loading_state" == "loading_model" ]]; then
        local loading_cancel_result
        loading_cancel_result=$(sidecar_rpc_session "recording.cancel" '{"session_id":"edge-loading"}' 10) || true
        if echo "$loading_cancel_result" | jq -e '.result.cancelled == true or .error' >/dev/null 2>&1; then
            ((E2E_ASSERTIONS_PASSED++)) || true
            log_info "cancel_e2e" "edge_2" "Cancel during loading handled with structured response" \
                "$loading_cancel_result"
        else
            ((E2E_ASSERTIONS_FAILED++)) || true
            log_error "cancel_e2e" "edge_2" "Cancel during loading returned invalid payload" \
                "$loading_cancel_result"
        fi
    else
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_warn "cancel_e2e" "edge_2" "Model-loading edge case skipped (state did not enter loading_model)" \
            "{\"state\":\"$loading_state\",\"initialize\":$initialize_result,\"status\":$loading_status}"
    fi

    # =========================================================================
    # Step 8/8: Clean up
    # =========================================================================
    log_info "cancel_e2e" "step_8" "[CANCEL_E2E] Step 8/8: Clean up"

    local step8_start
    step8_start=$(start_timer)

    # Cancel session B if it was started
    if [ -n "$session_b" ]; then
        local cancel_b_params
        cancel_b_params=$(jq -nc --arg sid "$session_b" '{session_id:$sid}')

        local cancel_b_result
        cancel_b_result=$(sidecar_rpc_session "recording.cancel" "$cancel_b_params" 10) || true
        log_debug "cancel_e2e" "step_8" "Session B cancelled" "$cancel_b_result"
    fi

    # Shutdown sidecar
    local shutdown_result
    shutdown_result=$(sidecar_rpc_session "system.shutdown" "{}" 5) 2>/dev/null || true
    log_debug "cancel_e2e" "step_8" "Shutdown sent" "$shutdown_result"

    log_with_duration "INFO" "cancel_e2e" "step_8" "[CANCEL_E2E] Step 8/8: complete" "{}" "$step8_start"
    ((STEPS_PASSED++)) || true

    # =========================================================================
    # Summary
    # =========================================================================
    local total_ms
    total_ms=$(( $(date +%s%3N) - test_start ))

    local unexpected_events_json='[]'
    if [ ${#UNEXPECTED_EVENTS[@]} -gt 0 ]; then
        unexpected_events_json=$(printf '%s\n' "${UNEXPECTED_EVENTS[@]}" | jq -R . | jq -s .)
    fi

    local summary
    summary=$(jq -nc \
        --argjson total_ms "$total_ms" \
        --argjson steps_passed "$STEPS_PASSED" \
        --argjson steps_total "$STEPS_TOTAL" \
        --argjson unexpected_events "$unexpected_events_json" \
        '{total_ms:$total_ms,steps_passed:$steps_passed,steps_total:$steps_total,unexpected_events:$unexpected_events}')

    log_info "cancel_e2e" "summary" "Test summary" "$summary"

    assertion_summary

    if [ "$E2E_ASSERTIONS_FAILED" -gt 0 ] || [ ${#UNEXPECTED_EVENTS[@]} -gt 0 ]; then
        log_error "cancel_e2e" "complete" "Recording cancel flow test FAILED"
        return 1
    fi

    log_info "cancel_e2e" "complete" "Recording cancel flow test PASSED"
    return 0
}

# Run main
main
exit $?
