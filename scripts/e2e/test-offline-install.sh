#!/usr/bin/env bash
#
# E2E Test: Offline Install Behavior
#
# Validates offline model behavior with detailed diagnostics:
# 1) Start sidecar with mocked-offline network
# 2) Verify existing cached model remains usable (asr.initialize)
# 3) Attempt model download with unreachable mirror => E_NETWORK
# 4) Verify no partial/corrupt staged files remain
# 5) Re-enable network and verify retry error is actionable
# 6) Verify sidecar remains functional throughout
#
# Exit codes:
#   0  pass
#   1  fail
#   77 skip (no usable cached model available)

set -euo pipefail

if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

source "$SCRIPT_DIR/lib/log.sh"
source "$SCRIPT_DIR/lib/common.sh"

STEPS_TOTAL=6
RPC_DEFAULT_TIMEOUT=20

TEST_LOG_FILE=""
LAST_ERROR=""
NETWORK_STATE="online"
CACHED_MODEL_ID=""
HOST_CACHE_DIR=""
OFFLINE_SHARED_ROOT=""
OFFLINE_XDG_CACHE_BASE=""
OFFLINE_MODEL_ID="offline-e2e-model"

ORIG_http_proxy="${http_proxy-__UNSET__}"
ORIG_https_proxy="${https_proxy-__UNSET__}"
ORIG_HTTP_PROXY="${HTTP_PROXY-__UNSET__}"
ORIG_HTTPS_PROXY="${HTTPS_PROXY-__UNSET__}"
ORIG_ALL_PROXY="${ALL_PROXY-__UNSET__}"
ORIG_NO_PROXY="${NO_PROXY-__UNSET__}"
ORIG_no_proxy="${no_proxy-__UNSET__}"
ORIG_OPENVOICY_SHARED_ROOT="${OPENVOICY_SHARED_ROOT-__UNSET__}"
ORIG_XDG_CACHE_HOME="${XDG_CACHE_HOME-__UNSET__}"

declare -a RPC_HISTORY=()
declare -a CACHE_HISTORY=()

ts_human() {
    if date +"%Y-%m-%d %H:%M:%S.%3N" >/dev/null 2>&1; then
        date +"%Y-%m-%d %H:%M:%S.%3N"
    else
        python3 -c 'from datetime import datetime; print(datetime.utcnow().strftime("%Y-%m-%d %H:%M:%S.%f")[:-3])'
    fi
}

emit_line() {
    local line="$1"
    echo "$line"
    if [[ -n "$TEST_LOG_FILE" ]]; then
        echo "$line" >> "$TEST_LOG_FILE"
    fi
}

step_log() {
    local step="$1"
    local message="$2"
    emit_line "[$(ts_human)] [STEP ${step}/${STEPS_TOTAL}] [network=${NETWORK_STATE}] ${message}"
}

record_rpc() {
    RPC_HISTORY+=("$1")
}

record_cache() {
    CACHE_HISTORY+=("$1")
}

restore_var() {
    local name="$1"
    local value="$2"
    if [[ "$value" == "__UNSET__" ]]; then
        unset "$name"
    else
        export "$name=$value"
    fi
}

restore_env() {
    restore_var "http_proxy" "$ORIG_http_proxy"
    restore_var "https_proxy" "$ORIG_https_proxy"
    restore_var "HTTP_PROXY" "$ORIG_HTTP_PROXY"
    restore_var "HTTPS_PROXY" "$ORIG_HTTPS_PROXY"
    restore_var "ALL_PROXY" "$ORIG_ALL_PROXY"
    restore_var "NO_PROXY" "$ORIG_NO_PROXY"
    restore_var "no_proxy" "$ORIG_no_proxy"
    restore_var "OPENVOICY_SHARED_ROOT" "$ORIG_OPENVOICY_SHARED_ROOT"
    restore_var "XDG_CACHE_HOME" "$ORIG_XDG_CACHE_HOME"
}

set_offline_network() {
    # Force outbound HTTP(S) calls through an unreachable local proxy.
    export http_proxy="http://127.0.0.1:9"
    export https_proxy="http://127.0.0.1:9"
    export HTTP_PROXY="http://127.0.0.1:9"
    export HTTPS_PROXY="http://127.0.0.1:9"
    export ALL_PROXY="http://127.0.0.1:9"
    export NO_PROXY="127.0.0.1,localhost"
    export no_proxy="127.0.0.1,localhost"
    NETWORK_STATE="offline-mocked"
}

set_online_network() {
    unset http_proxy https_proxy HTTP_PROXY HTTPS_PROXY ALL_PROXY
    export NO_PROXY="127.0.0.1,localhost"
    export no_proxy="127.0.0.1,localhost"
    NETWORK_STATE="online"
}

host_cache_dir() {
    case "$(uname -s)" in
        Darwin)
            echo "$HOME/Library/Caches/openvoicy/models"
            ;;
        MINGW*|CYGWIN*|MSYS*)
            if [[ -n "${LOCALAPPDATA:-}" ]]; then
                echo "$LOCALAPPDATA/openvoicy/models"
            else
                echo "$HOME/.cache/openvoicy/models"
            fi
            ;;
        *)
            if [[ -n "${XDG_CACHE_HOME:-}" ]]; then
                echo "$XDG_CACHE_HOME/openvoicy/models"
            else
                echo "$HOME/.cache/openvoicy/models"
            fi
            ;;
    esac
}

discover_cached_model_id() {
    local cache_dir="$1"
    [[ -d "$cache_dir" ]] || return 1

    local manifest_path=""
    while IFS= read -r manifest_path; do
        local model_dir
        model_dir="$(dirname "$manifest_path")"
        if find "$model_dir" -mindepth 1 -maxdepth 1 -type f ! -name "manifest.json" | grep -q .; then
            basename "$model_dir"
            return 0
        fi
    done < <(find "$cache_dir" -mindepth 2 -maxdepth 2 -type f -name "manifest.json" 2>/dev/null | sort)

    return 1
}

emit_cache_snapshot() {
    local label="$1"
    local cache_dir="$2"

    emit_line "[$(ts_human)] [CACHE] ${label}: ${cache_dir}"
    if [[ ! -d "$cache_dir" ]]; then
        emit_line "[$(ts_human)] [CACHE] ${label}: directory missing"
        record_cache "${label}:missing:${cache_dir}"
        return
    fi

    local listed=0
    while IFS= read -r entry; do
        emit_line "[$(ts_human)] [CACHE] ${label}: ${entry}"
        record_cache "${label}:${entry}"
        listed=$((listed + 1))
        if (( listed >= 40 )); then
            emit_line "[$(ts_human)] [CACHE] ${label}: ... truncated ..."
            break
        fi
    done < <(find "$cache_dir" -mindepth 1 -maxdepth 3 2>/dev/null | sort)

    if (( listed == 0 )); then
        emit_line "[$(ts_human)] [CACHE] ${label}: empty"
        record_cache "${label}:empty"
    fi
}

start_sidecar_session() {
    start_sidecar || return 1
    exec 4<"$E2E_SIDECAR_STDOUT"
    emit_line "[$(ts_human)] [SESSION] sidecar started pid=${E2E_SIDECAR_PID}"
}

stop_sidecar_session() {
    local shutdown_response
    shutdown_response=$(sidecar_rpc_session "system.shutdown" "{}" 8 || true)
    emit_line "[$(ts_human)] [RPC][RES][system.shutdown] ${shutdown_response}"
    { exec 4<&-; } 2>/dev/null || true
    stop_sidecar || true
}

sidecar_rpc_session() {
    local method="$1"
    local params="$2"
    [[ -z "$params" ]] && params='{}'
    local timeout="${3:-$RPC_DEFAULT_TIMEOUT}"

    local request_id
    request_id=$((RANDOM * RANDOM))

    local request
    request=$(jq -nc \
        --arg method "$method" \
        --argjson params "$params" \
        --argjson id "$request_id" \
        '{jsonrpc:"2.0",id:$id,method:$method,params:$params}')

    emit_line "[$(ts_human)] [RPC][REQ] [network=${NETWORK_STATE}] method=${method} id=${request_id} payload=${request}"
    record_rpc "REQUEST method=${method} id=${request_id} payload=${request}"

    printf '%s\n' "$request" >&3

    local deadline=$((SECONDS + timeout))
    local line=""

    while (( SECONDS < deadline )); do
        local wait_s=$((deadline - SECONDS))
        (( wait_s <= 0 )) && break

        if IFS= read -r -u 4 -t "$wait_s" line; then
            if [[ "$line" != *'"jsonrpc"'* ]]; then
                emit_line "[$(ts_human)] [RPC][RAW] ${line}"
                record_rpc "RAW ${line}"
                continue
            fi

            local line_method
            line_method=$(echo "$line" | jq -r '.method // empty' 2>/dev/null || true)
            if [[ -n "$line_method" ]]; then
                emit_line "[$(ts_human)] [RPC][NOTIFY] payload=${line}"
                record_rpc "NOTIFY ${line}"
            fi

            local line_id
            line_id=$(echo "$line" | jq -r '.id // empty' 2>/dev/null || true)
            if [[ "$line_id" == "$request_id" ]]; then
                emit_line "[$(ts_human)] [RPC][RES] [network=${NETWORK_STATE}] method=${method} id=${request_id} payload=${line}"
                record_rpc "RESPONSE method=${method} id=${request_id} payload=${line}"
                printf '%s\n' "$line"
                return 0
            fi
        else
            break
        fi
    done

    local timeout_response='{"error":{"message":"timeout"}}'
    emit_line "[$(ts_human)] [RPC][TIMEOUT] method=${method} id=${request_id} timeout_s=${timeout}"
    record_rpc "TIMEOUT method=${method} id=${request_id} timeout_s=${timeout}"
    printf '%s\n' "$timeout_response"
    return 1
}

assert_has_result() {
    local response="$1"
    local label="$2"
    if echo "$response" | jq -e '.result' >/dev/null 2>&1; then
        return 0
    fi
    LAST_ERROR="${label} did not return result"
    return 1
}

assert_network_error_actionable() {
    local response="$1"
    local label="$2"

    local kind
    kind=$(echo "$response" | jq -r '.error.data.kind // ""' 2>/dev/null || true)
    kind=$(echo "$kind" | tr '[:lower:]' '[:upper:]')

    if [[ "$kind" != "E_NETWORK" ]]; then
        LAST_ERROR="${label} did not return E_NETWORK (kind=${kind:-missing})"
        return 1
    fi

    local message_blob
    message_blob=$(echo "$response" | jq -r '[.error.message // "", .error.data.details.reason // "", .error.data.details.suggested_recovery // ""] | join(" ")' 2>/dev/null || true)
    local normalized
    normalized=$(echo "$message_blob" | tr '[:upper:]' '[:lower:]')

    if [[ "$normalized" != *"network"* ]]; then
        LAST_ERROR="${label} E_NETWORK message missing network context"
        return 1
    fi

    if [[ "$normalized" != *"retry"* ]] && [[ "$normalized" != *"check"* ]]; then
        LAST_ERROR="${label} E_NETWORK message missing retry/check guidance"
        return 1
    fi

    return 0
}

prepare_offline_manifest_fixture() {
    OFFLINE_SHARED_ROOT=$(mktemp -d)
    OFFLINE_XDG_CACHE_BASE=$(mktemp -d)

    mkdir -p "$OFFLINE_SHARED_ROOT/model"

    cat > "$OFFLINE_SHARED_ROOT/model/MODEL_MANIFEST.json" <<JSON
{
  "schema_version": "1",
  "model_id": "${OFFLINE_MODEL_ID}",
  "display_name": "Offline E2E Model",
  "source": "offline/e2e",
  "source_url": "https://offline.invalid/models",
  "revision": "offline-e2e-r1",
  "files": [
    {
      "path": "offline-e2e.bin",
      "size_bytes": 1024,
      "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "description": "Offline install test payload"
    }
  ],
  "total_size_bytes": 1024,
  "mirrors": [
    {
      "provider": "offline-mock",
      "url": "https://offline.invalid/models/offline-e2e.bin",
      "auth_required": false
    }
  ]
}
JSON
}

dump_failure_context() {
    emit_line "[$(ts_human)] [FAILURE] ${LAST_ERROR}"

    emit_line "[$(ts_human)] [FAILURE] Last 8 RPC entries:"
    local rpc_count=${#RPC_HISTORY[@]}
    local from=$(( rpc_count > 8 ? rpc_count - 8 : 0 ))
    local i
    for (( i=from; i<rpc_count; i++ )); do
        emit_line "[$(ts_human)] [FAILURE][RPC] ${RPC_HISTORY[$i]}"
    done

    emit_line "[$(ts_human)] [FAILURE] Cache history:"
    local cache_count=${#CACHE_HISTORY[@]}
    local j
    for (( j=0; j<cache_count; j++ )); do
        emit_line "[$(ts_human)] [FAILURE][CACHE] ${CACHE_HISTORY[$j]}"
    done
}

cleanup() {
    local exit_code=$?

    { exec 4<&-; } 2>/dev/null || true
    stop_sidecar || true

    if [[ -n "$OFFLINE_SHARED_ROOT" ]]; then
        rm -rf "$OFFLINE_SHARED_ROOT" || true
    fi
    if [[ -n "$OFFLINE_XDG_CACHE_BASE" ]]; then
        rm -rf "$OFFLINE_XDG_CACHE_BASE" || true
    fi

    restore_env

    if [[ "$exit_code" -ne 0 && "$exit_code" -ne 77 && -n "$LAST_ERROR" ]]; then
        dump_failure_context
    fi

    return "$exit_code"
}

main() {
    require_jq
    init_common
    trap cleanup EXIT INT TERM

    mkdir -p "$E2E_PROJECT_ROOT/logs/e2e"
    TEST_LOG_FILE="$E2E_PROJECT_ROOT/logs/e2e/test-offline-install-$(date -u +%Y%m%dT%H%M%S).log"
    touch "$TEST_LOG_FILE"

    emit_line "[$(ts_human)] [START] Offline install behavior E2E"
    emit_line "[$(ts_human)] [START] Log file: $TEST_LOG_FILE"

    HOST_CACHE_DIR=$(host_cache_dir)
    emit_cache_snapshot "host-before" "$HOST_CACHE_DIR"

    CACHED_MODEL_ID=$(discover_cached_model_id "$HOST_CACHE_DIR" || true)
    if [[ -z "$CACHED_MODEL_ID" ]]; then
        emit_line "[$(ts_human)] [SKIP] No usable cached model found in $HOST_CACHE_DIR"
        exit 77
    fi

    step_log 1 "Start sidecar with offline-mocked network"
    set_offline_network
    unset OPENVOICY_SHARED_ROOT
    unset XDG_CACHE_HOME

    start_sidecar_session || {
        LAST_ERROR="failed to start sidecar in offline mode"
        return 1
    }

    local ping_offline
    ping_offline=$(sidecar_rpc_session "system.ping" "{}" 8 || true)
    assert_has_result "$ping_offline" "system.ping (offline startup)" || return 1

    step_log 2 "Verify cached model remains usable via asr.initialize"
    local init_params
    init_params=$(jq -nc --arg model_id "$CACHED_MODEL_ID" '{model_id:$model_id}')

    local init_response
    init_response=$(sidecar_rpc_session "asr.initialize" "$init_params" 45 || true)

    if ! assert_has_result "$init_response" "asr.initialize (offline cached model)"; then
        local init_kind
        init_kind=$(echo "$init_response" | jq -r '.error.data.kind // ""' 2>/dev/null || true)
        if [[ "$init_kind" == "E_MODEL_NOT_FOUND" ]] || [[ "$init_kind" == "E_MODEL_LOAD" ]] || [[ "$init_kind" == "E_NOT_READY" ]]; then
            emit_line "[$(ts_human)] [SKIP] Cached model '$CACHED_MODEL_ID' not usable on this host (${init_kind})"
            return 77
        fi
        return 1
    fi

    local status_after_init
    status_after_init=$(sidecar_rpc_session "status.get" "{}" 8 || true)
    assert_has_result "$status_after_init" "status.get after offline initialize" || return 1

    stop_sidecar_session

    step_log 3 "Attempt model.download with unreachable mirror (expect E_NETWORK)"
    prepare_offline_manifest_fixture

    export OPENVOICY_SHARED_ROOT="$OFFLINE_SHARED_ROOT"
    export XDG_CACHE_HOME="$OFFLINE_XDG_CACHE_BASE"
    set_offline_network

    local isolated_cache_dir="$OFFLINE_XDG_CACHE_BASE/openvoicy/models"
    emit_cache_snapshot "isolated-before" "$isolated_cache_dir"

    start_sidecar_session || {
        LAST_ERROR="failed to start sidecar for isolated offline fixture"
        return 1
    }

    local download_error
    download_error=$(sidecar_rpc_session "model.download" "{}" 25 || true)
    assert_network_error_actionable "$download_error" "model.download offline attempt" || return 1

    step_log 4 "Verify no partial/corrupt model files are left behind"
    emit_cache_snapshot "isolated-after-failure" "$isolated_cache_dir"

    local partial_dir="$isolated_cache_dir/.partial/$OFFLINE_MODEL_ID"
    local final_dir="$isolated_cache_dir/$OFFLINE_MODEL_ID"

    if [[ -d "$partial_dir" ]]; then
        LAST_ERROR="partial staging directory still exists after failed download: $partial_dir"
        return 1
    fi

    if [[ -d "$final_dir" ]] && [[ ! -f "$final_dir/manifest.json" ]]; then
        LAST_ERROR="corrupt final model directory detected without manifest: $final_dir"
        return 1
    fi

    local ping_during_failure
    ping_during_failure=$(sidecar_rpc_session "system.ping" "{}" 8 || true)
    assert_has_result "$ping_during_failure" "system.ping after failed download" || return 1

    local status_after_failure
    status_after_failure=$(sidecar_rpc_session "status.get" "{}" 8 || true)
    assert_has_result "$status_after_failure" "status.get after failed download" || return 1

    stop_sidecar_session

    step_log 5 "Re-enable network and retry (verify error remains actionable)"
    set_online_network
    start_sidecar_session || {
        LAST_ERROR="failed to restart sidecar after re-enabling network"
        return 1
    }

    local retry_response
    retry_response=$(sidecar_rpc_session "model.download" "{}" 25 || true)
    assert_network_error_actionable "$retry_response" "model.download retry" || return 1

    step_log 6 "Verify sidecar remains functional after retry"
    local ping_after_retry
    ping_after_retry=$(sidecar_rpc_session "system.ping" "{}" 8 || true)
    assert_has_result "$ping_after_retry" "system.ping after retry" || return 1

    local status_after_retry
    status_after_retry=$(sidecar_rpc_session "status.get" "{}" 8 || true)
    assert_has_result "$status_after_retry" "status.get after retry" || return 1

    stop_sidecar_session

    emit_cache_snapshot "host-after" "$HOST_CACHE_DIR"

    emit_line "[$(ts_human)] [PASS] Offline install behavior checks passed"
    emit_line "[$(ts_human)] [PASS] Log file: $TEST_LOG_FILE"
}

main
