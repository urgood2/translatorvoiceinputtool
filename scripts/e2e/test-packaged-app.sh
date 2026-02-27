#!/usr/bin/env bash
#
# Packaged App Smoke Test
#
# Verifies that the bundled sidecar can locate packaged shared resources and
# that system.info reports resource paths consistent with the bundle layout.
#
# Usage:
#   ./scripts/e2e/test-packaged-app.sh [--target TARGET_TRIPLE]
#

set -euo pipefail

if [[ -n "${BASH_SOURCE[0]:-}" ]]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi

REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TAURI_BINARIES="$REPO_ROOT/src-tauri/binaries"
BUNDLE_ROOT="$REPO_ROOT/src-tauri/target/release/bundle"

TARGET=""

find_bundle_artifact() {
    if [[ ! -d "$BUNDLE_ROOT" ]]; then
        return 1
    fi

    if [[ "$TARGET" == *"apple-darwin"* ]]; then
        local app_dir
        app_dir="$(find "$BUNDLE_ROOT" -maxdepth 4 -type d -name "*.app" | sort | head -n 1)"
        if [[ -n "$app_dir" ]]; then
            echo "$app_dir"
            return 0
        fi

        find "$BUNDLE_ROOT" -maxdepth 4 -type f -name "*.dmg" | sort | head -n 1
        return 0
    fi

    if [[ "$TARGET" == *"linux"* ]]; then
        local linux_artifact
        linux_artifact="$(
            find "$BUNDLE_ROOT" -maxdepth 4 -type f \
                \( -name "*.AppImage" -o -name "*.deb" -o -name "*.rpm" \) \
                | sort \
                | head -n 1
        )"
        if [[ -n "$linux_artifact" ]]; then
            echo "$linux_artifact"
            return 0
        fi
    fi

    return 1
}

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
        echo "[PACKAGED_APP][ERROR] No supported timeout runner found (timeout/gtimeout/python3)" >&2
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
            echo "[PACKAGED_APP][ERROR] Unsupported timeout runner: $runner" >&2
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
            echo "[PACKAGED_APP][ERROR] Unsupported OS: $(uname -s)" >&2
            exit 2
            ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64) arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *)
            echo "[PACKAGED_APP][ERROR] Unsupported architecture: $(uname -m)" >&2
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
            echo "[PACKAGED_APP][ERROR] Unknown option: $1" >&2
            exit 2
            ;;
    esac
done

if [[ -z "$TARGET" ]]; then
    TARGET="$(detect_target_triple)"
fi

if [[ "$TARGET" == *"windows"* ]]; then
    echo "[PACKAGED_APP][SKIP] Windows packaged smoke test is not supported in this host shell"
    exit 77
fi

if [[ ! -d "$BUNDLE_ROOT" ]]; then
    echo "[PACKAGED_APP][ERROR] Missing packaged app bundle output directory: $BUNDLE_ROOT" >&2
    echo "[PACKAGED_APP][ERROR] Run npm run tauri build first to generate real bundle artifacts" >&2
    exit 2
fi

BUNDLE_ARTIFACT="$(find_bundle_artifact || true)"
if [[ -z "$BUNDLE_ARTIFACT" ]]; then
    echo "[PACKAGED_APP][ERROR] No packaged app artifact found under $BUNDLE_ROOT for target $TARGET" >&2
    echo "[PACKAGED_APP][ERROR] Expected at least one platform artifact (.AppImage/.deb/.rpm/.app/.dmg)" >&2
    exit 1
fi

SIDECAR_BIN="$TAURI_BINARIES/openvoicy-sidecar-$TARGET"
if [[ ! -f "$SIDECAR_BIN" ]]; then
    echo "[PACKAGED_APP][ERROR] Missing packaged sidecar binary: $SIDECAR_BIN" >&2
    echo "[PACKAGED_APP][ERROR] Run ./scripts/build-sidecar.sh and ./scripts/bundle-sidecar.sh first" >&2
    exit 2
fi

SHARED_ROOT="$TAURI_BINARIES/shared"
if [[ ! -d "$SHARED_ROOT" ]]; then
    echo "[PACKAGED_APP][ERROR] Missing packaged shared root: $SHARED_ROOT" >&2
    exit 2
fi

echo "[PACKAGED_APP] target=$TARGET"
echo "[PACKAGED_APP] bundle_root=$BUNDLE_ROOT"
echo "[PACKAGED_APP] bundle_artifact=$BUNDLE_ARTIFACT"
echo "[PACKAGED_APP] sidecar=$SIDECAR_BIN"
echo "[PACKAGED_APP] shared_root=$SHARED_ROOT"
echo "[PACKAGED_APP] Real bundle output summary:"
find "$BUNDLE_ROOT" -maxdepth 4 -print | sort | sed 's#^#[PACKAGED_APP]   #'
echo "[PACKAGED_APP] Bundle contents summary:"
find "$TAURI_BINARIES" -maxdepth 4 -type f | sort | sed 's#^#[PACKAGED_APP]   #'

check_path() {
    local label="$1"
    local relative="$2"
    local expected="$SHARED_ROOT/$relative"
    local actual=""
    if [[ -e "$expected" ]]; then
        actual="$(cd "$(dirname "$expected")" && pwd)/$(basename "$expected")"
    fi
    echo "[PACKAGED_APP] check $label expected=$expected actual=${actual:-<missing>}"
    [[ -e "$expected" ]]
}

check_path "presets" "replacements/PRESETS.json"
check_path "model manifest" "model/MODEL_MANIFEST.json"
check_path "model catalog" "model/MODEL_CATALOG.json"
check_path "contracts dir" "contracts"

if ! compgen -G "$SHARED_ROOT/model/manifests/*.json" >/dev/null; then
    echo "[PACKAGED_APP][ERROR] Missing packaged model manifests under $SHARED_ROOT/model/manifests" >&2
    exit 1
fi

if [[ -z "${PYTHONPATH:-}" ]]; then
    export PYTHONPATH="$REPO_ROOT/sidecar/src"
else
    export PYTHONPATH="$REPO_ROOT/sidecar/src:$PYTHONPATH"
fi

export OPENVOICY_SHARED_ROOT="$SHARED_ROOT"
export OPENVOICY_SIDECAR_COMMAND="$SIDECAR_BIN"

echo "[PACKAGED_APP] Running python -m openvoicy_sidecar.self_test against packaged sidecar..."
python3 -m openvoicy_sidecar.self_test

echo "[PACKAGED_APP] Querying system.info from packaged sidecar..."
SYSTEM_INFO_RAW="$(
    echo '{"jsonrpc":"2.0","id":1,"method":"system.info","params":{}}' \
    | run_with_timeout 10 "$SIDECAR_BIN" 2>/dev/null
)"

if [[ -z "$SYSTEM_INFO_RAW" ]]; then
    echo "[PACKAGED_APP][ERROR] system.info returned no output" >&2
    exit 1
fi

python3 - "$SYSTEM_INFO_RAW" "$SHARED_ROOT" <<'PY'
import json
import os
import sys
from pathlib import Path

raw = sys.argv[1].strip().splitlines()[0]
shared_root = Path(sys.argv[2]).resolve()
payload = json.loads(raw)
result = payload.get("result")
if not isinstance(result, dict):
    raise SystemExit("[PACKAGED_APP][ERROR] system.info result missing")

resource_paths = result.get("resource_paths")
if not isinstance(resource_paths, dict):
    raise SystemExit("[PACKAGED_APP][ERROR] system.info.resource_paths missing")

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
        raise SystemExit(f"[PACKAGED_APP][ERROR] system.info.resource_paths.{key} missing")
    actual_path = Path(actual).resolve()
    print(
        f"[PACKAGED_APP] system.info {key}: expected={expected_path} actual={actual_path}",
        flush=True,
    )
    if actual_path != expected_path:
        raise SystemExit(
            f"[PACKAGED_APP][ERROR] resource path mismatch for {key}: "
            f"expected={expected_path} actual={actual_path}"
        )
    if not actual_path.exists():
        raise SystemExit(
            f"[PACKAGED_APP][ERROR] system.info path does not exist for {key}: {actual_path}"
        )

print("[PACKAGED_APP] system.info resource path validation: OK", flush=True)
PY

echo "[PACKAGED_APP] packaged app smoke test passed"
