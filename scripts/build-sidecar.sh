#!/usr/bin/env bash
#
# Build OpenVoicy sidecar as standalone executable using PyInstaller.
# Works on Linux and macOS.
#
# Usage: ./scripts/build-sidecar.sh [--clean] [--no-verify]
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SIDECAR_DIR="$PROJECT_ROOT/sidecar"
DIST_DIR="$SIDECAR_DIR/dist"

# Resolve timeout runner in a cross-platform way.
# Order:
#  1) Explicit override via SIDECAR_TIMEOUT_RUNNER (timeout|gtimeout|python3)
#  2) Auto-detect timeout, then gtimeout, then python3 fallback
sidecar_timeout_runner() {
    local preferred="${SIDECAR_TIMEOUT_RUNNER:-auto}"

    case "$preferred" in
        ""|auto)
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
            ;;
        timeout|gtimeout|python3)
            if command -v "$preferred" >/dev/null 2>&1; then
                echo "$preferred"
                return 0
            fi
            return 1
            ;;
        *)
            return 2
            ;;
    esac
}

# Run a command with timeout seconds, portable across Linux/macOS.
# Args: timeout_seconds, command, [args...]
sidecar_timeout_run() {
    local seconds="$1"
    shift

    local runner
    runner="$(sidecar_timeout_runner)" || {
        echo "ERROR: No supported timeout runner found (timeout/gtimeout/python3)" >&2
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
    # Match GNU timeout timeout exit code.
    sys.exit(124)

sys.exit(completed.returncode)
PY
            ;;
        *)
            echo "ERROR: Unsupported timeout runner: $runner" >&2
            return 127
            ;;
    esac
}

# Parse arguments
CLEAN=false
VERIFY=true
for arg in "$@"; do
    case $arg in
        --clean) CLEAN=true ;;
        --no-verify) VERIFY=false ;;
        -h|--help)
            echo "Usage: $0 [--clean] [--no-verify]"
            echo "  --clean     Remove build artifacts before building"
            echo "  --no-verify Skip binary verification step"
            exit 0
            ;;
    esac
done

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
    Linux)  PLATFORM="linux" ;;
    Darwin) PLATFORM="macos" ;;
    *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac
case "$ARCH" in
    x86_64)  ARCH_SUFFIX="x64" ;;
    aarch64) ARCH_SUFFIX="arm64" ;;
    arm64)   ARCH_SUFFIX="arm64" ;;
    *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

ARTIFACT_NAME="openvoicy-sidecar"
PLATFORM_TAG="${PLATFORM}-${ARCH_SUFFIX}"

echo "=== Building OpenVoicy Sidecar ==="
echo "Platform: $PLATFORM_TAG"
echo "Sidecar dir: $SIDECAR_DIR"
echo ""

cd "$SIDECAR_DIR"

# Clean if requested
if [ "$CLEAN" = true ]; then
    echo "Cleaning build artifacts..."
    rm -rf build/ dist/ __pycache__/
fi

# Ensure virtual environment exists
if [ ! -d ".venv" ]; then
    echo "Creating virtual environment..."
    python3 -m venv .venv
fi

# Activate venv
source .venv/bin/activate

# Install/upgrade dependencies
echo "Installing dependencies..."
pip install --quiet --upgrade pip
pip install --quiet -e .
pip install --quiet pyinstaller

# Check for PortAudio on Linux
if [ "$PLATFORM" = "linux" ]; then
    if ! ldconfig -p | grep -q libportaudio; then
        echo "WARNING: libportaudio not found in system libraries"
        echo "Install with: sudo apt-get install libportaudio2"
    fi
fi

# Run PyInstaller
echo ""
echo "Running PyInstaller..."
BUILD_START=$(date +%s)
pyinstaller --noconfirm openvoicy_sidecar.spec
BUILD_END=$(date +%s)
BUILD_TIME=$((BUILD_END - BUILD_START))
echo "Build completed in ${BUILD_TIME}s"

# Check binary exists
BINARY_PATH="$DIST_DIR/$ARTIFACT_NAME"
if [ ! -f "$BINARY_PATH" ]; then
    echo "ERROR: Binary not found at $BINARY_PATH"
    exit 1
fi

# Get binary size
BINARY_SIZE=$(stat -c%s "$BINARY_PATH" 2>/dev/null || stat -f%z "$BINARY_PATH")
BINARY_SIZE_MB=$(echo "scale=2; $BINARY_SIZE / 1048576" | bc)
echo "Binary size: ${BINARY_SIZE_MB} MB ($BINARY_SIZE bytes)"

# Verify binary (unless skipped)
STARTUP_TIME_MS=0
if [ "$VERIFY" = true ]; then
    echo ""
    echo "Verifying binary..."

    # Test system.ping
    VERIFY_START=$(date +%s%3N 2>/dev/null || python3 -c "import time; print(int(time.time()*1000))")
    PING_RESULT=$(echo '{"jsonrpc":"2.0","id":1,"method":"system.ping"}' | sidecar_timeout_run 10 "$BINARY_PATH" 2>/dev/null || echo "FAILED")
    VERIFY_END=$(date +%s%3N 2>/dev/null || python3 -c "import time; print(int(time.time()*1000))")
    STARTUP_TIME_MS=$((VERIFY_END - VERIFY_START))

    if echo "$PING_RESULT" | grep -q '"result"'; then
        echo "✓ system.ping: OK (${STARTUP_TIME_MS}ms)"
    else
        echo "✗ system.ping: FAILED"
        echo "  Response: $PING_RESULT"
        exit 1
    fi

    # Test audio.list_devices
    DEVICES_RESULT=$(echo '{"jsonrpc":"2.0","id":2,"method":"audio.list_devices"}' | sidecar_timeout_run 10 "$BINARY_PATH" 2>/dev/null || echo "FAILED")
    if echo "$DEVICES_RESULT" | grep -q '"result"'; then
        echo "✓ audio.list_devices: OK"
    else
        echo "✗ audio.list_devices: FAILED"
        echo "  Response: $DEVICES_RESULT"
        exit 1
    fi

    echo ""
    echo "Verification passed!"
fi

# Generate manifest
echo ""
echo "Generating manifest..."
GIT_SHA=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
BUILD_TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
PYTHON_VERSION=$(python3 --version | awk '{print $2}')

# Get version from pyproject.toml
VERSION=$(grep '^version' pyproject.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

# Determine ONNX status (not yet included in minimal build)
ONNX_VERSION="not-included"
if pip show onnxruntime >/dev/null 2>&1; then
    ONNX_VERSION=$(pip show onnxruntime | grep Version | awk '{print $2}')
fi

cat > "$DIST_DIR/manifest.json" << EOF
{
  "artifact_name": "$ARTIFACT_NAME",
  "version": "$VERSION",
  "platform": "$PLATFORM_TAG",
  "python_version": "$PYTHON_VERSION",
  "build_timestamp": "$BUILD_TIMESTAMP",
  "git_sha": "$GIT_SHA",
  "binary_size_bytes": $BINARY_SIZE,
  "startup_time_ms": $STARTUP_TIME_MS,
  "gpu_support": "none",
  "onnxruntime_version": "$ONNX_VERSION",
  "build_time_seconds": $BUILD_TIME
}
EOF

echo "Manifest written to: $DIST_DIR/manifest.json"

# Summary
echo ""
echo "=== Build Summary ==="
echo "Artifact: $BINARY_PATH"
echo "Size: ${BINARY_SIZE_MB} MB"
if [ "$STARTUP_TIME_MS" -gt 0 ]; then
    STARTUP_SEC=$(echo "scale=2; $STARTUP_TIME_MS / 1000" | bc)
    echo "Startup time: ${STARTUP_SEC}s"
fi
echo "Manifest: $DIST_DIR/manifest.json"
echo ""

# Check against targets
echo "=== Target Compliance ==="
if (( BINARY_SIZE < 524288000 )); then
    echo "✓ Binary size: ${BINARY_SIZE_MB} MB < 500 MB limit"
else
    echo "✗ Binary size: ${BINARY_SIZE_MB} MB exceeds 500 MB limit"
fi

if [ "$STARTUP_TIME_MS" -gt 0 ] && (( STARTUP_TIME_MS < 5000 )); then
    echo "✓ Startup time: ${STARTUP_TIME_MS}ms < 5000ms limit"
elif [ "$STARTUP_TIME_MS" -gt 0 ]; then
    echo "✗ Startup time: ${STARTUP_TIME_MS}ms exceeds 5000ms limit"
fi

echo ""
echo "Build complete!"
