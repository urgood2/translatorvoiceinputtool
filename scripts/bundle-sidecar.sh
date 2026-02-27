#!/usr/bin/env bash
#
# Bundle Sidecar for Tauri
#
# Copies the PyInstaller-built sidecar binary to the Tauri binaries directory
# with the correct target-triple naming for cross-platform bundling.
#
# Usage:
#   ./scripts/bundle-sidecar.sh [--target TARGET_TRIPLE] [--smoke-test]
#
# Examples:
#   ./scripts/bundle-sidecar.sh                                    # Auto-detect current platform
#   ./scripts/bundle-sidecar.sh --target x86_64-unknown-linux-gnu  # Explicit target
#   ./scripts/bundle-sidecar.sh --target x86_64-pc-windows-msvc    # Windows target
#
# The script expects the sidecar binary to already be built via:
#   ./scripts/build-sidecar.sh
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Directories
SIDECAR_DIST="$PROJECT_ROOT/sidecar/dist"
TAURI_BINARIES="$PROJECT_ROOT/src-tauri/binaries"
PROJECT_SHARED_ROOT="$PROJECT_ROOT/shared"
PROJECT_SHARED_MODEL="$PROJECT_SHARED_ROOT/model"
PROJECT_SHARED_MANIFESTS="$PROJECT_SHARED_MODEL/manifests"
PROJECT_SHARED_CONTRACTS="$PROJECT_SHARED_ROOT/contracts"
PROJECT_SHARED_REPLACEMENTS="$PROJECT_SHARED_ROOT/replacements"
TAURI_SHARED_ROOT="$TAURI_BINARIES/shared"
TAURI_SHARED_MODEL="$TAURI_SHARED_ROOT/model"
TAURI_SHARED_MANIFESTS="$TAURI_SHARED_MODEL/manifests"
TAURI_SHARED_CONTRACTS="$TAURI_SHARED_ROOT/contracts"
TAURI_SHARED_REPLACEMENTS="$TAURI_SHARED_ROOT/replacements"

# Binary names
SIDECAR_NAME="openvoicy-sidecar"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
log_step() { echo -e "${BLUE}[STEP]${NC} $*"; }

# Detect target triple for current platform
detect_target_triple() {
    local os arch

    case "$(uname -s)" in
        Linux)
            os="unknown-linux-gnu"
            ;;
        Darwin)
            os="apple-darwin"
            ;;
        MINGW*|CYGWIN*|MSYS*)
            os="pc-windows-msvc"
            ;;
        *)
            log_error "Unknown OS: $(uname -s)"
            exit 1
            ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        aarch64|arm64)
            arch="aarch64"
            ;;
        *)
            log_error "Unknown architecture: $(uname -m)"
            exit 1
            ;;
    esac

    echo "${arch}-${os}"
}

# Get source binary path
get_source_binary() {
    local target="$1"

    if [[ "$target" == *"windows"* ]]; then
        echo "$SIDECAR_DIST/${SIDECAR_NAME}.exe"
    else
        echo "$SIDECAR_DIST/${SIDECAR_NAME}"
    fi
}

# Get destination binary path with target triple
get_dest_binary() {
    local target="$1"

    if [[ "$target" == *"windows"* ]]; then
        echo "$TAURI_BINARIES/${SIDECAR_NAME}-${target}.exe"
    else
        echo "$TAURI_BINARIES/${SIDECAR_NAME}-${target}"
    fi
}

# Main function
main() {
    local target=""
    local run_smoke_test=false

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --target)
                target="$2"
                shift 2
                ;;
            --smoke-test)
                run_smoke_test=true
                shift
                ;;
            -h|--help)
                echo "Usage: $0 [--target TARGET_TRIPLE] [--smoke-test]"
                echo ""
                echo "Bundles the sidecar binary for Tauri with proper naming."
                echo ""
                echo "Options:"
                echo "  --target  Target triple (e.g., x86_64-unknown-linux-gnu)"
                echo "            Auto-detected if not specified."
                echo "  --smoke-test"
                echo "            Run scripts/e2e/test-packaged-app.sh after bundling."
                echo ""
                echo "Supported targets:"
                echo "  x86_64-unknown-linux-gnu    Linux x64"
                echo "  aarch64-unknown-linux-gnu   Linux ARM64"
                echo "  x86_64-apple-darwin         macOS Intel"
                echo "  aarch64-apple-darwin        macOS Apple Silicon"
                echo "  x86_64-pc-windows-msvc      Windows x64"
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done

    # Auto-detect target if not specified
    if [[ -z "$target" ]]; then
        target=$(detect_target_triple)
        log_info "Auto-detected target: $target"
    fi

    local source_bin dest_bin
    source_bin=$(get_source_binary "$target")
    dest_bin=$(get_dest_binary "$target")

    echo "=================================="
    echo "  Bundle Sidecar for Tauri"
    echo "=================================="
    echo ""
    echo "Target:      $target"
    echo "Source:      $source_bin"
    echo "Destination: $dest_bin"
    echo ""

    # Check source exists
    log_step "Checking source binary..."
    if [[ ! -f "$source_bin" ]]; then
        log_error "Source binary not found: $source_bin"
        log_error "Run ./scripts/build-sidecar.sh first"
        exit 1
    fi
    if [[ ! -f "$PROJECT_SHARED_MODEL/MODEL_CATALOG.json" ]]; then
        log_error "Model catalog not found: $PROJECT_SHARED_MODEL/MODEL_CATALOG.json"
        log_error "Run ./scripts/build-sidecar.sh first"
        exit 1
    fi
    if [[ ! -f "$PROJECT_SHARED_MODEL/MODEL_MANIFEST.json" ]]; then
        log_error "Model manifest not found: $PROJECT_SHARED_MODEL/MODEL_MANIFEST.json"
        log_error "Run ./scripts/build-sidecar.sh first"
        exit 1
    fi
    if [[ ! -d "$PROJECT_SHARED_MANIFESTS" ]]; then
        log_error "Model manifests directory not found: $PROJECT_SHARED_MANIFESTS"
        log_error "Run ./scripts/build-sidecar.sh first"
        exit 1
    fi
    if ! compgen -G "$PROJECT_SHARED_MANIFESTS/*.json" >/dev/null; then
        log_error "No model manifest JSON files found in: $PROJECT_SHARED_MANIFESTS"
        log_error "Run ./scripts/build-sidecar.sh first"
        exit 1
    fi
    if [[ ! -d "$PROJECT_SHARED_CONTRACTS" ]]; then
        log_error "Contracts directory not found: $PROJECT_SHARED_CONTRACTS"
        exit 1
    fi
    if ! find "$PROJECT_SHARED_CONTRACTS" -type f | grep -q .; then
        log_error "No contract files found in: $PROJECT_SHARED_CONTRACTS"
        exit 1
    fi
    if [[ ! -d "$PROJECT_SHARED_REPLACEMENTS" ]]; then
        log_error "Replacements directory not found: $PROJECT_SHARED_REPLACEMENTS"
        exit 1
    fi
    if [[ ! -f "$PROJECT_SHARED_REPLACEMENTS/PRESETS.json" ]]; then
        log_error "Preset file not found: $PROJECT_SHARED_REPLACEMENTS/PRESETS.json"
        exit 1
    fi

    local source_size
    source_size=$(stat -c%s "$source_bin" 2>/dev/null || stat -f%z "$source_bin")
    log_info "Source binary: $(numfmt --to=iec-i --suffix=B "$source_size" 2>/dev/null || echo "$((source_size / 1024 / 1024)) MB")"

    # Create destination directory
    log_step "Creating Tauri binaries directory..."
    mkdir -p "$TAURI_BINARIES" "$TAURI_SHARED_MANIFESTS"

    # Copy binary
    log_step "Copying binary..."
    cp "$source_bin" "$dest_bin"

    # Ensure executable permissions (Unix)
    if [[ ! "$target" == *"windows"* ]]; then
        chmod +x "$dest_bin"
    fi

    # Copy canonical shared resources for packaged runtime resolution.
    log_step "Copying shared resources..."
    rm -rf "$TAURI_SHARED_CONTRACTS" "$TAURI_SHARED_REPLACEMENTS"
    cp "$PROJECT_SHARED_MODEL/MODEL_CATALOG.json" "$TAURI_SHARED_MODEL/MODEL_CATALOG.json"
    cp "$PROJECT_SHARED_MODEL/MODEL_MANIFEST.json" "$TAURI_SHARED_MODEL/MODEL_MANIFEST.json"
    cp "$PROJECT_SHARED_MANIFESTS"/*.json "$TAURI_SHARED_MANIFESTS/"
    cp -R "$PROJECT_SHARED_CONTRACTS" "$TAURI_SHARED_CONTRACTS"
    cp -R "$PROJECT_SHARED_REPLACEMENTS" "$TAURI_SHARED_REPLACEMENTS"

    # Verify copy
    log_step "Verifying..."
    if [[ ! -f "$dest_bin" ]]; then
        log_error "Failed to copy binary"
        exit 1
    fi

    local dest_size
    dest_size=$(stat -c%s "$dest_bin" 2>/dev/null || stat -f%z "$dest_bin")

    if [[ "$source_size" != "$dest_size" ]]; then
        log_error "Size mismatch after copy!"
        exit 1
    fi
    if [[ ! -f "$TAURI_SHARED_MODEL/MODEL_CATALOG.json" ]]; then
        log_error "MODEL_CATALOG.json missing after copy"
        exit 1
    fi
    if [[ ! -f "$TAURI_SHARED_MODEL/MODEL_MANIFEST.json" ]]; then
        log_error "MODEL_MANIFEST.json missing after copy"
        exit 1
    fi
    if ! compgen -G "$TAURI_SHARED_MANIFESTS/*.json" >/dev/null; then
        log_error "No copied model manifests found in: $TAURI_SHARED_MANIFESTS"
        exit 1
    fi
    if [[ ! -d "$TAURI_SHARED_CONTRACTS" ]]; then
        log_error "contracts directory missing after copy"
        exit 1
    fi
    if ! find "$TAURI_SHARED_CONTRACTS" -type f | grep -q .; then
        log_error "No copied contract files found in: $TAURI_SHARED_CONTRACTS"
        exit 1
    fi
    if [[ ! -f "$TAURI_SHARED_REPLACEMENTS/PRESETS.json" ]]; then
        log_error "PRESETS.json missing after copy"
        exit 1
    fi

    # Quick self-check (Unix only)
    if [[ ! "$target" == *"windows"* ]]; then
        log_step "Running sidecar self-check..."
        local ping_result
        if ping_result=$(echo '{"jsonrpc":"2.0","id":1,"method":"system.ping","params":{}}' | timeout 10 "$dest_bin" 2>/dev/null); then
            if echo "$ping_result" | grep -q '"protocol":"v1"'; then
                log_info "Sidecar self-check passed"
            else
                log_warn "Sidecar responded but protocol check failed"
            fi
        else
            log_warn "Could not verify sidecar (may need runtime dependencies)"
        fi
    fi

    echo ""
    log_info "Sidecar bundled successfully!"
    echo ""
    echo "Bundled binary: $dest_bin"
    echo ""
    echo "Next steps:"
    echo "  1. Build Tauri app: cd src-tauri && cargo tauri build"
    echo "  2. The sidecar will be included in the app bundle"
    echo ""

    # List all bundled binaries
    if [[ -d "$TAURI_BINARIES" ]]; then
        echo "Bundled sidecars:"
        ls -la "$TAURI_BINARIES"/ 2>/dev/null || true
    fi

    if [[ "$run_smoke_test" == true ]]; then
        log_step "Running packaged app smoke test..."
        "$PROJECT_ROOT/scripts/e2e/test-packaged-app.sh" --target "$target"
    fi
}

main "$@"
