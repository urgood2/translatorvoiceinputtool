#!/usr/bin/env bash
#
# E2E Test Assertion Library
# Provides assertion functions for E2E tests with structured logging.
#
# Usage:
#   source scripts/e2e/lib/log.sh
#   source scripts/e2e/lib/assert.sh
#   assert_eq "expected" "$actual" "Values should match"
#

set -euo pipefail

# Track assertion counts
E2E_ASSERTIONS_PASSED=0
E2E_ASSERTIONS_FAILED=0

# Assert two values are equal
# Args: expected, actual, msg
# Returns: 0 on pass, 1 on fail
assert_eq() {
    local expected="$1"
    local actual="$2"
    local msg="${3:-Values should be equal}"

    if [ "$expected" = "$actual" ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_eq" "PASS: $msg" "{\"expected\":\"$expected\",\"actual\":\"$actual\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_eq" "FAIL: $msg" "{\"expected\":\"$expected\",\"actual\":\"$actual\"}"
        return 1
    fi
}

# Assert two values are not equal
# Args: unexpected, actual, msg
assert_ne() {
    local unexpected="$1"
    local actual="$2"
    local msg="${3:-Values should not be equal}"

    if [ "$unexpected" != "$actual" ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_ne" "PASS: $msg" "{\"unexpected\":\"$unexpected\",\"actual\":\"$actual\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_ne" "FAIL: $msg" "{\"unexpected\":\"$unexpected\",\"actual\":\"$actual\"}"
        return 1
    fi
}

# Assert string contains substring
# Args: haystack, needle, msg
assert_contains() {
    local haystack="$1"
    local needle="$2"
    local msg="${3:-String should contain substring}"

    if [[ "$haystack" == *"$needle"* ]]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_contains" "PASS: $msg" "{\"needle\":\"$needle\",\"found\":true}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_contains" "FAIL: $msg" "{\"needle\":\"$needle\",\"found\":false,\"haystack\":\"${haystack:0:100}\"}"
        return 1
    fi
}

# Assert string does not contain substring
# Args: haystack, needle, msg
assert_not_contains() {
    local haystack="$1"
    local needle="$2"
    local msg="${3:-String should not contain substring}"

    if [[ "$haystack" != *"$needle"* ]]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_not_contains" "PASS: $msg" "{\"needle\":\"$needle\",\"found\":false}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_not_contains" "FAIL: $msg" "{\"needle\":\"$needle\",\"found\":true}"
        return 1
    fi
}

# Assert file exists
# Args: filepath, msg
assert_file_exists() {
    local filepath="$1"
    local msg="${2:-File should exist}"

    if [ -f "$filepath" ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_file_exists" "PASS: $msg" "{\"path\":\"$filepath\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_file_exists" "FAIL: $msg" "{\"path\":\"$filepath\"}"
        return 1
    fi
}

# Assert directory exists
# Args: dirpath, msg
assert_dir_exists() {
    local dirpath="$1"
    local msg="${2:-Directory should exist}"

    if [ -d "$dirpath" ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_dir_exists" "PASS: $msg" "{\"path\":\"$dirpath\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_dir_exists" "FAIL: $msg" "{\"path\":\"$dirpath\"}"
        return 1
    fi
}

# Assert process is running
# Args: process_name_or_pid, msg
assert_process_running() {
    local target="$1"
    local msg="${2:-Process should be running}"

    local running=false
    if [[ "$target" =~ ^[0-9]+$ ]]; then
        # PID
        if kill -0 "$target" 2>/dev/null; then
            running=true
        fi
    else
        # Process name
        if pgrep -f "$target" >/dev/null 2>&1; then
            running=true
        fi
    fi

    if [ "$running" = true ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_process_running" "PASS: $msg" "{\"target\":\"$target\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_process_running" "FAIL: $msg" "{\"target\":\"$target\"}"
        return 1
    fi
}

# Assert command succeeds (exit code 0)
# Args: command, msg
assert_cmd_succeeds() {
    local cmd="$1"
    local msg="${2:-Command should succeed}"

    if eval "$cmd" >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_cmd_succeeds" "PASS: $msg" "{\"command\":\"$cmd\"}"
        return 0
    else
        local exit_code=$?
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_cmd_succeeds" "FAIL: $msg" "{\"command\":\"$cmd\",\"exit_code\":$exit_code}"
        return 1
    fi
}

# Assert command fails (non-zero exit code)
# Args: command, msg
assert_cmd_fails() {
    local cmd="$1"
    local msg="${2:-Command should fail}"

    if ! eval "$cmd" >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_cmd_fails" "PASS: $msg" "{\"command\":\"$cmd\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_cmd_fails" "FAIL: $msg" "{\"command\":\"$cmd\"}"
        return 1
    fi
}

# Assert JSON field equals value
# Args: json_string, jq_path, expected_value, msg
assert_json_eq() {
    local json="$1"
    local path="$2"
    local expected="$3"
    local msg="${4:-JSON field should equal expected value}"

    local actual
    actual=$(echo "$json" | jq -r "$path" 2>/dev/null || echo "__JQ_ERROR__")

    if [ "$actual" = "$expected" ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_json_eq" "PASS: $msg" "{\"path\":\"$path\",\"expected\":\"$expected\",\"actual\":\"$actual\"}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_json_eq" "FAIL: $msg" "{\"path\":\"$path\",\"expected\":\"$expected\",\"actual\":\"$actual\"}"
        return 1
    fi
}

# Assert value is within numeric range
# Args: actual, min, max, msg
assert_in_range() {
    local actual="$1"
    local min="$2"
    local max="$3"
    local msg="${4:-Value should be in range}"

    if (( $(echo "$actual >= $min" | bc -l) )) && (( $(echo "$actual <= $max" | bc -l) )); then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_in_range" "PASS: $msg" "{\"actual\":$actual,\"min\":$min,\"max\":$max}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_in_range" "FAIL: $msg" "{\"actual\":$actual,\"min\":$min,\"max\":$max}"
        return 1
    fi
}

# Assert duration is under limit
# Args: duration_ms, limit_ms, msg
assert_duration_under() {
    local duration="$1"
    local limit="$2"
    local msg="${3:-Duration should be under limit}"

    if (( duration < limit )); then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert_duration_under" "PASS: $msg" "{\"duration_ms\":$duration,\"limit_ms\":$limit}"
        return 0
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert_duration_under" "FAIL: $msg" "{\"duration_ms\":$duration,\"limit_ms\":$limit}"
        return 1
    fi
}

# Wait for condition with timeout
# Args: condition_cmd, timeout_seconds, poll_interval_ms, msg
wait_for() {
    local condition="$1"
    local timeout="${2:-30}"
    local poll_interval="${3:-500}"
    local msg="${4:-Waiting for condition}"

    local start_time
    start_time=$(date +%s)
    local end_time=$((start_time + timeout))

    log_info "test" "wait_for" "Waiting: $msg" "{\"timeout\":$timeout,\"poll_interval_ms\":$poll_interval}"

    while [ "$(date +%s)" -lt "$end_time" ]; do
        if eval "$condition" >/dev/null 2>&1; then
            local duration=$(($(date +%s) - start_time))
            log_info "test" "wait_for" "Condition met: $msg" "{\"waited_seconds\":$duration}"
            return 0
        fi
        sleep "$(echo "scale=3; $poll_interval / 1000" | bc)"
    done

    log_error "test" "wait_for" "Timeout: $msg" "{\"timeout\":$timeout}"
    return 1
}

# Print assertion summary
assertion_summary() {
    local total=$((E2E_ASSERTIONS_PASSED + E2E_ASSERTIONS_FAILED))
    local result="PASS"
    [ "$E2E_ASSERTIONS_FAILED" -gt 0 ] && result="FAIL"

    log_info "test" "summary" "Assertion summary: $result" "{\"passed\":$E2E_ASSERTIONS_PASSED,\"failed\":$E2E_ASSERTIONS_FAILED,\"total\":$total}"

    echo ""
    echo "=== Assertions ==="
    echo "Passed: $E2E_ASSERTIONS_PASSED"
    echo "Failed: $E2E_ASSERTIONS_FAILED"
    echo "Total:  $total"

    return "$E2E_ASSERTIONS_FAILED"
}
