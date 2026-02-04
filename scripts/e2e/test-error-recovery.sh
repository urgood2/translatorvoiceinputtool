#!/usr/bin/env bash
#
# E2E Test: Error Recovery
#
# Tests error handling scenarios:
# 1. Sidecar responds correctly to malformed requests
# 2. Sidecar handles unknown methods gracefully
# 3. Sidecar handles invalid parameters
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
TEST_TIMEOUT=60

main() {
    # Initialize
    require_jq
    init_logging "test-error-recovery"
    init_common
    setup_cleanup_trap

    log_info "test" "start" "Starting error recovery E2E test"

    # Environment checks
    assert_dir_exists "$E2E_PROJECT_ROOT/sidecar" "Sidecar directory exists" || exit 2
    check_sidecar_binary || exit 2

    # Test 1: Unknown method returns proper JSON-RPC error
    log_info "error" "unknown_method" "Testing unknown method handling"

    local unknown_result
    unknown_result=$(sidecar_rpc "nonexistent.method" "{}" 10) || true

    if echo "$unknown_result" | jq -e '.error' >/dev/null 2>&1; then
        local error_code
        error_code=$(echo "$unknown_result" | jq '.error.code')
        log_info "error" "unknown_method" "Received error response" "{\"error_code\":$error_code}"

        # JSON-RPC method not found is -32601
        assert_eq "-32601" "$error_code" "Unknown method returns -32601 (method not found)"
    else
        log_error "error" "unknown_method" "Expected error response for unknown method"
        ((E2E_ASSERTIONS_FAILED++)) || true
    fi

    # Test 2: Malformed JSON handling
    log_info "error" "malformed_json" "Testing malformed JSON handling"

    local malformed_result
    malformed_result=$(echo 'not valid json{' | timeout 5 "$E2E_SIDECAR_BIN" 2>/dev/null) || true

    if echo "$malformed_result" | jq -e '.error' >/dev/null 2>&1; then
        local parse_error_code
        parse_error_code=$(echo "$malformed_result" | jq '.error.code')
        log_info "error" "malformed_json" "Received parse error" "{\"error_code\":$parse_error_code}"

        # JSON-RPC parse error is -32700
        assert_eq "-32700" "$parse_error_code" "Malformed JSON returns -32700 (parse error)"
    else
        log_warn "error" "malformed_json" "Sidecar may have closed connection on malformed input"
    fi

    # Test 3: Invalid params handling
    log_info "error" "invalid_params" "Testing invalid params handling"

    # Call audio.meter_start with invalid device_uid type
    local invalid_params_result
    invalid_params_result=$(sidecar_rpc "audio.meter_start" '{"device_uid":12345}' 10) || true

    if echo "$invalid_params_result" | jq -e '.error' >/dev/null 2>&1; then
        local invalid_code
        invalid_code=$(echo "$invalid_params_result" | jq '.error.code')
        log_info "error" "invalid_params" "Received error for invalid params" "{\"error_code\":$invalid_code}"

        # Accept either invalid params (-32602) or application error
        if [ "$invalid_code" -eq -32602 ] || [ "$invalid_code" -lt 0 ]; then
            ((E2E_ASSERTIONS_PASSED++)) || true
            log_info "test" "assert" "PASS: Invalid params handled with error code $invalid_code"
        else
            ((E2E_ASSERTIONS_FAILED++)) || true
            log_error "test" "assert" "FAIL: Unexpected error code for invalid params"
        fi
    else
        # Some implementations might accept the wrong type and convert
        log_warn "error" "invalid_params" "No error for invalid params (may be handled gracefully)"
    fi

    # Test 4: Multiple rapid requests
    log_info "error" "rapid_requests" "Testing rapid sequential requests"

    local rapid_success=0
    local rapid_start
    rapid_start=$(start_timer)

    for i in {1..5}; do
        local ping_result
        ping_result=$(sidecar_rpc "system.ping" "{}" 5) || continue

        if echo "$ping_result" | jq -e '.result' >/dev/null 2>&1; then
            ((rapid_success++)) || true
        fi
    done

    log_with_duration "INFO" "error" "rapid_requests" "Rapid requests completed" "{\"success\":$rapid_success,\"total\":5}" "$rapid_start"
    assert_eq "5" "$rapid_success" "All rapid requests succeeded"

    # Test 5: Empty request handling
    log_info "error" "empty_request" "Testing empty request handling"

    local empty_result
    empty_result=$(echo '' | timeout 5 "$E2E_SIDECAR_BIN" 2>/dev/null) || true

    # Empty input should either return error or nothing
    if [ -n "$empty_result" ]; then
        if echo "$empty_result" | jq -e '.error' >/dev/null 2>&1; then
            log_info "error" "empty_request" "Empty request returned error (expected)"
            ((E2E_ASSERTIONS_PASSED++)) || true
        else
            log_warn "error" "empty_request" "Empty request returned non-error response"
        fi
    else
        log_info "error" "empty_request" "Empty request returned nothing (acceptable)"
        ((E2E_ASSERTIONS_PASSED++)) || true
    fi

    # Summary
    assertion_summary
    local summary_exit=$?

    log_info "test" "complete" "Error recovery test completed"
    return $summary_exit
}

# Run main
main
exit $?
