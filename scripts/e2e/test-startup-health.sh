#!/usr/bin/env bash
#
# E2E Test: Sidecar Startup Health Sequence
#
# Validates startup flow:
# 1) Start sidecar process
# 2) system.ping
# 3) system.info
# 4) status.get
# 5) Startup sequence under threshold
# 6) system.shutdown with clean exit
#

set -euo pipefail

if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

source "$SCRIPT_DIR/lib/log.sh"
source "$SCRIPT_DIR/lib/assert.sh"
source "$SCRIPT_DIR/lib/common.sh"

STEPS_TOTAL=6
STEPS_PASSED=0
STARTUP_BUDGET_MS=10000
STARTUP_SEQUENCE_START_MS=0
STARTUP_LOG_FILE=""

declare -a STARTUP_RPC_HISTORY=()
declare -a STARTUP_SIDEcar_OUTPUT=()

step_log() {
    local step="$1"
    local message="$2"
    local line="[STARTUP_E2E] Step ${step}/${STEPS_TOTAL}: ${message}"
    echo "$line"
    if [[ -n "${STARTUP_LOG_FILE:-}" ]]; then
        echo "$line" >> "$STARTUP_LOG_FILE"
    fi
    log_info "startup_e2e" "step_${step}" "$message"
}

record_rpc_history() {
    local method="$1"
    local duration_ms="$2"
    local response="$3"
    STARTUP_RPC_HISTORY+=("method=${method} duration_ms=${duration_ms} response=${response}")
}

dump_failure_context() {
    local rpc_count="${#STARTUP_RPC_HISTORY[@]}"
    local sidecar_count="${#STARTUP_SIDEcar_OUTPUT[@]}"

    echo "[STARTUP_E2E] FAILURE: dumping context (last 5 RPC exchanges + recent sidecar output)"
    if [[ -n "${STARTUP_LOG_FILE:-}" ]]; then
        echo "[STARTUP_E2E] FAILURE: dumping context (last 5 RPC exchanges + recent sidecar output)" >> "$STARTUP_LOG_FILE"
    fi

    if (( rpc_count > 0 )); then
        local from=$(( rpc_count > 5 ? rpc_count - 5 : 0 ))
        local i
        for (( i=from; i<rpc_count; i++ )); do
            local line="[STARTUP_E2E] RPC[$((i + 1))/${rpc_count}] ${STARTUP_RPC_HISTORY[$i]}"
            echo "$line"
            if [[ -n "${STARTUP_LOG_FILE:-}" ]]; then
                echo "$line" >> "$STARTUP_LOG_FILE"
            fi
        done
    fi

    if (( sidecar_count > 0 )); then
        local from=$(( sidecar_count > 5 ? sidecar_count - 5 : 0 ))
        local i
        for (( i=from; i<sidecar_count; i++ )); do
            local line="[STARTUP_E2E] SIDECAR[$((i + 1))/${sidecar_count}] ${STARTUP_SIDEcar_OUTPUT[$i]}"
            echo "$line"
            if [[ -n "${STARTUP_LOG_FILE:-}" ]]; then
                echo "$line" >> "$STARTUP_LOG_FILE"
            fi
        done
    fi
}

# Stateful RPC for already-running sidecar process.
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

    local call_start_ms
    call_start_ms=$(date +%s%3N)

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
                STARTUP_SIDEcar_OUTPUT+=("$line")
                continue
            fi

            local line_id
            line_id=$(echo "$line" | jq -r '.id // empty' 2>/dev/null || true)
            if [[ "$line_id" != "$request_id" ]]; then
                continue
            fi

            local call_end_ms
            call_end_ms=$(date +%s%3N)
            local duration_ms=$((call_end_ms - call_start_ms))
            record_rpc_history "$method" "$duration_ms" "$line"
            printf '%s\n' "$line"
            return 0
        fi
    done

    local timeout_response
    timeout_response='{"error":{"message":"timeout"}}'
    local call_end_ms
    call_end_ms=$(date +%s%3N)
    local duration_ms=$((call_end_ms - call_start_ms))
    record_rpc_history "$method" "$duration_ms" "$timeout_response"
    printf '%s\n' "$timeout_response"
    return 1
}

emit_summary() {
    local total_ms="$1"
    local summary
    summary=$(jq -nc \
        --argjson total_ms "$total_ms" \
        --argjson steps_passed "$STEPS_PASSED" \
        --argjson steps_total "$STEPS_TOTAL" \
        '{total_ms:$total_ms,steps_passed:$steps_passed,steps_total:$steps_total}')
    echo "$summary"
    if [[ -n "${STARTUP_LOG_FILE:-}" ]]; then
        echo "$summary" >> "$STARTUP_LOG_FILE"
    fi
}

main() {
    require_jq
    init_logging "test-startup-health"
    init_common
    setup_cleanup_trap

    mkdir -p "$E2E_PROJECT_ROOT/logs/e2e"
    STARTUP_LOG_FILE="$E2E_PROJECT_ROOT/logs/e2e/test-startup-health-$(date -u +%Y%m%dT%H%M%S).log"
    touch "$STARTUP_LOG_FILE"

    STARTUP_SEQUENCE_START_MS=$(date +%s%3N)

    # Step 1: Start sidecar process
    local step1_start_ms
    step1_start_ms=$(date +%s%3N)
    start_sidecar || {
        log_error "startup_e2e" "step_1" "Failed to start sidecar"
        dump_failure_context
        local total_ms=$(( $(date +%s%3N) - STARTUP_SEQUENCE_START_MS ))
        emit_summary "$total_ms"
        return 1
    }
    exec 4<"$E2E_SIDECAR_STDOUT"
    local step1_ms=$(( $(date +%s%3N) - step1_start_ms ))
    step_log 1 "Starting sidecar process (pid=$E2E_SIDECAR_PID) (${step1_ms}ms)"
    ((STEPS_PASSED++)) || true

    # Step 2: system.ping
    local ping_start_ms
    ping_start_ms=$(date +%s%3N)
    local ping_result
    ping_result=$(sidecar_rpc_session "system.ping" "{}" 5) || {
        log_error "startup_e2e" "step_2" "system.ping failed"
        dump_failure_context
        local total_ms=$(( $(date +%s%3N) - STARTUP_SEQUENCE_START_MS ))
        emit_summary "$total_ms"
        return 1
    }

    # Backward compatibility: older fixtures mention pong=true, current API returns protocol/version.
    if ! echo "$ping_result" | jq -e '(.result.pong == true) or (.result.protocol == "v1")' >/dev/null 2>&1; then
        log_error "startup_e2e" "step_2" "system.ping response shape invalid" "$ping_result"
        dump_failure_context
        local total_ms=$(( $(date +%s%3N) - STARTUP_SEQUENCE_START_MS ))
        emit_summary "$total_ms"
        return 1
    fi
    local ping_ms=$(( $(date +%s%3N) - ping_start_ms ))
    step_log 2 "system.ping -> OK (${ping_ms}ms)"
    ((STEPS_PASSED++)) || true

    # Step 3: system.info
    local info_start_ms
    info_start_ms=$(date +%s%3N)
    local info_result
    info_result=$(sidecar_rpc_session "system.info" "{}" 5) || {
        log_error "startup_e2e" "step_3" "system.info failed"
        dump_failure_context
        local total_ms=$(( $(date +%s%3N) - STARTUP_SEQUENCE_START_MS ))
        emit_summary "$total_ms"
        return 1
    }

    if ! echo "$info_result" | jq -e '
        (.result.capabilities | type == "array" and length > 0) and
        (.result.runtime.python_version | type == "string" and test("^[0-9]+\\.[0-9]+\\.[0-9]+$")) and
        (.result.runtime.platform | IN("win32","darwin","linux")) and
        (.result.runtime.cuda_available | type == "boolean")
    ' >/dev/null 2>&1; then
        log_error "startup_e2e" "step_3" "system.info response validation failed" "$info_result"
        dump_failure_context
        local total_ms=$(( $(date +%s%3N) - STARTUP_SEQUENCE_START_MS ))
        emit_summary "$total_ms"
        return 1
    fi

    local info_ms=$(( $(date +%s%3N) - info_start_ms ))
    local capabilities
    capabilities=$(echo "$info_result" | jq -c '.result.capabilities')
    local py_ver
    py_ver=$(echo "$info_result" | jq -r '.result.runtime.python_version')
    local platform
    platform=$(echo "$info_result" | jq -r '.result.runtime.platform')
    local cuda
    cuda=$(echo "$info_result" | jq -r '.result.runtime.cuda_available')
    step_log 3 "system.info -> capabilities=${capabilities}, python=${py_ver}, platform=${platform}, cuda=${cuda} (${info_ms}ms)"
    ((STEPS_PASSED++)) || true

    # Step 4: status.get
    local status_start_ms
    status_start_ms=$(date +%s%3N)
    local status_result
    status_result=$(sidecar_rpc_session "status.get" "{}" 5) || {
        log_error "startup_e2e" "step_4" "status.get failed"
        dump_failure_context
        local total_ms=$(( $(date +%s%3N) - STARTUP_SEQUENCE_START_MS ))
        emit_summary "$total_ms"
        return 1
    }

    if ! echo "$status_result" | jq -e '
        .result.state as $state |
        ($state | IN("idle","recording","transcribing","error")) and
        ((.result | has("detail") | not) or (.result.detail | type == "string")) and
        (if $state == "error" then ((.result | has("detail")) and (.result.detail | type == "string")) else true end) and
        ((.result | has("model") | not) or ((.result.model.model_id | type == "string") and (.result.model.status | type == "string")))
    ' >/dev/null 2>&1; then
        log_error "startup_e2e" "step_4" "status.get response validation failed" "$status_result"
        dump_failure_context
        local total_ms=$(( $(date +%s%3N) - STARTUP_SEQUENCE_START_MS ))
        emit_summary "$total_ms"
        return 1
    fi

    local status_ms=$(( $(date +%s%3N) - status_start_ms ))
    local state
    state=$(echo "$status_result" | jq -r '.result.state')
    local model_status
    model_status=$(echo "$status_result" | jq -r '.result.model.status // "none"')
    step_log 4 "status.get -> state=${state}, model=${model_status} (${status_ms}ms)"
    ((STEPS_PASSED++)) || true

    # Step 5: startup sequence budget
    local total_startup_ms=$(( $(date +%s%3N) - STARTUP_SEQUENCE_START_MS ))
    if (( total_startup_ms >= STARTUP_BUDGET_MS )); then
        log_error "startup_e2e" "step_5" "Startup sequence exceeded threshold" "{\"total_ms\":$total_startup_ms,\"threshold_ms\":$STARTUP_BUDGET_MS}"
        dump_failure_context
        emit_summary "$total_startup_ms"
        return 1
    fi
    step_log 5 "Total startup time: ${total_startup_ms}ms (threshold: ${STARTUP_BUDGET_MS}ms) ✓"
    ((STEPS_PASSED++)) || true

    # Step 6: system.shutdown + clean exit
    local shutdown_result
    shutdown_result=$(sidecar_rpc_session "system.shutdown" "{}" 5) || {
        log_error "startup_e2e" "step_6" "system.shutdown failed"
        dump_failure_context
        local total_ms=$(( $(date +%s%3N) - STARTUP_SEQUENCE_START_MS ))
        emit_summary "$total_ms"
        return 1
    }
    if ! echo "$shutdown_result" | jq -e '.result.status == "shutting_down" or .result.ok == true' >/dev/null 2>&1; then
        log_error "startup_e2e" "step_6" "system.shutdown response validation failed" "$shutdown_result"
        dump_failure_context
        local total_ms=$(( $(date +%s%3N) - STARTUP_SEQUENCE_START_MS ))
        emit_summary "$total_ms"
        return 1
    fi

    local wait_exit=0
    if [[ -n "${E2E_SIDECAR_PID:-}" ]]; then
        wait "$E2E_SIDECAR_PID" || wait_exit=$?
        E2E_SIDECAR_PID=""
    fi
    if (( wait_exit != 0 )); then
        log_error "startup_e2e" "step_6" "Sidecar did not exit cleanly" "{\"exit_code\":$wait_exit}"
        dump_failure_context
        local total_ms=$(( $(date +%s%3N) - STARTUP_SEQUENCE_START_MS ))
        emit_summary "$total_ms"
        return 1
    fi
    step_log 6 "system.shutdown -> clean exit (code=0)"
    ((STEPS_PASSED++)) || true

    local total_ms=$(( $(date +%s%3N) - STARTUP_SEQUENCE_START_MS ))
    emit_summary "$total_ms"
    return 0
}

main
exit $?
