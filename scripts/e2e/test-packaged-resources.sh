#!/usr/bin/env bash
#
# E2E Check: Packaged-style sidecar resource resolution
#
# This script simulates a packaged app layout by staging shared resources
# into a temporary directory and running:
#   python -m openvoicy_sidecar.self_test
# with OPENVOICY_SHARED_ROOT pointing at that staged tree.
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

cleanup() {
    rm -rf "$TEMP_ROOT"
}
trap cleanup EXIT

echo "[PACKAGED_RESOURCES] staging shared resources under: $STAGED_SHARED"
mkdir -p "$STAGED_SHARED"

cp -R "$REPO_ROOT/shared/contracts" "$STAGED_SHARED/"
cp -R "$REPO_ROOT/shared/model" "$STAGED_SHARED/"
cp -R "$REPO_ROOT/shared/replacements" "$STAGED_SHARED/"

if [[ -z "${PYTHONPATH:-}" ]]; then
    export PYTHONPATH="$REPO_ROOT/sidecar/src"
else
    export PYTHONPATH="$REPO_ROOT/sidecar/src:$PYTHONPATH"
fi
export OPENVOICY_SHARED_ROOT="$STAGED_SHARED"

echo "[PACKAGED_RESOURCES] running sidecar self-test in packaged context simulation"
python3 -m openvoicy_sidecar.self_test
echo "[PACKAGED_RESOURCES] self-test passed"
