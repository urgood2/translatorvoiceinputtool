#!/usr/bin/env bash
#
# E2E Test Runner
#
# Runs the plan-defined E2E suite in a stable order with timestamped logs.
# Also preserves a legacy --parallel mode used by regression tests.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

PARALLEL=false
FILTER=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --parallel)
            PARALLEL=true
            shift
            ;;
        --filter)
            FILTER="${2:-}"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [--parallel] [--filter PATTERN]"
            echo "  --parallel  Run discovered shell test scripts in parallel (legacy mode)"
            echo "  --filter    Run only entries matching the pattern (id or label)"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 2
            ;;
    esac
done

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'
HEAVY_LINE="════════════════════════════════════════════════════"
LIGHT_LINE="────────────────────────────────────────────────────"

RUN_LOG_FILE=""

TESTS_PASSED=0
TESTS_FAILED=0
TESTS_SKIPPED=0

declare -a SUMMARY_LABELS=()
declare -a SUMMARY_STATUS=()
declare -a SUMMARY_DURATION_MS=()
declare -a SUMMARY_NOTE=()

timestamp_human() {
    if date +"%Y-%m-%d %H:%M:%S.%3N" >/dev/null 2>&1; then
        date +"%Y-%m-%d %H:%M:%S.%3N"
    else
        python3 -c 'from datetime import datetime; print(datetime.utcnow().strftime("%Y-%m-%d %H:%M:%S.%f")[:-3])'
    fi
}

now_ms() {
    if date +%s%3N >/dev/null 2>&1; then
        date +%s%3N
    else
        python3 -c 'import time; print(int(time.time() * 1000))'
    fi
}

format_duration_ms() {
    local duration_ms="$1"
    local seconds=$((duration_ms / 1000))
    local tenths=$(((duration_ms % 1000) / 100))
    printf "%d.%ds" "$seconds" "$tenths"
}

log_line() {
    local message="$1"
    local line="[$(timestamp_human)] ${message}"
    echo "$line"
    if [[ -n "$RUN_LOG_FILE" ]]; then
        echo "$line" >> "$RUN_LOG_FILE"
    fi
}

section_header() {
    local title="$1"
    log_line "$HEAVY_LINE"
    log_line "$title"
    log_line "$HEAVY_LINE"
}

append_summary() {
    SUMMARY_LABELS+=("$1")
    SUMMARY_STATUS+=("$2")
    SUMMARY_DURATION_MS+=("$3")
    SUMMARY_NOTE+=("${4:-}")
}

matches_filter() {
    local id="$1"
    local label="$2"
    if [[ -z "$FILTER" ]]; then
        return 0
    fi
    local f="${FILTER,,}"
    local id_l="${id,,}"
    local label_l="${label,,}"
    [[ "$id_l" == *"$f"* ]] || [[ "$label_l" == *"$f"* ]]
}

has_command() {
    command -v "$1" >/dev/null 2>&1
}

expected_sidecar_path() {
    case "$(uname -s)" in
        MINGW*|CYGWIN*|MSYS*) echo "$PROJECT_ROOT/sidecar/dist/openvoicy-sidecar.exe" ;;
        *) echo "$PROJECT_ROOT/sidecar/dist/openvoicy-sidecar" ;;
    esac
}

detected_memory_human() {
    case "$(uname -s)" in
        Linux)
            if [[ -r /proc/meminfo ]]; then
                awk '/MemTotal/ { printf "%.1f GiB", ($2 * 1024) / (1024 * 1024 * 1024); exit }' /proc/meminfo
                return 0
            fi
            ;;
        Darwin)
            if has_command sysctl; then
                local bytes
                bytes="$(sysctl -n hw.memsize 2>/dev/null || true)"
                if [[ -n "$bytes" ]]; then
                    python3 - "$bytes" <<'PY'
import sys
b = int(sys.argv[1])
print(f"{b / (1024**3):.1f} GiB")
PY
                    return 0
                fi
            fi
            ;;
    esac
    echo "unknown"
}

run_logged_command() {
    local name="$1"
    shift
    local tmp
    tmp="$(mktemp)"
    local exit_code=0

    set +e
    "$@" >"$tmp" 2>&1
    exit_code=$?
    set -e

    while IFS= read -r line || [[ -n "$line" ]]; do
        log_line "[RUN-ALL][${name}] ${line}"
    done < "$tmp"

    rm -f "$tmp"
    return "$exit_code"
}

run_environment_checks() {
    log_line "[RUN-ALL][env] Detecting platform and dependencies"

    local os
    local arch
    os="$(uname -s)"
    arch="$(uname -m)"
    local python_version
    python_version="$(python3 --version 2>&1 || true)"
    local rust_version="unavailable"
    if has_command rustc; then
        rust_version="$(rustc --version 2>/dev/null || true)"
    fi
    local memory
    memory="$(detected_memory_human)"

    log_line "[RUN-ALL][env] platform=${os} arch=${arch}"
    log_line "[RUN-ALL][env] python=${python_version}"
    log_line "[RUN-ALL][env] rust=${rust_version}"
    log_line "[RUN-ALL][env] memory=${memory}"

    local sidecar_bin
    sidecar_bin="$(expected_sidecar_path)"
    if [[ -f "$sidecar_bin" ]]; then
        log_line "[RUN-ALL][env] sidecar binary found: ${sidecar_bin}"
    else
        log_line "[RUN-ALL][env] missing sidecar binary: ${sidecar_bin}"
        return 1
    fi

    local dep
    for dep in bash jq python3; do
        if has_command "$dep"; then
            log_line "[RUN-ALL][env] dependency present: ${dep}"
        else
            log_line "[RUN-ALL][env] missing dependency: ${dep}"
            return 1
        fi
    done

    return 0
}

record_result() {
    local status="$1"
    case "$status" in
        PASS) ((TESTS_PASSED++)) || true ;;
        SKIP) ((TESTS_SKIPPED++)) || true ;;
        *) ((TESTS_FAILED++)) || true ;;
    esac
}

run_step() {
    local ordinal="$1"
    local total="$2"
    local id="$3"
    local label="$4"
    shift 4

    section_header "[RUN-ALL] Test ${ordinal}/${total}: ${label}"

    local started
    started="$(now_ms)"

    local exit_code=0
    set +e
    "$@"
    exit_code=$?
    set -e

    local ended
    ended="$(now_ms)"
    local duration_ms=$((ended - started))
    local duration_human
    duration_human="$(format_duration_ms "$duration_ms")"

    local status=""
    local note=""
    case "$exit_code" in
        0)
            status="PASS"
            ;;
        77)
            status="SKIP"
            note="skip contract (exit 77)"
            ;;
        *)
            status="FAIL"
            note="exit ${exit_code}"
            ;;
    esac

    record_result "$status"
    append_summary "${ordinal}. ${label}" "$status" "$duration_ms" "$note"

    log_line "[RUN-ALL] Result: ${status} (${duration_human})${note:+ - ${note}}"
    log_line ""
}

run_parallel_mode() {
    echo "========================================"
    echo "     OpenVoicy E2E Test Suite"
    echo "========================================"
    echo ""

    declare -a test_scripts=()
    local script
    for script in "$SCRIPT_DIR"/test-*.sh; do
        if [[ -f "$script" ]]; then
            local name
            name="$(basename "$script" .sh)"
            if [[ -z "$FILTER" ]] || [[ "$name" == *"$FILTER"* ]]; then
                test_scripts+=("$script")
            fi
        fi
    done

    if [[ "${#test_scripts[@]}" -eq 0 ]]; then
        echo "No tests found matching filter: $FILTER"
        return 0
    fi

    echo "Found ${#test_scripts[@]} test(s) to run"
    echo "Running tests in parallel..."
    echo ""

    local passed=0
    local failed=0
    local skipped=0

    declare -A results=()
    declare -A pids=()

    chmod +x "$SCRIPT_DIR"/test-*.sh "$SCRIPT_DIR"/lib/*.sh 2>/dev/null || true

    for script in "${test_scripts[@]}"; do
        local name
        name="$(basename "$script" .sh)"
        "$script" >"/tmp/e2e-${name}.out" 2>&1 &
        pids["$name"]=$!
    done

    local name
    for name in "${!pids[@]}"; do
        local pid
        pid="${pids[$name]}"
        if wait "$pid"; then
            results["$name"]="PASS"
            ((passed++)) || true
        else
            local exit_code=$?
            case "$exit_code" in
                77|2) results["$name"]="SKIP"; ((skipped++)) || true ;;
                3) results["$name"]="TIMEOUT"; ((failed++)) || true ;;
                *) results["$name"]="ERROR:$exit_code"; ((failed++)) || true ;;
            esac
        fi
    done

    echo "========================================"
    echo "              RESULTS"
    echo "========================================"
    echo ""
    echo "Detailed Results:"
    for name in "${!results[@]}"; do
        echo "  $name ${results[$name]}"
    done
    echo ""
    echo "Passed: $passed, Failed: $failed, Skipped: $skipped"

    if [[ "$failed" -gt 0 ]]; then
        return 1
    fi
    return 0
}

main() {
    mkdir -p "$PROJECT_ROOT/logs/e2e"
    RUN_LOG_FILE="$PROJECT_ROOT/logs/e2e/run-$(date -u +%Y-%m-%d-%H%M%S).log"
    : > "$RUN_LOG_FILE"

    if [[ "$PARALLEL" == true ]]; then
        run_parallel_mode
        exit $?
    fi

    log_line "$HEAVY_LINE"
    log_line "[RUN-ALL] OpenVoicy E2E Test Suite"
    log_line "[RUN-ALL] Log file: $RUN_LOG_FILE"
    log_line "$HEAVY_LINE"
    log_line ""

    declare -a ids=()
    declare -a labels=()
    declare -a handlers=()

    if matches_filter "environment" "Environment checks"; then
        ids+=("environment")
        labels+=("Environment checks")
        handlers+=("run_environment_checks")
    fi
    if matches_filter "startup-health" "Sidecar startup health"; then
        ids+=("startup-health")
        labels+=("Sidecar startup health")
        handlers+=("run_logged_command startup-health bash $SCRIPT_DIR/test-startup-health.sh")
    fi
    if matches_filter "ipc-compliance" "IPC compliance self-test"; then
        ids+=("ipc-compliance")
        labels+=("IPC compliance self-test")
        handlers+=("run_logged_command ipc-compliance python3 -m openvoicy_sidecar.self_test")
    fi
    if matches_filter "crash-loop-recovery" "Sidecar crash loop recovery"; then
        ids+=("crash-loop-recovery")
        labels+=("Sidecar crash loop recovery")
        handlers+=("run_logged_command crash-loop-recovery bash $SCRIPT_DIR/test-error-recovery.sh")
    fi
    if matches_filter "full-dictation-flow" "Full dictation flow"; then
        ids+=("full-dictation-flow")
        labels+=("Full dictation flow")
        handlers+=("run_logged_command full-dictation-flow bash $SCRIPT_DIR/test-full-flow.sh")
    fi
    if matches_filter "device-removal" "Device removal mid-recording"; then
        ids+=("device-removal")
        labels+=("Device removal mid-recording")
        handlers+=("run_logged_command device-removal bash $SCRIPT_DIR/test-device-removal.sh")
    fi
    if matches_filter "offline-install" "Offline install behavior"; then
        ids+=("offline-install")
        labels+=("Offline install behavior")
        handlers+=("run_logged_command offline-install bash $SCRIPT_DIR/test-offline-install.sh")
    fi

    if [[ "${#ids[@]}" -eq 0 ]]; then
        log_line "[RUN-ALL] No tests matched filter: ${FILTER}"
        exit 0
    fi

    local total="${#ids[@]}"
    local idx
    for idx in "${!ids[@]}"; do
        local ordinal=$((idx + 1))
        # shellcheck disable=SC2086
        run_step "$ordinal" "$total" "${ids[$idx]}" "${labels[$idx]}" ${handlers[$idx]}
    done

    local total_duration_ms=0
    local i
    for i in "${!SUMMARY_DURATION_MS[@]}"; do
        total_duration_ms=$((total_duration_ms + SUMMARY_DURATION_MS[$i]))
    done

    section_header "E2E TEST SUMMARY"
    for i in "${!SUMMARY_LABELS[@]}"; do
        local status="${SUMMARY_STATUS[$i]}"
        local color="$GREEN"
        if [[ "$status" == "SKIP" ]]; then
            color="$YELLOW"
        elif [[ "$status" != "PASS" ]]; then
            color="$RED"
        fi

        local duration_human
        duration_human="$(format_duration_ms "${SUMMARY_DURATION_MS[$i]}")"
        local note="${SUMMARY_NOTE[$i]}"
        local label="${SUMMARY_LABELS[$i]}"
        printf "%s  %-28s %b%-4s%b (%s)%s\n" \
            "[$(timestamp_human)]" \
            "$label" \
            "$color" \
            "$status" \
            "$NC" \
            "$duration_human" \
            "${note:+ - ${note}}" | tee -a "$RUN_LOG_FILE"
    done

    log_line "$LIGHT_LINE"
    local total_count=$((TESTS_PASSED + TESTS_SKIPPED + TESTS_FAILED))
    log_line "${TESTS_PASSED}/${total_count} PASSED, ${TESTS_SKIPPED} SKIPPED, ${TESTS_FAILED} FAILED"
    log_line "Total time: $(format_duration_ms "$total_duration_ms")"
    log_line "$HEAVY_LINE"

    if [[ "$TESTS_FAILED" -gt 0 ]]; then
        exit 1
    fi
    exit 0
}

main
