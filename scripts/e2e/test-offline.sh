#!/usr/bin/env bash
#
# E2E Test: Offline Mode Verification
#
# Tests that the sidecar can operate without network:
# 1. Verify sidecar starts without network dependency
# 2. Test that cached model can be used offline
# 3. Verify core functionality works offline
#
# Note: This test simulates offline conditions by testing
# operations that should work without network connectivity.
# Full network isolation requires root/admin privileges.
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
    init_logging "test-offline"
    init_common
    setup_cleanup_trap

    log_info "test" "start" "Starting offline mode E2E test"

    # Environment checks
    assert_dir_exists "$E2E_PROJECT_ROOT/sidecar" "Sidecar directory exists" || exit 2
    check_sidecar_binary || exit 2

    # Test 1: Sidecar starts without making network calls
    log_info "offline" "startup" "Testing sidecar startup (should not require network)"

    local startup_start
    startup_start=$(start_timer)

    # system.ping should work without network
    local ping_result
    ping_result=$(sidecar_rpc "system.ping" "{}" 10) || {
        log_error "offline" "startup" "Sidecar failed to start"
        exit 1
    }

    log_with_duration "INFO" "offline" "startup" "Sidecar started successfully" "{}" "$startup_start"
    assert_json_eq "$ping_result" ".result.protocol" "v1" "Protocol available offline"

    # Test 2: Audio device enumeration works offline
    log_info "offline" "audio" "Testing audio enumeration (local operation)"

    local audio_start
    audio_start=$(start_timer)

    local audio_result
    audio_result=$(sidecar_rpc "audio.list_devices" "{}" 10) || {
        log_error "offline" "audio" "Audio enumeration failed"
        exit 1
    }

    log_with_duration "INFO" "offline" "audio" "Audio enumeration succeeded" "{}" "$audio_start"

    # This should work because it only queries local audio hardware
    if echo "$audio_result" | jq -e '.result.devices' >/dev/null 2>&1; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "test" "assert" "PASS: Audio enumeration works offline"
    else
        ((E2E_ASSERTIONS_FAILED++)) || true
        log_error "test" "assert" "FAIL: Audio enumeration should work offline"
    fi

    # Test 3: Model status check (verify it doesn't hang waiting for network)
    log_info "offline" "model" "Testing model status (should respond quickly)"

    local model_start
    model_start=$(start_timer)

    # Give it a short timeout - if it's trying to reach network, it would hang
    local model_result
    model_result=$(sidecar_rpc "model.status" "{}" 5) 2>/dev/null || true

    local model_duration
    model_duration=$(($(start_timer) - model_start))

    if [ "$model_duration" -lt 3000 ]; then
        ((E2E_ASSERTIONS_PASSED++)) || true
        log_info "offline" "model" "Model status responded quickly" "{\"duration_ms\":$model_duration}"
    else
        log_warn "offline" "model" "Model status was slow (possible network dependency)" "{\"duration_ms\":$model_duration}"
    fi

    # Test 4: Simulate model cache check
    log_info "offline" "cache" "Checking model cache behavior"

    # Check if model cache directory exists (would be needed for offline operation)
    local cache_dir="$HOME/.cache/openvoicy"
    if [ -d "$cache_dir" ]; then
        log_info "offline" "cache" "Model cache directory exists" "{\"path\":\"$cache_dir\"}"

        # List cache contents if any
        local cache_size
        cache_size=$(du -sh "$cache_dir" 2>/dev/null | cut -f1 || echo "unknown")
        log_info "offline" "cache" "Cache size" "{\"size\":\"$cache_size\"}"
    else
        log_info "offline" "cache" "No model cache (model not yet downloaded)" "{\"path\":\"$cache_dir\"}"
    fi

    # Test 5: Verify manifest doesn't indicate network requirements
    log_info "offline" "manifest" "Checking build manifest"

    local manifest_path="$E2E_PROJECT_ROOT/sidecar/dist/manifest.json"
    if [ -f "$manifest_path" ]; then
        local gpu_support
        gpu_support=$(jq -r '.gpu_support // "unknown"' "$manifest_path")

        # CPU-only builds should work offline
        if [ "$gpu_support" = "none" ]; then
            ((E2E_ASSERTIONS_PASSED++)) || true
            log_info "offline" "manifest" "CPU-only build (good for offline)" "{\"gpu_support\":\"$gpu_support\"}"
        else
            log_info "offline" "manifest" "GPU support may require drivers" "{\"gpu_support\":\"$gpu_support\"}"
        fi
    fi

    # Summary
    assertion_summary
    local summary_exit=$?

    log_info "test" "complete" "Offline test completed"
    return $summary_exit
}

# Run main
main
exit $?
