#!/usr/bin/env bash
#
# E2E Test: Sidecar Crash Loop Recovery
#
# This script validates crash/restart behavior using:
# - Live sidecar start/kill/restart cycles
# - Supervisor policy tests (backoff + circuit breaker + manual reset)
#
# Exit codes:
#   0 - Pass
#   1 - Test failure
#   2 - Environment/setup failure
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

RECOVERY_LOG_FILE=""
RECOVERY_STARTED_AT_MS=0
declare -a RECOVERY_SIDECAR_OUTPUT=()

record_recovery_line() {
    local line="$1"
    echo "[ERROR_RECOVERY_E2E] $line"
    if [[ -n "${RECOVERY_LOG_FILE:-}" ]]; then
        echo "[ERROR_RECOVERY_E2E] $line" >> "$RECOVERY_LOG_FILE"
    fi
}

dump_failure_context() {
    record_recovery_line "FAILURE: dumping context"
    record_recovery_line "Last sidecar output lines:"
    local sidecar_count="${#RECOVERY_SIDECAR_OUTPUT[@]}"
    if (( sidecar_count == 0 )); then
        record_recovery_line "(no buffered sidecar output)"
    else
        local from=$(( sidecar_count > 10 ? sidecar_count - 10 : 0 ))
        for (( i=from; i<sidecar_count; i++ )); do
            record_recovery_line "SIDECAR[$((i + 1))/${sidecar_count}] ${RECOVERY_SIDECAR_OUTPUT[$i]}"
        done
    fi

    record_recovery_line "Process table excerpt:"
    if command -v ps >/dev/null 2>&1; then
        ps -ef | grep -E "openvoicy-sidecar|translator-voice-input-tool" | grep -v grep | while IFS= read -r line; do
            record_recovery_line "$line"
        done
    fi
}

# Stateful RPC for already-running sidecar process (via FIFO fds from common.sh).
sidecar_rpc_session() {
    local method="$1"
    local params="$2"
    [[ -z "$params" ]] && params='{}'
    local timeout="${3:-10}"

    local request_id
    request_id=$((RANDOM * RANDOM))
    local request
    request=$(printf '{"jsonrpc":"2.0","id":%s,"method":"%s","params":%s}' \
        "$request_id" \
        "$method" \
        "$params")

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
                RECOVERY_SIDECAR_OUTPUT+=("$line")
                continue
            fi

            local line_id
            line_id=$(echo "$line" | jq -r '.id // empty' 2>/dev/null || true)
            if [[ "$line_id" != "$request_id" ]]; then
                continue
            fi

            printf '%s\n' "$line"
            return 0
        fi
    done

    printf '%s\n' '{"error":{"message":"timeout"}}'
    return 1
}

start_sidecar_session() {
    start_sidecar || return 1
    exec 4<"$E2E_SIDECAR_STDOUT"
    return 0
}

force_kill_sidecar() {
    if [[ -n "${E2E_SIDECAR_PID:-}" ]] && kill -0 "$E2E_SIDECAR_PID" 2>/dev/null; then
        log_info "recovery" "kill" "Force-killing sidecar" "{\"pid\":$E2E_SIDECAR_PID}"
        kill -9 "$E2E_SIDECAR_PID" 2>/dev/null || true
        wait "$E2E_SIDECAR_PID" 2>/dev/null || true
        E2E_SIDECAR_PID=""
    fi
    exec 4<&- 2>/dev/null || true
}

run_policy_test() {
    local test_name="$1"
    local started_ms
    started_ms=$(date +%s%3N)
    if cargo test --manifest-path "$E2E_PROJECT_ROOT/src-tauri/Cargo.toml" "$test_name" >/dev/null 2>&1; then
        local elapsed_ms=$(( $(date +%s%3N) - started_ms ))
        log_info "policy" "cargo_test" "PASS: $test_name" "{\"duration_ms\":$elapsed_ms}"
        ((E2E_ASSERTIONS_PASSED++)) || true
        record_recovery_line "policy test pass: $test_name (${elapsed_ms}ms)"
    else
        log_error "policy" "cargo_test" "FAIL: $test_name"
        ((E2E_ASSERTIONS_FAILED++)) || true
        record_recovery_line "policy test fail: $test_name"
    fi
}

main() {
    require_jq
    init_logging "test-error-recovery"
    init_common
    setup_cleanup_trap

    mkdir -p "$E2E_PROJECT_ROOT/logs/e2e"
    RECOVERY_LOG_FILE="$E2E_PROJECT_ROOT/logs/e2e/test-error-recovery-$(date -u +%Y%m%dT%H%M%SZ).log"
    touch "$RECOVERY_LOG_FILE"

    RECOVERY_STARTED_AT_MS=$(date +%s%3N)
    record_recovery_line "starting crash loop recovery e2e"
    log_info "test" "start" "Starting sidecar crash loop recovery E2E test"

    assert_dir_exists "$E2E_PROJECT_ROOT/sidecar" "Sidecar directory exists" || exit 2
    check_sidecar_binary || exit 2

    # Step 1: start sidecar and verify ready.
    local step1_start_ms
    step1_start_ms=$(date +%s%3N)
    start_sidecar_session || {
        log_error "recovery" "step_1" "Failed to start sidecar"
        dump_failure_context
        return 1
    }
    local ping_result
    ping_result=$(sidecar_rpc_session "system.ping" "{}" 10) || {
        log_error "recovery" "step_1" "system.ping failed after startup"
        dump_failure_context
        return 1
    }
    if echo "$ping_result" | jq -e '.result.protocol == "v1"' >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        record_recovery_line "step1 pass: sidecar ready in $(( $(date +%s%3N) - step1_start_ms ))ms"
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "recovery" "step_1" "Unexpected ping payload" "$ping_result"
    fi

    # Step 2: kill sidecar forcefully.
    force_kill_sidecar
    if [[ -n "${E2E_SIDECAR_PID:-}" ]] && kill -0 "$E2E_SIDECAR_PID" 2>/dev/null; then
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "recovery" "step_2" "Sidecar still running after SIGKILL"
    else
        ((E2E_ASSERTIONS_PASSED++)) || true
        record_recovery_line "step2 pass: sidecar killed forcefully"
    fi

    # Step 3-9: exercise crash/recovery loop and validate policy tests.
    local -a cycle_delays=(1 2 4)
    local previous_restart_ms=0
    local restart_count=0

    for delay_s in "${cycle_delays[@]}"; do
        restart_count=$((restart_count + 1))
        record_recovery_line "cycle $restart_count: waiting ${delay_s}s before restart attempt"
        sleep "$delay_s"

        local restart_start_ms
        restart_start_ms=$(date +%s%3N)
        start_sidecar_session || {
            ((E2E_ASSERTIONS_FAILED++)) || true
            log_error "recovery" "cycle_restart" "Failed restart attempt" "{\"cycle\":$restart_count}"
            dump_failure_context
            break
        }

        local cycle_ping
        cycle_ping=$(sidecar_rpc_session "system.ping" "{}" 10) || {
            ((E2E_ASSERTIONS_FAILED++)) || true
            log_error "recovery" "cycle_restart" "Ping failed after restart" "{\"cycle\":$restart_count}"
            dump_failure_context
            break
        }

        if echo "$cycle_ping" | jq -e '.result.protocol == "v1"' >/dev/null 2>&1; then
            ((E2E_ASSERTIONS_PASSED++)) || true
            local elapsed_ms=$(( $(date +%s%3N) - restart_start_ms ))
            record_recovery_line "cycle $restart_count: restart reached ready (${elapsed_ms}ms)"

            if (( previous_restart_ms > 0 )) && (( elapsed_ms < previous_restart_ms / 4 )); then
                # Keep this as warning only: machine load can vary; policy is validated below by supervisor tests.
                log_warn "recovery" "timing" "Observed restart latency dropped unexpectedly" "{\"cycle\":$restart_count,\"elapsed_ms\":$elapsed_ms,\"prev_ms\":$previous_restart_ms}"
            fi
            previous_restart_ms=$elapsed_ms
        else
            ((E2E_ASSERTIONS_FAILED++)) || true
            log_error "recovery" "cycle_restart" "Unexpected ping payload after restart" "$cycle_ping"
        fi

        force_kill_sidecar
    done

    # Supervisor-policy validations (maps to crash-loop + breaker behavior).
    run_policy_test "supervisor::tests::restart_attempt_progression_is_immediate_then_delayed"
    run_policy_test "supervisor::tests::circuit_breaker_trips_at_exact_configured_threshold"
    run_policy_test "supervisor::tests::tripped_breaker_disables_auto_restart_until_manual_reset"
    run_policy_test "supervisor::tests::manual_reset_reenables_auto_restart_path"

    # Step 10/11 UI + command affordance proxy checks in source.
    if rg -n "RESTART_SIDECAR|restart_sidecar" "$E2E_PROJECT_ROOT/src-tauri/src/tray.rs" "$E2E_PROJECT_ROOT/src-tauri/src/commands.rs" >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        record_recovery_line "ui/command affordance check pass: restart sidecar action present"
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        record_recovery_line "ui/command affordance check fail: restart sidecar action missing"
    fi

    # Ensure no sidecar left running.
    stop_sidecar
    exec 4<&- 2>/dev/null || true

    assertion_summary
    local summary_exit=$?

    local total_ms=$(( $(date +%s%3N) - RECOVERY_STARTED_AT_MS ))
    if [[ $summary_exit -eq 0 ]]; then
        log_info "test" "complete" "Crash loop recovery test completed" "{\"duration_ms\":$total_ms}"
        record_recovery_line "PASS total_duration_ms=$total_ms"
        return 0
    fi

    log_error "test" "complete" "Crash loop recovery test failed" "{\"duration_ms\":$total_ms}"
    dump_failure_context
    record_recovery_line "FAIL total_duration_ms=$total_ms"
    return 1
}

main
exit $?
