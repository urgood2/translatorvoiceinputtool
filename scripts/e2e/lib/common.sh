#!/usr/bin/env bash
#
# E2E Test Common Utilities
# Provides helpers for sidecar IPC, process management, and platform detection.
#
# Usage:
#   source scripts/e2e/lib/log.sh
#   source scripts/e2e/lib/common.sh
#

set -euo pipefail

# Project paths
E2E_PROJECT_ROOT=""
E2E_SIDECAR_BIN=""
E2E_SIDECAR_PID=""
E2E_SIDECAR_STDIN=""
E2E_SIDECAR_STDOUT=""

# Platform detection
E2E_PLATFORM=""
E2E_ARCH=""

# Initialize common paths and detect platform
init_common() {
    # Find project root by looking for sidecar directory
    # This handles both bash (BASH_SOURCE) and zsh (no BASH_SOURCE) environments
    local script_dir=""

    if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
        script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
        E2E_PROJECT_ROOT="$(cd "$script_dir/../../.." && pwd)"
    elif [[ -n "${0:-}" ]] && [[ -f "$0" ]]; then
        script_dir="$(cd "$(dirname "$0")" && pwd)"
        E2E_PROJECT_ROOT="$(cd "$script_dir/../.." && pwd)"
    else
        # Fallback: search upward for sidecar directory from cwd
        E2E_PROJECT_ROOT="$(pwd)"
        while [[ "$E2E_PROJECT_ROOT" != "/" ]] && [[ ! -d "$E2E_PROJECT_ROOT/sidecar" ]]; do
            E2E_PROJECT_ROOT="$(dirname "$E2E_PROJECT_ROOT")"
        done
        if [[ "$E2E_PROJECT_ROOT" == "/" ]]; then
            # Last resort: try common locations
            if [[ -d "/data/projects/translatorvoiceinputtool/sidecar" ]]; then
                E2E_PROJECT_ROOT="/data/projects/translatorvoiceinputtool"
            else
                echo "ERROR: Could not find project root" >&2
                return 1
            fi
        fi
    fi

    # Detect platform
    case "$(uname -s)" in
        Linux)  E2E_PLATFORM="linux" ;;
        Darwin) E2E_PLATFORM="macos" ;;
        MINGW*|CYGWIN*|MSYS*) E2E_PLATFORM="windows" ;;
        *) E2E_PLATFORM="unknown" ;;
    esac

    case "$(uname -m)" in
        x86_64)  E2E_ARCH="x64" ;;
        aarch64) E2E_ARCH="arm64" ;;
        arm64)   E2E_ARCH="arm64" ;;
        *) E2E_ARCH="unknown" ;;
    esac

    # Set sidecar binary path
    if [ "$E2E_PLATFORM" = "windows" ]; then
        E2E_SIDECAR_BIN="$E2E_PROJECT_ROOT/sidecar/dist/openvoicy-sidecar.exe"
    else
        E2E_SIDECAR_BIN="$E2E_PROJECT_ROOT/sidecar/dist/openvoicy-sidecar"
    fi

    log_info "setup" "platform" "Platform detected" "{\"os\":\"$E2E_PLATFORM\",\"arch\":\"$E2E_ARCH\"}"
}

# Check if sidecar binary exists
check_sidecar_binary() {
    if [ ! -f "$E2E_SIDECAR_BIN" ]; then
        log_error "setup" "sidecar_check" "Sidecar binary not found" "{\"path\":\"$E2E_SIDECAR_BIN\"}"
        echo "ERROR: Sidecar binary not found at: $E2E_SIDECAR_BIN"
        echo "Run ./scripts/build-sidecar.sh first"
        return 1
    fi
    log_info "setup" "sidecar_check" "Sidecar binary found" "{\"path\":\"$E2E_SIDECAR_BIN\"}"
    return 0
}

# Start sidecar process for IPC testing
# Returns: 0 on success, sets E2E_SIDECAR_PID
start_sidecar() {
    check_sidecar_binary || return 1

    log_info "sidecar" "start" "Starting sidecar process"

    # Create named pipes for communication
    local tmpdir
    tmpdir=$(mktemp -d)
    E2E_SIDECAR_STDIN="$tmpdir/stdin"
    E2E_SIDECAR_STDOUT="$tmpdir/stdout"
    mkfifo "$E2E_SIDECAR_STDIN"
    mkfifo "$E2E_SIDECAR_STDOUT"

    # Start sidecar with pipes
    "$E2E_SIDECAR_BIN" < "$E2E_SIDECAR_STDIN" > "$E2E_SIDECAR_STDOUT" 2>&1 &
    E2E_SIDECAR_PID=$!

    # Keep stdin pipe open
    exec 3>"$E2E_SIDECAR_STDIN"

    # Give it a moment to start
    sleep 0.5

    if ! kill -0 "$E2E_SIDECAR_PID" 2>/dev/null; then
        log_error "sidecar" "start" "Sidecar failed to start"
        return 1
    fi

    log_info "sidecar" "start" "Sidecar started" "{\"pid\":$E2E_SIDECAR_PID}"
    return 0
}

# Stop sidecar process
stop_sidecar() {
    if [ -n "$E2E_SIDECAR_PID" ] && kill -0 "$E2E_SIDECAR_PID" 2>/dev/null; then
        log_info "sidecar" "stop" "Stopping sidecar" "{\"pid\":$E2E_SIDECAR_PID}"
        kill "$E2E_SIDECAR_PID" 2>/dev/null || true
        wait "$E2E_SIDECAR_PID" 2>/dev/null || true
        E2E_SIDECAR_PID=""
    fi

    # Close stdin pipe
    exec 3>&- 2>/dev/null || true

    # Cleanup temp files
    rm -f "$E2E_SIDECAR_STDIN" "$E2E_SIDECAR_STDOUT" 2>/dev/null || true
}

# Send JSON-RPC request to sidecar and get response
# Args: method, [params_json], [timeout_seconds]
# Returns: response JSON on stdout
sidecar_rpc() {
    local method="$1"
    local params="${2:-{}}"
    local timeout="${3:-10}"

    local request_id
    request_id=$((RANDOM * RANDOM))

    local request
    request=$(jq -nc \
        --arg method "$method" \
        --argjson params "$params" \
        --argjson id "$request_id" \
        '{jsonrpc:"2.0",id:$id,method:$method,params:$params}')

    log_debug "ipc" "request" "Sending RPC request" "{\"method\":\"$method\",\"id\":$request_id}"

    local raw_response
    raw_response=$(echo "$request" | timeout "$timeout" "$E2E_SIDECAR_BIN" 2>/dev/null || echo '{"error":"timeout"}')

    # Extract only the JSON-RPC response line (contains "jsonrpc")
    # Use pure bash for portability (grep/awk may be aliased in some environments)
    local response=""
    while IFS= read -r line; do
        if [[ "$line" == *'"jsonrpc"'* ]]; then
            response="$line"
            break
        fi
    done <<< "$raw_response"

    # If no valid JSON-RPC line found, check for timeout
    if [ -z "$response" ]; then
        if echo "$raw_response" | grep -q '"error":"timeout"'; then
            log_error "ipc" "response" "RPC timeout" "{\"method\":\"$method\",\"timeout\":$timeout}"
            return 1
        fi
        # Return raw response as fallback (might be error JSON)
        response="$raw_response"
    fi

    log_debug "ipc" "response" "RPC response received" "{\"method\":\"$method\"}"
    echo "$response"
}

# Send RPC and verify success (has "result" field)
# Args: method, [params_json], [timeout_seconds]
# Returns: result field on stdout
sidecar_rpc_ok() {
    local method="$1"
    local params="${2:-{}}"
    local timeout="${3:-10}"

    local response
    response=$(sidecar_rpc "$method" "$params" "$timeout")

    if echo "$response" | jq -e '.result' >/dev/null 2>&1; then
        echo "$response" | jq -c '.result'
        return 0
    else
        local error
        error=$(echo "$response" | jq -c '.error // "unknown error"')
        log_error "ipc" "rpc_ok" "RPC failed" "{\"method\":\"$method\",\"error\":$error}"
        return 1
    fi
}

# Test sidecar connectivity with system.ping
test_sidecar_ping() {
    log_info "test" "ping" "Testing sidecar connectivity"

    local start_time
    start_time=$(start_timer)

    local result
    result=$(sidecar_rpc_ok "system.ping" "{}")

    if [ $? -eq 0 ]; then
        log_with_duration "INFO" "test" "ping" "Sidecar ping successful" "$result" "$start_time"
        return 0
    else
        log_error "test" "ping" "Sidecar ping failed"
        return 1
    fi
}

# Get audio device list from sidecar
get_audio_devices() {
    sidecar_rpc_ok "audio.list_devices" "{}"
}

# Get model status from sidecar
get_model_status() {
    sidecar_rpc_ok "model.status" "{}"
}

# Cleanup handler for trap
cleanup() {
    local exit_code=$?
    log_info "cleanup" "start" "Running cleanup"

    stop_sidecar

    if [ -n "${E2E_LOG_JSON:-}" ]; then
        finalize_logging "$exit_code"
    fi

    return "$exit_code"
}

# Set up cleanup trap
setup_cleanup_trap() {
    trap cleanup EXIT INT TERM
}

# Check if jq is available
require_jq() {
    if ! command -v jq &>/dev/null; then
        echo "ERROR: jq is required but not installed"
        echo "Install with: apt-get install jq (Linux) or brew install jq (macOS)"
        exit 2
    fi
}

# Create temp directory for test artifacts
create_temp_dir() {
    local prefix="${1:-e2e-test}"
    mktemp -d -t "${prefix}-XXXXXX"
}

# Redact sensitive data from text (for logging transcriptions)
redact_text() {
    local text="$1"
    local length="${#text}"
    if [ "$length" -le 10 ]; then
        echo "[REDACTED:${length}chars]"
    else
        echo "${text:0:5}...[REDACTED:${length}chars]"
    fi
}
