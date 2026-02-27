#!/usr/bin/env bash
#
# E2E Test: Offline install behavior
#
# Validates:
# 1. Sidecar starts with network mocked/disabled.
# 2. Existing cached model remains usable via asr.initialize.
# 3. model.download returns E_NETWORK with clear error text while offline.
# 4. Failed download leaves no committed/corrupt model artifacts.
# 5. After restoring network, retry succeeds.
# 6. Sidecar remains functional after failure/retry.
#
# Output:
#   logs/e2e/test-offline-install-TIMESTAMP.log
#
# Exit codes:
#   0  pass
#   1  fail
#   77 skip (cached model not available)
#

set -euo pipefail

if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

source "$SCRIPT_DIR/lib/log.sh"
source "$SCRIPT_DIR/lib/common.sh"

TEST_NAME="test-offline-install"
MOCK_PROXY_URL="http://127.0.0.1:9"
RETRY_GUIDANCE="Check your internet connection and retry"
STEPS_TOTAL=6
NETWORK_STATE="online"

LOG_FILE=""
FAIL_REASON=""

ORIG_OPENVOICY_SHARED_ROOT="${OPENVOICY_SHARED_ROOT-__UNSET__}"
ORIG_XDG_CACHE_HOME="${XDG_CACHE_HOME-__UNSET__}"
ORIG_HTTP_PROXY="${HTTP_PROXY-__UNSET__}"
ORIG_HTTPS_PROXY="${HTTPS_PROXY-__UNSET__}"
ORIG_ALL_PROXY="${ALL_PROXY-__UNSET__}"
ORIG_http_proxy="${http_proxy-__UNSET__}"
ORIG_https_proxy="${https_proxy-__UNSET__}"
ORIG_all_proxy="${all_proxy-__UNSET__}"
ORIG_NO_PROXY="${NO_PROXY-__UNSET__}"
ORIG_no_proxy="${no_proxy-__UNSET__}"

DEFAULT_MODEL_ID=""
DEFAULT_CACHE_ROOT=""
DEFAULT_MODEL_DIR=""

OFFLINE_TEST_ROOT=""
OFFLINE_SHARED_ROOT=""
OFFLINE_SERVER_DIR=""
OFFLINE_SERVER_LOG=""
OFFLINE_SERVER_PORT=""
OFFLINE_SERVER_PID=""
OFFLINE_XDG_CACHE_HOME=""
OFFLINE_CACHE_ROOT=""
OFFLINE_MODEL_ID="offline-e2e-model"
OFFLINE_MODEL_FILE="offline.bin"
OFFLINE_MODEL_DIR=""
OFFLINE_PARTIAL_DIR=""

declare -a ERROR_CHAIN=()
declare -a RPC_RESPONSES=()

timestamp_utc_ms() {
    if date --version >/dev/null 2>&1; then
        date -u +"%Y-%m-%dT%H:%M:%S.%3NZ"
    else
        python3 - <<'PY'
from datetime import datetime
print(datetime.utcnow().strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z")
PY
    fi
}

log_line() {
    local level="$1"
    local step="$2"
    local msg="$3"
    local ts
    ts=$(timestamp_utc_ms)
    local line="[$ts] [$level] [$step] $msg"
    echo "$line"
    if [[ -n "$LOG_FILE" ]]; then
        echo "$line" >> "$LOG_FILE"
    fi
}

step_log() {
    local step="$1"
    local msg="$2"
    log_line "INFO" "step_${step}" "[network=${NETWORK_STATE}] Step ${step}/${STEPS_TOTAL}: ${msg}"
}

set_or_unset_var() {
    local name="$1"
    local value="$2"
    if [[ "$value" == "__UNSET__" ]]; then
        unset "$name"
    else
        export "$name=$value"
    fi
}

restore_environment() {
    set_or_unset_var "OPENVOICY_SHARED_ROOT" "$ORIG_OPENVOICY_SHARED_ROOT"
    set_or_unset_var "XDG_CACHE_HOME" "$ORIG_XDG_CACHE_HOME"
    set_or_unset_var "HTTP_PROXY" "$ORIG_HTTP_PROXY"
    set_or_unset_var "HTTPS_PROXY" "$ORIG_HTTPS_PROXY"
    set_or_unset_var "ALL_PROXY" "$ORIG_ALL_PROXY"
    set_or_unset_var "http_proxy" "$ORIG_http_proxy"
    set_or_unset_var "https_proxy" "$ORIG_https_proxy"
    set_or_unset_var "all_proxy" "$ORIG_all_proxy"
    set_or_unset_var "NO_PROXY" "$ORIG_NO_PROXY"
    set_or_unset_var "no_proxy" "$ORIG_no_proxy"
}

append_error() {
    ERROR_CHAIN+=("$1")
}

fail() {
    local reason="$1"
    FAIL_REASON="$reason"
    append_error "$reason"
    return 1
}

log_network_state() {
    local step="$1"
    log_line "INFO" "$step" "network_state http_proxy=${http_proxy-<unset>} https_proxy=${https_proxy-<unset>} no_proxy=${no_proxy-<unset>}"
}

use_default_runtime_env() {
    set_or_unset_var "OPENVOICY_SHARED_ROOT" "$ORIG_OPENVOICY_SHARED_ROOT"
    set_or_unset_var "XDG_CACHE_HOME" "$ORIG_XDG_CACHE_HOME"
}

use_offline_fixture_env() {
    export OPENVOICY_SHARED_ROOT="$OFFLINE_SHARED_ROOT"
    export XDG_CACHE_HOME="$OFFLINE_XDG_CACHE_HOME"
}

apply_mock_network() {
    export HTTP_PROXY="$MOCK_PROXY_URL"
    export HTTPS_PROXY="$MOCK_PROXY_URL"
    export ALL_PROXY="$MOCK_PROXY_URL"
    export http_proxy="$MOCK_PROXY_URL"
    export https_proxy="$MOCK_PROXY_URL"
    export all_proxy="$MOCK_PROXY_URL"
    export NO_PROXY=""
    export no_proxy=""
}

restore_network() {
    unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy
    export NO_PROXY="127.0.0.1,localhost"
    export no_proxy="127.0.0.1,localhost"
}

set_offline_network() {
    NETWORK_STATE="offline-mocked"
    apply_mock_network
    log_network_state "network_mocked"
}

set_online_network() {
    NETWORK_STATE="online"
    restore_network
    log_network_state "network_restored"
}

snapshot_dir() {
    local step="$1"
    local path="$2"
    log_line "INFO" "$step" "snapshot path=$path"
    if [[ ! -e "$path" ]]; then
        log_line "INFO" "$step" "snapshot_result=missing"
        return 0
    fi
    while IFS= read -r entry; do
        log_line "INFO" "$step" "snapshot_entry=$entry"
    done < <(find "$path" -mindepth 0 -maxdepth 4 -print | sort)
}

expect_jq_true() {
    local step="$1"
    local json="$2"
    local jq_filter="$3"
    local msg="$4"
    if echo "$json" | jq -e "$jq_filter" >/dev/null 2>&1; then
        log_line "INFO" "$step" "assert_pass $msg"
        return 0
    fi
    log_line "ERROR" "$step" "assert_fail $msg"
    append_error "$step: assertion failed: $msg"
    append_error "$step: response=$json"
    return 1
}

run_rpc_once() {
    local step="$1"
    local method="$2"
    local params="$3"
    local timeout="${4:-20}"

    local request_id
    request_id=$((RANDOM * RANDOM))

    local request
    request=$(jq -nc \
        --arg method "$method" \
        --argjson params "$params" \
        --argjson id "$request_id" \
        '{jsonrpc:"2.0",id:$id,method:$method,params:$params}')

    log_line "INFO" "$step" "[RPC][REQ] $request"

    local raw_output
    raw_output=$(printf '%s\n' "$request" | e2e_timeout_run "$timeout" "$E2E_SIDECAR_BIN" 2>&1 || true)

    local response=""
    local line=""
    while IFS= read -r line; do
        [[ -z "$line" ]] && continue
        log_line "INFO" "$step" "[RPC][STREAM] $line"
        local line_id
        line_id=$(echo "$line" | jq -r '.id // empty' 2>/dev/null || true)
        if [[ "$line_id" == "$request_id" ]]; then
            response="$line"
        fi
    done <<< "$raw_output"

    if [[ -z "$response" ]]; then
        response='{"error":{"message":"timeout_or_invalid_response"}}'
        append_error "$step: no json-rpc response for id=$request_id"
        append_error "$step: raw_output=$raw_output"
        log_line "ERROR" "$step" "rpc_response_missing id=$request_id"
        echo "$response"
        return 1
    fi

    RPC_RESPONSES+=("$response")
    log_line "INFO" "$step" "[RPC][RES] $response"
    echo "$response"
}

assert_network_error_actionable() {
    local response="$1"
    local step="${2:-step3_model_download_offline}"

    expect_jq_true "$step" "$response" '.error.data.kind == "E_NETWORK"' \
        "model.download returns E_NETWORK when offline" || return 1
    expect_jq_true "$step" "$response" \
        '(.error.message | type == "string") and ((.error.message | length) > 0)' \
        "offline error message is populated" || return 1

    if ! echo "$response" | jq -e \
        '.error.message | ascii_downcase | (contains("retry") or contains("check") or contains("connection"))' \
        >/dev/null 2>&1; then
        append_error "$step: missing retry/check guidance"
        log_line "ERROR" "$step" "missing retry/check guidance"
        return 1
    fi

    log_line "INFO" "$step" "retry_guidance=$RETRY_GUIDANCE"
    return 0
}

assert_atomic_install_state() {
    local isolated_cache_dir="$1"
    local step="${2:-step4_atomicity}"
    local final_dir="$isolated_cache_dir/$OFFLINE_MODEL_ID"
    local partial_dir="$isolated_cache_dir/.partial/$OFFLINE_MODEL_ID"

    if [[ -d "$final_dir" && ! -f "$final_dir/manifest.json" ]]; then
        append_error "$step: corrupt final model directory detected without manifest: $final_dir"
        log_line "ERROR" "$step" "corrupt final model directory detected without manifest"
        return 1
    fi

    if [[ -d "$partial_dir" ]]; then
        local non_manifest_files
        non_manifest_files=$(find "$partial_dir" -type f ! -name "manifest.json" 2>/dev/null || true)
        if [[ -n "$non_manifest_files" ]]; then
            append_error "$step: partial staging directory still exists with payload files: $partial_dir"
            append_error "$step: files=$(echo "$non_manifest_files" | tr '\n' ';')"
            log_line "ERROR" "$step" "partial staging directory still exists"
            return 1
        fi
    fi

    log_line "INFO" "$step" "atomic cache state verified"
    return 0
}

prepare_fixture_assets() {
    OFFLINE_TEST_ROOT=$(mktemp -d -t "${TEST_NAME}-XXXXXX")
    OFFLINE_SHARED_ROOT="$OFFLINE_TEST_ROOT/shared"
    OFFLINE_SERVER_DIR="$OFFLINE_TEST_ROOT/http"
    OFFLINE_XDG_CACHE_HOME="$OFFLINE_TEST_ROOT/xdg-cache"
    OFFLINE_CACHE_ROOT="$OFFLINE_XDG_CACHE_HOME/openvoicy/models"
    OFFLINE_MODEL_DIR="$OFFLINE_CACHE_ROOT/$OFFLINE_MODEL_ID"
    OFFLINE_PARTIAL_DIR="$OFFLINE_CACHE_ROOT/.partial/$OFFLINE_MODEL_ID"
    OFFLINE_SERVER_LOG="$OFFLINE_TEST_ROOT/http-server.log"

    mkdir -p "$OFFLINE_SHARED_ROOT/model" "$OFFLINE_SERVER_DIR" "$OFFLINE_XDG_CACHE_HOME"

    local payload_path="$OFFLINE_SERVER_DIR/$OFFLINE_MODEL_FILE"
    printf 'offline-install-fixture-%s\n' "$(date -u +%s)" > "$payload_path"

    local size_and_sha
    size_and_sha=$(python3 - "$payload_path" <<'PY'
import hashlib
import os
import sys

path = sys.argv[1]
data = open(path, "rb").read()
print(f"{len(data)} {hashlib.sha256(data).hexdigest()}")
PY
)
    local payload_size payload_sha
    payload_size="${size_and_sha%% *}"
    payload_sha="${size_and_sha##* }"

    OFFLINE_SERVER_PORT=$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
)

    local fixture_url="http://127.0.0.1:${OFFLINE_SERVER_PORT}/${OFFLINE_MODEL_FILE}"
    jq -nc \
        --arg model_id "$OFFLINE_MODEL_ID" \
        --arg file_path "$OFFLINE_MODEL_FILE" \
        --arg file_sha "$payload_sha" \
        --arg manifest_url "$fixture_url" \
        --argjson file_size "$payload_size" \
        '{
            schema_version: "1",
            model_id: $model_id,
            display_name: "Offline Fixture Model",
            source: "e2e/offline-fixture",
            source_url: $manifest_url,
            revision: "offline-fixture-v1",
            files: [
                {
                    path: $file_path,
                    size_bytes: $file_size,
                    sha256: $file_sha,
                    description: "small local fixture"
                }
            ],
            total_size_bytes: $file_size,
            mirrors: [
                {
                    provider: "local-fixture",
                    url: $manifest_url,
                    auth_required: false
                }
            ]
        }' > "$OFFLINE_SHARED_ROOT/model/MODEL_MANIFEST.json"
}

start_fixture_server() {
    python3 -m http.server "$OFFLINE_SERVER_PORT" \
        --bind 127.0.0.1 \
        --directory "$OFFLINE_SERVER_DIR" \
        >"$OFFLINE_SERVER_LOG" 2>&1 &
    OFFLINE_SERVER_PID=$!
    sleep 0.3
    if ! kill -0 "$OFFLINE_SERVER_PID" 2>/dev/null; then
        append_error "failed to start fixture HTTP server"
        append_error "server_log=$(cat "$OFFLINE_SERVER_LOG" 2>/dev/null || true)"
        return 1
    fi
}

stop_fixture_server() {
    if [[ -n "$OFFLINE_SERVER_PID" ]] && kill -0 "$OFFLINE_SERVER_PID" 2>/dev/null; then
        kill "$OFFLINE_SERVER_PID" 2>/dev/null || true
        wait "$OFFLINE_SERVER_PID" 2>/dev/null || true
    fi
    OFFLINE_SERVER_PID=""
}

resolve_default_cache() {
    local default_manifest="$E2E_PROJECT_ROOT/shared/model/MODEL_MANIFEST.json"
    DEFAULT_MODEL_ID=$(jq -r '.model_id' "$default_manifest")
    if [[ -n "${XDG_CACHE_HOME:-}" ]]; then
        DEFAULT_CACHE_ROOT="$XDG_CACHE_HOME/openvoicy/models"
    else
        DEFAULT_CACHE_ROOT="$HOME/.cache/openvoicy/models"
    fi
    DEFAULT_MODEL_DIR="$DEFAULT_CACHE_ROOT/$DEFAULT_MODEL_ID"
}

cached_model_is_available() {
    local default_manifest="$E2E_PROJECT_ROOT/shared/model/MODEL_MANIFEST.json"
    if [[ ! -d "$DEFAULT_MODEL_DIR" ]]; then
        log_line "WARN" "preflight" "model_cache_missing path=$DEFAULT_MODEL_DIR"
        return 1
    fi

    local missing=0
    local rel=""
    while IFS= read -r rel; do
        [[ -z "$rel" ]] && continue
        if [[ ! -f "$DEFAULT_MODEL_DIR/$rel" ]]; then
            log_line "WARN" "preflight" "model_cache_missing_file path=$DEFAULT_MODEL_DIR/$rel"
            missing=1
        fi
    done < <(jq -r '.files[].path' "$default_manifest")

    [[ "$missing" -eq 0 ]]
}

dump_failure_context() {
    log_line "ERROR" "failure" "FAIL reason=${FAIL_REASON:-unknown}"
    log_line "ERROR" "failure" "error_chain_count=${#ERROR_CHAIN[@]}"
    local entry=""
    for entry in "${ERROR_CHAIN[@]}"; do
        log_line "ERROR" "failure" "error_chain_entry=$entry"
    done

    log_line "ERROR" "failure" "rpc_response_count=${#RPC_RESPONSES[@]}"
    for entry in "${RPC_RESPONSES[@]}"; do
        log_line "ERROR" "failure" "rpc_response_verbatim=$entry"
    done

    if [[ -n "$OFFLINE_SERVER_LOG" && -f "$OFFLINE_SERVER_LOG" ]]; then
        while IFS= read -r entry; do
            log_line "ERROR" "failure" "fixture_http_log=$entry"
        done < "$OFFLINE_SERVER_LOG"
    fi
}

cleanup() {
    local exit_code=$?
    stop_fixture_server
    restore_environment

    if [[ -n "$OFFLINE_TEST_ROOT" && -d "$OFFLINE_TEST_ROOT" ]]; then
        rm -rf "$OFFLINE_TEST_ROOT"
    fi

    if [[ "$exit_code" -ne 0 && "$exit_code" -ne 77 ]]; then
        dump_failure_context
    fi

    if [[ "$exit_code" -eq 0 ]]; then
        log_line "INFO" "result" "PASS"
    elif [[ "$exit_code" -eq 77 ]]; then
        log_line "WARN" "result" "SKIP"
    fi
}

main() {
    require_jq
    init_common

    mkdir -p "$E2E_PROJECT_ROOT/logs/e2e"
    local ts
    ts=$(date -u +"%Y%m%dT%H%M%S")
    LOG_FILE="$E2E_PROJECT_ROOT/logs/e2e/${TEST_NAME}-${ts}.log"
    : > "$LOG_FILE"

    log_line "INFO" "start" "log_file=$LOG_FILE"
    resolve_default_cache
    log_line "INFO" "start" "default_model_id=$DEFAULT_MODEL_ID default_model_dir=$DEFAULT_MODEL_DIR"

    if ! cached_model_is_available; then
        log_line "WARN" "preflight" "cached model unavailable; skipping test"
        return 77
    fi

    snapshot_dir "cache_before_default" "$DEFAULT_MODEL_DIR"
    prepare_fixture_assets
    start_fixture_server
    snapshot_dir "cache_before_offline" "$OFFLINE_CACHE_ROOT"

    step_log 1 "Start sidecar with network disabled/mocked and verify system.ping"
    set_offline_network

    # Step 1: Sidecar responsive with network mocked.
    use_default_runtime_env
    local ping_response
    ping_response=$(run_rpc_once "step1_system_ping" "system.ping" "{}" 10) || return 1
    expect_jq_true "step1_system_ping" "$ping_response" '.result.protocol == "v1"' \
        "system.ping succeeds with mocked network" || return 1

    step_log 2 "Verify existing installed model is usable via asr.initialize"
    # Step 2: Existing cached model still usable.
    local init_response
    init_response=$(run_rpc_once "step2_asr_initialize" "asr.initialize" "{}" 60) || true
    expect_jq_true "step2_asr_initialize" "$init_response" '.result.status == "ready"' \
        "asr.initialize succeeds from cache while offline" || return 1

    step_log 3 "Attempt model.download offline and verify actionable E_NETWORK response"
    # Step 3: Download fails offline with E_NETWORK.
    use_offline_fixture_env
    local download_offline_response
    download_offline_response=$(run_rpc_once "step3_model_download_offline" "model.download" "{}" 20) || true
    assert_network_error_actionable "$download_offline_response" "step3_model_download_offline" || return 1

    step_log 4 "Verify no partial/corrupt model files remain after failed install"
    # Step 4: Atomic install property (no committed/corrupt artifacts).
    snapshot_dir "cache_after_offline_failure" "$OFFLINE_CACHE_ROOT"
    assert_atomic_install_state "$OFFLINE_CACHE_ROOT" "step4_atomicity" || return 1

    step_log 5 "Re-enable network and verify retry succeeds"
    # Step 5: Re-enable network and verify retry succeeds.
    set_online_network
    local download_retry_response
    download_retry_response=$(run_rpc_once "step5_model_download_retry" "model.download" "{}" 20) || return 1
    expect_jq_true "step5_model_download_retry" "$download_retry_response" '.result.status == "ready"' \
        "retry succeeds with network restored" || return 1

    step_log 6 "Verify sidecar remains functional after failure and retry"
    # Step 6: Sidecar still functional after failure/retry.
    local ping_after_failure
    ping_after_failure=$(run_rpc_once "step6a_ping_after_failure" "system.ping" "{}" 10) || return 1
    expect_jq_true "step6a_ping_after_failure" "$ping_after_failure" '.result.protocol == "v1"' \
        "sidecar remains functional after failed download" || return 1

    local status_after_retry
    status_after_retry=$(run_rpc_once "step6_status_after_retry" "status.get" "{}" 10) || return 1
    expect_jq_true "step6_status_after_retry" "$status_after_retry" '.result | type == "object"' \
        "status.get returns structured payload after retry" || return 1

    local ping_after_retry
    ping_after_retry=$(run_rpc_once "step6b_ping_after_retry" "system.ping" "{}" 10) || return 1
    expect_jq_true "step6b_ping_after_retry" "$ping_after_retry" '.result.protocol == "v1"' \
        "sidecar remains functional after retry" || return 1

    snapshot_dir "cache_after_retry" "$OFFLINE_MODEL_DIR"
    use_default_runtime_env
    snapshot_dir "cache_after_default" "$DEFAULT_MODEL_DIR"

    return 0
}

trap cleanup EXIT
main
exit $?
