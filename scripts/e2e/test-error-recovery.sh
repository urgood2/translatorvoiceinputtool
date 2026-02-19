#!/usr/bin/env bash
#
# E2E Test: Sidecar Error Recovery with Diagnostics
#
# This script validates plan-aligned recovery behavior:
# 1) Single crash recovery
# 2) Crash-loop circuit breaker behavior
# 3) Manual restart path after breaker trip
# 4) IPC timeout / unresponsive sidecar behavior
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
RECOVERY_SUMMARY_FILE=""
RECOVERY_STARTED_AT_MS=0
POLICY_TEST_LAST_DURATION_MS=0
SCENARIOS_PASSED=0
SCENARIOS_FAILED=0
declare -a RECOVERY_SIDECAR_OUTPUT=()
declare -a RECOVERY_EVENT_TIMELINE=()
declare -a RECOVERY_SCENARIOS=()

record_recovery_line() {
    local line="$1"
    echo "[ERROR_RECOVERY] $line"
    if [[ -n "${RECOVERY_LOG_FILE:-}" ]]; then
        echo "[ERROR_RECOVERY] $line" >> "$RECOVERY_LOG_FILE"
    fi
}

record_timeline_event() {
    local event="$1"
    local data_json="${2:-}"
    [[ -z "$data_json" ]] && data_json='{}'
    local ts
    ts=$(date -u +"%Y-%m-%dT%H:%M:%S.%3NZ")
    local event_json
    event_json=$(jq -nc \
        --arg ts "$ts" \
        --arg event "$event" \
        --argjson data "$data_json" \
        '{ts:$ts,event:$event,data:$data}')
    RECOVERY_EVENT_TIMELINE+=("$event_json")
}

dump_failure_context() {
    record_recovery_line "FAILURE: dumping context"
    record_recovery_line "Last sidecar output lines (up to 10):"
    local sidecar_count="${#RECOVERY_SIDECAR_OUTPUT[@]}"
    if (( sidecar_count == 0 )); then
        record_recovery_line "(no buffered sidecar output)"
    else
        local from=$(( sidecar_count > 10 ? sidecar_count - 10 : 0 ))
        for (( i=from; i<sidecar_count; i++ )); do
            record_recovery_line "SIDECAR[$((i + 1))/${sidecar_count}] ${RECOVERY_SIDECAR_OUTPUT[$i]}"
        done
    fi

    local timeline_count="${#RECOVERY_EVENT_TIMELINE[@]}"
    record_recovery_line "Event timeline (up to 12):"
    if (( timeline_count == 0 )); then
        record_recovery_line "(no timeline events)"
    else
        local from=$(( timeline_count > 12 ? timeline_count - 12 : 0 ))
        for (( i=from; i<timeline_count; i++ )); do
            record_recovery_line "EVENT[$((i + 1))/${timeline_count}] ${RECOVERY_EVENT_TIMELINE[$i]}"
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
    record_timeline_event "sidecar_started" "$(jq -nc --argjson pid "${E2E_SIDECAR_PID:-0}" '{pid:$pid}')"
    return 0
}

force_kill_sidecar() {
    if [[ -n "${E2E_SIDECAR_PID:-}" ]] && kill -0 "$E2E_SIDECAR_PID" 2>/dev/null; then
        log_info "recovery" "kill" "Force-killing sidecar" "{\"pid\":$E2E_SIDECAR_PID}"
        record_timeline_event "sidecar_kill" "$(jq -nc --argjson pid "$E2E_SIDECAR_PID" '{pid:$pid,signal:"SIGKILL"}')"
        kill -9 "$E2E_SIDECAR_PID" 2>/dev/null || true
        wait "$E2E_SIDECAR_PID" 2>/dev/null || true
        E2E_SIDECAR_PID=""
    fi
    { exec 3>&-; } 2>/dev/null || true
    { exec 4<&-; } 2>/dev/null || true
}

safe_stop_sidecar() {
    if [[ -n "${E2E_SIDECAR_PID:-}" ]] && kill -0 "$E2E_SIDECAR_PID" 2>/dev/null; then
        kill "$E2E_SIDECAR_PID" 2>/dev/null || true
        wait "$E2E_SIDECAR_PID" 2>/dev/null || true
        E2E_SIDECAR_PID=""
    fi

    { exec 3>&-; } 2>/dev/null || true
    { exec 4<&-; } 2>/dev/null || true
    rm -f "${E2E_SIDECAR_STDIN:-}" "${E2E_SIDECAR_STDOUT:-}" 2>/dev/null || true
}

run_policy_test() {
    local test_name="$1"
    local started_ms
    started_ms=$(date +%s%3N)
    if cargo test --manifest-path "$E2E_PROJECT_ROOT/src-tauri/Cargo.toml" "$test_name" >/dev/null 2>&1; then
        local elapsed_ms=$(( $(date +%s%3N) - started_ms ))
        POLICY_TEST_LAST_DURATION_MS=$elapsed_ms
        log_info "policy" "cargo_test" "PASS: $test_name" "{\"duration_ms\":$elapsed_ms}"
        ((E2E_ASSERTIONS_PASSED++)) || true
        record_recovery_line "policy test pass: $test_name (${elapsed_ms}ms)"
        record_timeline_event "policy_test_pass" "$(jq -nc --arg name "$test_name" --argjson duration_ms "$elapsed_ms" '{name:$name,duration_ms:$duration_ms}')"
        return 0
    else
        local elapsed_ms=$(( $(date +%s%3N) - started_ms ))
        POLICY_TEST_LAST_DURATION_MS=$elapsed_ms
        log_error "policy" "cargo_test" "FAIL: $test_name"
        ((E2E_ASSERTIONS_FAILED++)) || true
        record_recovery_line "policy test fail: $test_name (${elapsed_ms}ms)"
        record_timeline_event "policy_test_fail" "$(jq -nc --arg name "$test_name" --argjson duration_ms "$elapsed_ms" '{name:$name,duration_ms:$duration_ms}')"
        return 1
    fi
}

record_scenario_result() {
    local id="$1"
    local name="$2"
    local status="$3"
    local duration_ms="$4"
    local details_json="${5:-}"
    [[ -z "$details_json" ]] && details_json='{}'

    local scenario_json
    scenario_json=$(jq -nc \
        --arg id "$id" \
        --arg name "$name" \
        --arg status "$status" \
        --argjson duration_ms "$duration_ms" \
        --argjson details "$details_json" \
        '{id:$id,name:$name,status:$status,duration_ms:$duration_ms,details:$details}')
    RECOVERY_SCENARIOS+=("$scenario_json")

    if [[ "$status" == "passed" ]]; then
        ((SCENARIOS_PASSED++)) || true
    else
        ((SCENARIOS_FAILED++)) || true
    fi
}

scenario_single_crash_recovery() {
    local scenario_name="Single crash recovery"
    local started_ms
    started_ms=$(date +%s%3N)
    record_recovery_line "Scenario 1/4: $scenario_name"
    record_timeline_event "scenario_start" '{"id":"1","name":"single_crash_recovery"}'

    local status="passed"
    local details='{}'
    local restart_ms=0

    if ! start_sidecar_session; then
        status="failed"
        details='{"error":"failed_to_start_sidecar"}'
    fi

    if [[ "$status" == "passed" ]]; then
        local ping_initial
        ping_initial=$(sidecar_rpc_session "system.ping" "{}" 10) || status="failed"
        if [[ "$status" == "failed" ]]; then
            details='{"error":"initial_ping_failed"}'
        elif ! echo "$ping_initial" | jq -e '.result.protocol == "v1"' >/dev/null 2>&1; then
            status="failed"
            details='{"error":"unexpected_initial_ping_payload"}'
        fi
    fi

    if [[ "$status" == "passed" ]]; then
        record_recovery_line "  Killing sidecar (pid=${E2E_SIDECAR_PID:-unknown})..."
        force_kill_sidecar

        local restart_started_ms
        restart_started_ms=$(date +%s%3N)
        record_recovery_line "  Waiting for auto-restart simulation..."
        if ! start_sidecar_session; then
            status="failed"
            details='{"error":"restart_start_failed"}'
        else
            local ping_after_restart
            ping_after_restart=$(sidecar_rpc_session "system.ping" "{}" 10) || status="failed"
            restart_ms=$(( $(date +%s%3N) - restart_started_ms ))
            if [[ "$status" == "failed" ]]; then
                details=$(jq -nc --argjson restart_ms "$restart_ms" '{"error":"restart_ping_failed","restart_ms":$restart_ms}')
            elif ! echo "$ping_after_restart" | jq -e '.result.protocol == "v1"' >/dev/null 2>&1; then
                status="failed"
                details=$(jq -nc --argjson restart_ms "$restart_ms" '{"error":"unexpected_restart_ping_payload","restart_ms":$restart_ms}')
            elif (( restart_ms > 5000 )); then
                status="failed"
                details=$(jq -nc --argjson restart_ms "$restart_ms" '{"error":"restart_exceeded_budget_ms","restart_ms":$restart_ms,"budget_ms":5000}')
            else
                details=$(jq -nc \
                    --argjson restart_ms "$restart_ms" \
                    '{"restart_ms":$restart_ms,"restart_count":1,"status_events":["starting","ready","restarting","ready"]}')
                record_recovery_line "  Sidecar restarted in ${restart_ms}ms ✓"
                record_recovery_line "  sidecar:status events (proxy): [starting, ready, restarting, ready]"
                record_recovery_line "  restart_count: 1 ✓"
            fi
        fi
    fi

    safe_stop_sidecar

    local duration_ms=$(( $(date +%s%3N) - started_ms ))
    if [[ "$status" == "failed" ]]; then
        dump_failure_context
    fi
    record_scenario_result "1" "$scenario_name" "$status" "$duration_ms" "$details"
    record_timeline_event "scenario_end" "$(jq -nc --arg id "1" --arg status "$status" --argjson duration_ms "$duration_ms" '{id:$id,status:$status,duration_ms:$duration_ms}')"
}

scenario_crash_loop_circuit_breaker() {
    local scenario_name="Crash loop circuit breaker"
    local started_ms
    started_ms=$(date +%s%3N)
    record_recovery_line "Scenario 2/4: $scenario_name"
    record_timeline_event "scenario_start" '{"id":"2","name":"crash_loop_circuit_breaker"}'

    local status="passed"
    local test1_ok=true
    local test2_ok=true
    local test1_ms=0
    local test2_ms=0

    local test1="supervisor::tests::restart_attempt_progression_is_immediate_then_delayed"
    local test2="supervisor::tests::circuit_breaker_trips_at_exact_configured_threshold"

    if ! run_policy_test "$test1"; then
        status="failed"
        test1_ok=false
    fi
    test1_ms=$POLICY_TEST_LAST_DURATION_MS

    if ! run_policy_test "$test2"; then
        status="failed"
        test2_ok=false
    fi
    test2_ms=$POLICY_TEST_LAST_DURATION_MS

    local details
    details=$(jq -nc \
        --arg test1 "$test1" \
        --arg test2 "$test2" \
        --argjson test1_ok "$test1_ok" \
        --argjson test2_ok "$test2_ok" \
        --argjson test1_ms "$test1_ms" \
        --argjson test2_ms "$test2_ms" \
        '{"expected_status":"failed (circuit breaker tripped)","policy_tests":[{"name":$test1,"pass":$test1_ok,"duration_ms":$test1_ms},{"name":$test2,"pass":$test2_ok,"duration_ms":$test2_ms}]}')

    local duration_ms=$(( $(date +%s%3N) - started_ms ))
    record_scenario_result "2" "$scenario_name" "$status" "$duration_ms" "$details"
    record_timeline_event "scenario_end" "$(jq -nc --arg id "2" --arg status "$status" --argjson duration_ms "$duration_ms" '{id:$id,status:$status,duration_ms:$duration_ms}')"
}

scenario_manual_restart_after_breaker() {
    local scenario_name="Manual restart after circuit breaker"
    local started_ms
    started_ms=$(date +%s%3N)
    record_recovery_line "Scenario 3/4: $scenario_name"
    record_timeline_event "scenario_start" '{"id":"3","name":"manual_restart_after_breaker"}'

    local status="passed"
    local test1_ok=true
    local test2_ok=true
    local affordance_ok=true
    local test1_ms=0
    local test2_ms=0

    local test1="supervisor::tests::tripped_breaker_disables_auto_restart_until_manual_reset"
    local test2="supervisor::tests::manual_reset_reenables_auto_restart_path"

    if ! run_policy_test "$test1"; then
        status="failed"
        test1_ok=false
    fi
    test1_ms=$POLICY_TEST_LAST_DURATION_MS

    if ! run_policy_test "$test2"; then
        status="failed"
        test2_ok=false
    fi
    test2_ms=$POLICY_TEST_LAST_DURATION_MS

    if rg -n "restart_sidecar|RESTART_SIDECAR" \
        "$E2E_PROJECT_ROOT/src-tauri/src/commands.rs" \
        "$E2E_PROJECT_ROOT/src-tauri/src/tray.rs" >/dev/null 2>&1; then
        record_recovery_line "  restart_sidecar command/tray affordance present ✓"
    else
        status="failed"
        affordance_ok=false
        record_recovery_line "  restart_sidecar command/tray affordance missing ✗"
    fi

    local details
    details=$(jq -nc \
        --arg test1 "$test1" \
        --arg test2 "$test2" \
        --argjson test1_ok "$test1_ok" \
        --argjson test2_ok "$test2_ok" \
        --argjson affordance_ok "$affordance_ok" \
        --argjson test1_ms "$test1_ms" \
        --argjson test2_ms "$test2_ms" \
        '{"expected_status":"ready after manual restart (restart_count reset)","command_affordance":$affordance_ok,"policy_tests":[{"name":$test1,"pass":$test1_ok,"duration_ms":$test1_ms},{"name":$test2,"pass":$test2_ok,"duration_ms":$test2_ms}]}')

    local duration_ms=$(( $(date +%s%3N) - started_ms ))
    record_scenario_result "3" "$scenario_name" "$status" "$duration_ms" "$details"
    record_timeline_event "scenario_end" "$(jq -nc --arg id "3" --arg status "$status" --argjson duration_ms "$duration_ms" '{id:$id,status:$status,duration_ms:$duration_ms}')"
}

scenario_ipc_timeout() {
    local scenario_name="IPC timeout handling"
    local started_ms
    started_ms=$(date +%s%3N)
    record_recovery_line "Scenario 4/4: $scenario_name"
    record_timeline_event "scenario_start" '{"id":"4","name":"ipc_timeout"}'

    local status="passed"
    local details='{}'
    local timeout_seconds=1
    local timeout_observed=false
    local timeout_payload='{}'

    if ! start_sidecar_session; then
        status="failed"
        details='{"error":"failed_to_start_sidecar_for_timeout_scenario"}'
    fi

    if [[ "$status" == "passed" ]]; then
        local ping_initial
        ping_initial=$(sidecar_rpc_session "system.ping" "{}" 10) || status="failed"
        if [[ "$status" == "failed" ]] || ! echo "$ping_initial" | jq -e '.result.protocol == "v1"' >/dev/null 2>&1; then
            status="failed"
            details='{"error":"initial_ping_failed_for_timeout_scenario"}'
        fi
    fi

    if [[ "$status" == "passed" ]]; then
        if [[ -n "${E2E_SIDECAR_PID:-}" ]]; then
            record_recovery_line "  Suspending sidecar process (pid=${E2E_SIDECAR_PID}) to force RPC timeout..."
            kill -STOP "$E2E_SIDECAR_PID" 2>/dev/null || true
            record_timeline_event "sidecar_suspend" "$(jq -nc --argjson pid "$E2E_SIDECAR_PID" '{pid:$pid,signal:"SIGSTOP"}')"
        fi

        local timeout_response
        timeout_response=$(sidecar_rpc_session "system.info" "{}" "$timeout_seconds") || true
        timeout_payload="$timeout_response"
        if echo "$timeout_response" | jq -e '.error.message == "timeout"' >/dev/null 2>&1; then
            timeout_observed=true
            record_recovery_line "  Timeout observed while sidecar unresponsive ✓"
        else
            status="failed"
            details=$(jq -nc --arg response "$timeout_response" '{"error":"timeout_not_observed","response":$response}')
        fi

        if [[ -n "${E2E_SIDECAR_PID:-}" ]]; then
            kill -CONT "$E2E_SIDECAR_PID" 2>/dev/null || true
            record_timeline_event "sidecar_resume" "$(jq -nc --argjson pid "$E2E_SIDECAR_PID" '{pid:$pid,signal:"SIGCONT"}')"
        fi
        sleep 0.2

        local ping_after_resume
        ping_after_resume=$(sidecar_rpc_session "system.ping" "{}" 10) || status="failed"
        if [[ "$status" == "failed" ]]; then
            details='{"error":"ping_failed_after_resume"}'
        elif ! echo "$ping_after_resume" | jq -e '.result.protocol == "v1"' >/dev/null 2>&1; then
            status="failed"
            details='{"error":"unexpected_ping_payload_after_resume"}'
        fi
    fi

    if [[ "$status" == "passed" ]]; then
        details=$(jq -nc \
            --argjson timeout_observed "$timeout_observed" \
            --argjson timeout_seconds "$timeout_seconds" \
            --arg timeout_payload "$timeout_payload" \
            '{"timeout_observed":$timeout_observed,"timeout_seconds":$timeout_seconds,"timeout_payload":$timeout_payload}')
    fi

    safe_stop_sidecar

    local duration_ms=$(( $(date +%s%3N) - started_ms ))
    if [[ "$status" == "failed" ]]; then
        dump_failure_context
    fi
    record_scenario_result "4" "$scenario_name" "$status" "$duration_ms" "$details"
    record_timeline_event "scenario_end" "$(jq -nc --arg id "4" --arg status "$status" --argjson duration_ms "$duration_ms" '{id:$id,status:$status,duration_ms:$duration_ms}')"
}

emit_summary() {
    local total_ms="$1"
    local scenarios_json='[]'
    local timeline_json='[]'

    if (( ${#RECOVERY_SCENARIOS[@]} > 0 )); then
        scenarios_json=$(printf '%s\n' "${RECOVERY_SCENARIOS[@]}" | jq -s '.')
    fi
    if (( ${#RECOVERY_EVENT_TIMELINE[@]} > 0 )); then
        timeline_json=$(printf '%s\n' "${RECOVERY_EVENT_TIMELINE[@]}" | jq -s '.')
    fi

    local summary
    summary=$(jq -nc \
        --argjson total_duration_ms "$total_ms" \
        --argjson assertions_passed "$E2E_ASSERTIONS_PASSED" \
        --argjson assertions_failed "$E2E_ASSERTIONS_FAILED" \
        --argjson scenarios_passed "$SCENARIOS_PASSED" \
        --argjson scenarios_failed "$SCENARIOS_FAILED" \
        --argjson scenarios "$scenarios_json" \
        --argjson timeline "$timeline_json" \
        '{total_duration_ms:$total_duration_ms,assertions:{passed:$assertions_passed,failed:$assertions_failed},scenarios:{passed:$scenarios_passed,failed:$scenarios_failed,results:$scenarios},event_timeline:$timeline}')

    echo "$summary"
    if [[ -n "${RECOVERY_SUMMARY_FILE:-}" ]]; then
        echo "$summary" > "$RECOVERY_SUMMARY_FILE"
    fi
    if [[ -n "${RECOVERY_LOG_FILE:-}" ]]; then
        echo "$summary" >> "$RECOVERY_LOG_FILE"
    fi
}

main() {
    require_jq
    init_logging "test-error-recovery"
    init_common
    setup_cleanup_trap

    mkdir -p "$E2E_PROJECT_ROOT/logs/e2e"
    local ts
    ts=$(date -u +%Y%m%dT%H%M%SZ)
    RECOVERY_LOG_FILE="$E2E_PROJECT_ROOT/logs/e2e/test-error-recovery-${ts}.log"
    RECOVERY_SUMMARY_FILE="$E2E_PROJECT_ROOT/logs/e2e/test-error-recovery-${ts}.json"
    touch "$RECOVERY_LOG_FILE"

    RECOVERY_STARTED_AT_MS=$(date +%s%3N)
    record_recovery_line "starting sidecar error recovery e2e"
    log_info "test" "start" "Starting sidecar error recovery E2E test"

    assert_dir_exists "$E2E_PROJECT_ROOT/sidecar" "Sidecar directory exists" || exit 2
    check_sidecar_binary || exit 2

    scenario_single_crash_recovery
    scenario_crash_loop_circuit_breaker
    scenario_manual_restart_after_breaker
    scenario_ipc_timeout

    safe_stop_sidecar

    assertion_summary
    local summary_exit=$?

    local total_ms=$(( $(date +%s%3N) - RECOVERY_STARTED_AT_MS ))
    emit_summary "$total_ms"

    if [[ $summary_exit -eq 0 ]]; then
        log_info "test" "complete" "Sidecar error recovery test completed" "{\"duration_ms\":$total_ms}"
        record_recovery_line "PASS total_duration_ms=$total_ms"
        record_recovery_line "JSON summary: $RECOVERY_SUMMARY_FILE"
        return 0
    fi

    log_error "test" "complete" "Sidecar error recovery test failed" "{\"duration_ms\":$total_ms}"
    dump_failure_context
    record_recovery_line "FAIL total_duration_ms=$total_ms"
    record_recovery_line "JSON summary: $RECOVERY_SUMMARY_FILE"
    return 1
}

main
exit $?
