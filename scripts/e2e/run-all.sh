#!/usr/bin/env bash
#
# E2E Test Runner
#
# Runs all E2E tests and aggregates results.
#
# Usage:
#   ./scripts/e2e/run-all.sh [--parallel] [--filter PATTERN]
#
# Exit codes:
#   0 - All tests passed
#   1 - One or more tests failed
#   2 - Environment setup error
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Configuration
PARALLEL=false
FILTER=""
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_SKIPPED=0

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --parallel)
            PARALLEL=true
            shift
            ;;
        --filter)
            FILTER="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [--parallel] [--filter PATTERN]"
            echo "  --parallel  Run tests in parallel"
            echo "  --filter    Only run tests matching pattern"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 2
            ;;
    esac
done

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

echo "========================================"
echo "     OpenVoicy E2E Test Suite"
echo "========================================"
echo ""

# Check prerequisites
if ! command -v jq &>/dev/null; then
    echo -e "${RED}ERROR: jq is required but not installed${NC}"
    exit 2
fi

# Check sidecar binary
if [ ! -f "$PROJECT_ROOT/sidecar/dist/openvoicy-sidecar" ] && \
   [ ! -f "$PROJECT_ROOT/sidecar/dist/openvoicy-sidecar.exe" ]; then
    echo -e "${YELLOW}WARNING: Sidecar binary not found${NC}"
    echo "Run ./scripts/build-sidecar.sh first"
    echo ""
fi

# Find all test scripts
declare -a TEST_SCRIPTS
for script in "$SCRIPT_DIR"/test-*.sh; do
    if [ -f "$script" ]; then
        name=$(basename "$script" .sh)
        if [ -z "$FILTER" ] || [[ "$name" == *"$FILTER"* ]]; then
            TEST_SCRIPTS+=("$script")
        fi
    fi
done

if [ ${#TEST_SCRIPTS[@]} -eq 0 ]; then
    echo "No tests found matching filter: $FILTER"
    exit 0
fi

echo "Found ${#TEST_SCRIPTS[@]} test(s) to run"
echo ""

# Results tracking
declare -A RESULTS
declare -A DURATIONS

# Run a single test
run_test() {
    local script="$1"
    local name
    name=$(basename "$script" .sh)

    echo -n "Running $name... "

    local start_time
    start_time=$(date +%s)

    local exit_code=0
    local output
    output=$("$script" 2>&1) || exit_code=$?

    local end_time
    end_time=$(date +%s)
    local duration=$((end_time - start_time))

    DURATIONS[$name]=$duration

    case $exit_code in
        0)
            RESULTS[$name]="PASS"
            echo -e "${GREEN}PASS${NC} (${duration}s)"
            ((TESTS_PASSED++)) || true
            ;;
        1)
            RESULTS[$name]="FAIL"
            echo -e "${RED}FAIL${NC} (${duration}s)"
            ((TESTS_FAILED++)) || true
            # Show output on failure
            echo "--- Output ---"
            echo "$output" | tail -20
            echo "--------------"
            ;;
        2)
            RESULTS[$name]="SKIP"
            echo -e "${YELLOW}SKIP${NC} (setup error)"
            ((TESTS_SKIPPED++)) || true
            ;;
        3)
            RESULTS[$name]="TIMEOUT"
            echo -e "${RED}TIMEOUT${NC}"
            ((TESTS_FAILED++)) || true
            ;;
        *)
            RESULTS[$name]="ERROR:$exit_code"
            echo -e "${RED}ERROR${NC} (exit code: $exit_code)"
            ((TESTS_FAILED++)) || true
            ;;
    esac
}

# Make test scripts executable
chmod +x "$SCRIPT_DIR"/test-*.sh "$SCRIPT_DIR"/lib/*.sh 2>/dev/null || true

# Run tests
if [ "$PARALLEL" = true ]; then
    echo "Running tests in parallel..."
    echo ""

    # Run all tests in background
    declare -A PIDS
    for script in "${TEST_SCRIPTS[@]}"; do
        name=$(basename "$script" .sh)
        "$script" > "/tmp/e2e-$name.out" 2>&1 &
        PIDS[$name]=$!
    done

    # Wait for all and collect results
    for name in "${!PIDS[@]}"; do
        local pid=${PIDS[$name]}
        local start_time
        start_time=$(date +%s)

        if wait "$pid"; then
            RESULTS[$name]="PASS"
            ((TESTS_PASSED++)) || true
        else
            local exit_code=$?
            case $exit_code in
                1) RESULTS[$name]="FAIL"; ((TESTS_FAILED++)) || true ;;
                2) RESULTS[$name]="SKIP"; ((TESTS_SKIPPED++)) || true ;;
                3) RESULTS[$name]="TIMEOUT"; ((TESTS_FAILED++)) || true ;;
                *) RESULTS[$name]="ERROR:$exit_code"; ((TESTS_FAILED++)) || true ;;
            esac
        fi

        local end_time
        end_time=$(date +%s)
        DURATIONS[$name]=$((end_time - start_time))
    done
else
    # Run tests sequentially
    for script in "${TEST_SCRIPTS[@]}"; do
        run_test "$script"
    done
fi

# Summary
echo ""
echo "========================================"
echo "              RESULTS"
echo "========================================"
echo ""

total=$((TESTS_PASSED + TESTS_FAILED + TESTS_SKIPPED))
echo "Total:   $total"
echo -e "Passed:  ${GREEN}$TESTS_PASSED${NC}"
echo -e "Failed:  ${RED}$TESTS_FAILED${NC}"
echo -e "Skipped: ${YELLOW}$TESTS_SKIPPED${NC}"
echo ""

# Detailed results
echo "Detailed Results:"
for name in "${!RESULTS[@]}"; do
    result="${RESULTS[$name]}"
    duration="${DURATIONS[$name]:-?}s"

    case $result in
        PASS)    echo -e "  ${GREEN}✓${NC} $name ($duration)" ;;
        FAIL)    echo -e "  ${RED}✗${NC} $name ($duration)" ;;
        SKIP)    echo -e "  ${YELLOW}○${NC} $name (skipped)" ;;
        TIMEOUT) echo -e "  ${RED}⏱${NC} $name (timeout)" ;;
        *)       echo -e "  ${RED}?${NC} $name ($result)" ;;
    esac
done

echo ""

# Log file locations
echo "Log files:"
for log in "$PROJECT_ROOT"/logs/e2e/*.jsonl; do
    [ -f "$log" ] && echo "  $log"
done | tail -5

echo ""

# Exit with failure if any tests failed
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo -e "${RED}Some tests failed!${NC}"
    exit 1
fi

echo -e "${GREEN}All tests passed!${NC}"
exit 0
