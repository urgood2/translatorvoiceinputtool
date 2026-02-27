#!/usr/bin/env bash
#
# E2E Check: Packaged-style sidecar resource resolution
#
# This script simulates a packaged app layout by staging shared resources
# into a temporary directory, then querying system.info from the packaged
# sidecar binary with OPENVOICY_SHARED_ROOT pointing at the staged tree.
#

set -euo pipefail

if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TEMP_ROOT="$(mktemp -d -t openvoicy-packaged-resources-XXXXXX)"
STAGED_SHARED="$TEMP_ROOT/shared"
TARGET=""

detect_timeout_runner() {
    if command -v timeout >/dev/null 2>&1; then
        echo "timeout"
        return 0
    fi
    if command -v gtimeout >/dev/null 2>&1; then
        echo "gtimeout"
        return 0
    fi
    if command -v python3 >/dev/null 2>&1; then
        echo "python3"
        return 0
    fi
    return 1
}

run_with_timeout() {
    local seconds="$1"
    shift

    local runner
    runner="$(detect_timeout_runner)" || {
        echo "[PACKAGED_RESOURCES][ERROR] No supported timeout runner found (timeout/gtimeout/python3)" >&2
        return 127
    }

    case "$runner" in
        timeout|gtimeout)
            "$runner" "$seconds" "$@"
            ;;
        python3)
            python3 - "$seconds" "$@" <<'PY'
import subprocess
import sys

timeout_s = float(sys.argv[1])
cmd = sys.argv[2:]

try:
    completed = subprocess.run(
        cmd,
        stdin=sys.stdin.buffer,
        stdout=sys.stdout.buffer,
        stderr=sys.stderr.buffer,
        timeout=timeout_s,
        check=False,
    )
except subprocess.TimeoutExpired:
    sys.exit(124)

sys.exit(completed.returncode)
PY
            ;;
        *)
            echo "[PACKAGED_RESOURCES][ERROR] Unsupported timeout runner: $runner" >&2
            return 127
            ;;
    esac
}

detect_target_triple() {
    local os arch

    case "$(uname -s)" in
        Linux) os="unknown-linux-gnu" ;;
        Darwin) os="apple-darwin" ;;
        MINGW*|CYGWIN*|MSYS*) os="pc-windows-msvc" ;;
        *)
            echo "[PACKAGED_RESOURCES][ERROR] Unsupported OS: $(uname -s)" >&2
            exit 2
            ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64) arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *)
            echo "[PACKAGED_RESOURCES][ERROR] Unsupported architecture: $(uname -m)" >&2
            exit 2
            ;;
    esac

    echo "${arch}-${os}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --target)
            TARGET="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [--target TARGET_TRIPLE]"
            exit 0
            ;;
        *)
            echo "[PACKAGED_RESOURCES][ERROR] Unknown option: $1" >&2
            exit 2
            ;;
    esac
done

if [[ -z "$TARGET" ]]; then
    TARGET="$(detect_target_triple)"
fi

if [[ "$TARGET" == *"windows"* ]]; then
    echo "[PACKAGED_RESOURCES][SKIP] Windows packaged-resource smoke test is not supported in this host shell"
    exit 77
fi

SIDECAR_BIN="$REPO_ROOT/src-tauri/binaries/openvoicy-sidecar-$TARGET"
if [[ ! -f "$SIDECAR_BIN" ]]; then
    echo "[PACKAGED_RESOURCES][ERROR] Missing packaged sidecar binary: $SIDECAR_BIN" >&2
    echo "[PACKAGED_RESOURCES][ERROR] Run ./scripts/build-sidecar.sh and ./scripts/bundle-sidecar.sh first" >&2
    exit 2
fi

cleanup() {
    rm -rf "$TEMP_ROOT"
}
trap cleanup EXIT

echo "[PACKAGED_RESOURCES] staging shared resources under: $STAGED_SHARED"
mkdir -p "$STAGED_SHARED"

cp -R "$REPO_ROOT/shared/contracts" "$STAGED_SHARED/"
cp -R "$REPO_ROOT/shared/model" "$STAGED_SHARED/"
cp -R "$REPO_ROOT/shared/replacements" "$STAGED_SHARED/"

export OPENVOICY_SHARED_ROOT="$STAGED_SHARED"
export OPENVOICY_SIDECAR_COMMAND="$SIDECAR_BIN"

echo "[PACKAGED_RESOURCES] target=$TARGET"
echo "[PACKAGED_RESOURCES] sidecar=$SIDECAR_BIN"

SYSTEM_INFO_PREFLIGHT_RAW="$(
    echo '{"jsonrpc":"2.0","id":1,"method":"system.info","params":{}}' \
    | run_with_timeout 10 "$SIDECAR_BIN" 2>/dev/null
)"

if [[ -z "$SYSTEM_INFO_PREFLIGHT_RAW" ]]; then
    echo "[PACKAGED_RESOURCES][ERROR] system.info returned no output during schema preflight" >&2
    exit 1
fi

python3 - "$SYSTEM_INFO_PREFLIGHT_RAW" <<'PY'
import json
import sys

raw = sys.argv[1].strip().splitlines()[0]
payload = json.loads(raw)
result = payload.get("result")
if not isinstance(result, dict):
    raise SystemExit(
        "[PACKAGED_RESOURCES][ERROR] system.info schema preflight failed: result missing"
    )

capabilities = result.get("capabilities")
if not isinstance(capabilities, list) or not all(isinstance(item, str) for item in capabilities):
    raise SystemExit(
        "[PACKAGED_RESOURCES][ERROR] system.info schema preflight failed: "
        "result.capabilities must be string[] (bundled sidecar appears stale)"
    )

runtime = result.get("runtime")
if not isinstance(runtime, dict):
    raise SystemExit(
        "[PACKAGED_RESOURCES][ERROR] system.info schema preflight failed: "
        "result.runtime must be an object"
    )
if not isinstance(runtime.get("python_version"), str):
    raise SystemExit(
        "[PACKAGED_RESOURCES][ERROR] system.info schema preflight failed: "
        "runtime.python_version must be a string (bundled sidecar appears stale)"
    )
if not isinstance(runtime.get("platform"), str):
    raise SystemExit(
        "[PACKAGED_RESOURCES][ERROR] system.info schema preflight failed: "
        "runtime.platform must be a string"
    )
if not isinstance(runtime.get("cuda_available"), bool):
    raise SystemExit(
        "[PACKAGED_RESOURCES][ERROR] system.info schema preflight failed: "
        "runtime.cuda_available must be a boolean"
    )
print("[PACKAGED_RESOURCES] system.info schema preflight: OK", flush=True)
PY

echo "[PACKAGED_RESOURCES] running packaged sidecar self-test via openvoicy_sidecar.self_test"
python3 -m openvoicy_sidecar.self_test
echo "[PACKAGED_RESOURCES] packaged sidecar self-test passed"

SYSTEM_INFO_RAW="$(
    echo '{"jsonrpc":"2.0","id":1,"method":"system.info","params":{}}' \
    | run_with_timeout 10 "$SIDECAR_BIN" 2>/dev/null
)"

if [[ -z "$SYSTEM_INFO_RAW" ]]; then
    echo "[PACKAGED_RESOURCES][ERROR] system.info returned no output" >&2
    exit 1
fi

python3 - "$SYSTEM_INFO_RAW" "$STAGED_SHARED" <<'PY'
import json
import sys
from pathlib import Path

raw = sys.argv[1].strip().splitlines()[0]
shared_root = Path(sys.argv[2]).resolve()
payload = json.loads(raw)
result = payload.get("result")
if not isinstance(result, dict):
    raise SystemExit("[PACKAGED_RESOURCES][ERROR] system.info result missing")

resource_paths = result.get("resource_paths")
if not isinstance(resource_paths, dict):
    raise SystemExit("[PACKAGED_RESOURCES][ERROR] system.info.resource_paths missing")

expected = {
    "shared_root": shared_root,
    "presets": shared_root / "replacements" / "PRESETS.json",
    "model_manifest": shared_root / "model" / "MODEL_MANIFEST.json",
    "model_catalog": shared_root / "model" / "MODEL_CATALOG.json",
    "contracts_dir": shared_root / "contracts",
}

for key, expected_path in expected.items():
    actual = resource_paths.get(key)
    if not isinstance(actual, str):
        raise SystemExit(f"[PACKAGED_RESOURCES][ERROR] system.info.resource_paths.{key} missing")
    actual_path = Path(actual).resolve()
    print(
        f"[PACKAGED_RESOURCES] system.info {key}: expected={expected_path} actual={actual_path}",
        flush=True,
    )
    if actual_path != expected_path:
        raise SystemExit(
            f"[PACKAGED_RESOURCES][ERROR] resource path mismatch for {key}: "
            f"expected={expected_path} actual={actual_path}"
        )
    if not actual_path.exists():
        raise SystemExit(
            f"[PACKAGED_RESOURCES][ERROR] system.info path does not exist for {key}: {actual_path}"
        )

print("[PACKAGED_RESOURCES] system.info resource path validation: OK", flush=True)
PY

echo "[PACKAGED_RESOURCES] packaged resource smoke test passed"
