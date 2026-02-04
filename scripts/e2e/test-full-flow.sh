#!/usr/bin/env bash
#
# E2E Test: Full Transcription Flow
#
# Tests the happy-path flow:
# 1. Sidecar startup
# 2. Model status check
# 3. Audio device enumeration
# 4. System ping verification
#
# Note: Full transcription testing requires audio hardware and model.
# This test validates the IPC layer and basic functionality.
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

    # Phase 2: Sidecar ping test
    log_info "sidecar" "ping_test" "Testing sidecar ping"

    local ping_start
    ping_start=$(start_timer)

    local ping_result
    ping_result=$(sidecar_rpc "system.ping" "{}" 10) || {
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
    devices_result=$(sidecar_rpc "audio.list_devices" "{}" 10) || {
        log_error "audio" "list_devices" "Device enumeration failed"
        exit 1
    }

    log_with_duration "INFO" "audio" "list_devices" "Devices enumerated" "{}" "$devices_start"

    # Check response has devices array (may be empty in headless environment)
    local devices_count
    devices_count=$(echo "$devices_result" | jq '.result.devices | length')
    log_info "audio" "device_count" "Audio devices found" "{\"count\":$devices_count}"

    # Phase 4: Model status (may not be implemented yet)
    log_info "model" "status_check" "Checking model status"

    local model_result
    model_result=$(sidecar_rpc "model.status" "{}" 10) 2>/dev/null || true

    if echo "$model_result" | jq -e '.result' >/dev/null 2>&1; then
        local model_status
        model_status=$(echo "$model_result" | jq -r '.result.status // "unknown"')
        log_info "model" "status" "Model status retrieved" "{\"status\":\"$model_status\"}"
    else
        log_warn "model" "status_check" "Model status endpoint not available (expected if not yet implemented)"
    fi

    # Phase 5: Verify sidecar startup time is within budget
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
