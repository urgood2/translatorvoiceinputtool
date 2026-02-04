#!/usr/bin/env bash
#
# E2E Test Logging Library
# Provides structured JSON logging for E2E test scripts.
#
# Usage:
#   source scripts/e2e/lib/log.sh
#   log_info "transcription" "recording_start" "Recording started" '{"session_id":"abc"}'
#

set -euo pipefail

# Log file paths (set by init_logging)
E2E_LOG_JSON=""
E2E_LOG_HUMAN=""
E2E_ARTIFACTS_DIR=""
E2E_TEST_NAME=""
E2E_START_TIME=""

# Initialize logging for a test run
# Args: test_name
init_logging() {
    local test_name="${1:-e2e-test}"
    local timestamp
    timestamp=$(date -u +"%Y%m%d_%H%M%S")

    E2E_TEST_NAME="$test_name"
    E2E_START_TIME=$(date +%s%3N)

    # Ensure directories exist - find project root from current location or script
    local script_dir project_root
    if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
        script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
        project_root="$(cd "$script_dir/../../.." && pwd)"
    elif [[ -n "${0:-}" ]]; then
        script_dir="$(cd "$(dirname "$0")" && pwd)"
        project_root="$(cd "$script_dir/../.." && pwd)"
    else
        # Fallback: look for sidecar directory from current dir
        project_root="$(pwd)"
        while [[ "$project_root" != "/" ]] && [[ ! -d "$project_root/sidecar" ]]; do
            project_root="$(dirname "$project_root")"
        done
    fi

    mkdir -p "$project_root/logs/e2e"
    mkdir -p "$project_root/artifacts/e2e"

    E2E_LOG_JSON="$project_root/logs/e2e/${test_name}_${timestamp}.jsonl"
    E2E_LOG_HUMAN="$project_root/logs/e2e/${test_name}_${timestamp}.log"
    E2E_ARTIFACTS_DIR="$project_root/artifacts/e2e/${test_name}_${timestamp}"

    mkdir -p "$E2E_ARTIFACTS_DIR"

    # Write header
    echo "# E2E Test Log: $test_name" > "$E2E_LOG_HUMAN"
    echo "# Started: $(date -u +"%Y-%m-%dT%H:%M:%SZ")" >> "$E2E_LOG_HUMAN"
    echo "# JSON log: $E2E_LOG_JSON" >> "$E2E_LOG_HUMAN"
    echo "" >> "$E2E_LOG_HUMAN"

    log_info "test" "init" "Test initialized" "{\"test_name\":\"$test_name\",\"timestamp\":\"$timestamp\"}"
}

# Core logging function - outputs JSON Lines format
# Args: level, phase, step, msg, [data_json]
log_json() {
    local level="$1"
    local phase="$2"
    local step="$3"
    local msg="$4"
    local data="${5:-null}"

    # Sanitize data - ensure it's valid JSON
    if [[ -z "$data" ]] || [[ "$data" == "" ]]; then
        data="null"
    fi
    # Validate data is valid JSON before passing to jq
    if ! echo "$data" | jq -e . >/dev/null 2>&1; then
        # If not valid JSON, wrap it as a string
        data="\"$data\""
    fi

    # Get timestamp with milliseconds
    local ts
    if date --version >/dev/null 2>&1; then
        # GNU date (Linux)
        ts=$(date -u +"%Y-%m-%dT%H:%M:%S.%3NZ")
    else
        # BSD date (macOS) - no %3N support, use Python fallback
        ts=$(python3 -c "from datetime import datetime; print(datetime.utcnow().strftime('%Y-%m-%dT%H:%M:%S.%f')[:-3] + 'Z')")
    fi

    # Build JSON using jq if available, otherwise fallback
    local json_line
    if command -v jq &>/dev/null; then
        json_line=$(jq -nc \
            --arg ts "$ts" \
            --arg level "$level" \
            --arg phase "$phase" \
            --arg step "$step" \
            --arg msg "$msg" \
            --argjson data "$data" \
            '{ts:$ts, level:$level, phase:$phase, step:$step, msg:$msg, data:$data}' 2>/dev/null) || \
        json_line="{\"ts\":\"$ts\",\"level\":\"$level\",\"phase\":\"$phase\",\"step\":\"$step\",\"msg\":\"$msg\",\"data\":null}"
    else
        # Fallback without jq (basic escaping)
        local escaped_msg
        escaped_msg="${msg//\\/\\\\}"
        escaped_msg="${escaped_msg//\"/\\\"}"
        json_line="{\"ts\":\"$ts\",\"level\":\"$level\",\"phase\":\"$phase\",\"step\":\"$step\",\"msg\":\"$escaped_msg\",\"data\":$data}"
    fi

    # Write to JSON log file if initialized
    if [ -n "$E2E_LOG_JSON" ]; then
        echo "$json_line" >> "$E2E_LOG_JSON"
    fi

    # Write human-readable version
    local color=""
    local reset="\033[0m"
    case "$level" in
        DEBUG) color="\033[0;37m" ;;  # Gray
        INFO)  color="\033[0;32m" ;;  # Green
        WARN)  color="\033[0;33m" ;;  # Yellow
        ERROR) color="\033[0;31m" ;;  # Red
    esac

    local human_line="[$ts] $level [$phase/$step] $msg"
    if [ "$data" != "null" ]; then
        human_line="$human_line ($data)"
    fi

    # Write to human log file if initialized
    if [ -n "$E2E_LOG_HUMAN" ]; then
        echo "$human_line" >> "$E2E_LOG_HUMAN"
    fi

    # Also output to terminal (colored) - use stderr to avoid polluting stdout
    if [ -t 2 ]; then
        echo -e "${color}${human_line}${reset}" >&2
    else
        echo "$human_line" >&2
    fi
}

# Convenience functions for different log levels
log_debug() { log_json "DEBUG" "$@"; }
log_info()  { log_json "INFO" "$@"; }
log_warn()  { log_json "WARN" "$@"; }
log_error() { log_json "ERROR" "$@"; }

# Start timing a phase
# Args: phase_name
# Returns: start timestamp in ms
start_timer() {
    date +%s%3N
}

# Log with duration
# Args: level, phase, step, msg, data_json, start_time_ms
log_with_duration() {
    local level="$1"
    local phase="$2"
    local step="$3"
    local msg="$4"
    local data="${5:-{}}"
    local start_ms="$6"

    local end_ms
    end_ms=$(date +%s%3N)
    local duration_ms=$((end_ms - start_ms))

    # Add duration to data - handle empty/null/invalid data
    if [ -z "$data" ] || [ "$data" = "null" ] || [ "$data" = "{}" ]; then
        data="{\"duration_ms\":$duration_ms}"
    else
        # Inject duration_ms into existing object, with fallback
        data=$(echo "$data" | jq -c ". + {duration_ms: $duration_ms}" 2>/dev/null) || data="{\"duration_ms\":$duration_ms}"
    fi

    log_json "$level" "$phase" "$step" "$msg" "$data"
}

# Finalize logging and print summary
finalize_logging() {
    local exit_code="${1:-0}"
    local end_time
    end_time=$(date +%s%3N)
    local total_duration=$((end_time - E2E_START_TIME))

    local status="PASSED"
    [ "$exit_code" -ne 0 ] && status="FAILED"

    log_info "test" "finalize" "Test $status" "{\"exit_code\":$exit_code,\"total_duration_ms\":$total_duration}"

    # Write summary to human log
    echo "" >> "$E2E_LOG_HUMAN"
    echo "# ============================================" >> "$E2E_LOG_HUMAN"
    echo "# Test: $E2E_TEST_NAME" >> "$E2E_LOG_HUMAN"
    echo "# Status: $status" >> "$E2E_LOG_HUMAN"
    echo "# Duration: $((total_duration / 1000)).$((total_duration % 1000))s" >> "$E2E_LOG_HUMAN"
    echo "# Exit code: $exit_code" >> "$E2E_LOG_HUMAN"
    echo "# ============================================" >> "$E2E_LOG_HUMAN"

    echo ""
    echo "=== Test Summary ==="
    echo "Test: $E2E_TEST_NAME"
    echo "Status: $status"
    echo "Duration: $((total_duration / 1000)).$((total_duration % 1000))s"
    echo "JSON log: $E2E_LOG_JSON"
    echo "Human log: $E2E_LOG_HUMAN"
    echo "Artifacts: $E2E_ARTIFACTS_DIR"
}

# Save artifact (screenshot, audio, etc.)
# Args: source_path, artifact_name
save_artifact() {
    local source="$1"
    local name="$2"

    if [ -f "$source" ]; then
        cp "$source" "$E2E_ARTIFACTS_DIR/$name"
        log_info "test" "artifact" "Saved artifact: $name" "{\"path\":\"$E2E_ARTIFACTS_DIR/$name\"}"
    else
        log_warn "test" "artifact" "Artifact source not found: $source"
    fi
}
