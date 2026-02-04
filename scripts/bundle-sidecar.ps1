#
# Bundle Sidecar for Tauri (Windows)
#
# Copies the PyInstaller-built sidecar binary to the Tauri binaries directory
# with the correct target-triple naming for cross-platform bundling.
#
# Usage:
#   .\scripts\bundle-sidecar.ps1 [-Target <TARGET_TRIPLE>]
#
# Examples:
#   .\scripts\bundle-sidecar.ps1                                    # Auto-detect
#   .\scripts\bundle-sidecar.ps1 -Target x86_64-pc-windows-msvc     # Explicit
#

param(
    [string]$Target = ""
)

$ErrorActionPreference = "Stop"

# Directories
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$SidecarDist = Join-Path $ProjectRoot "sidecar\dist"
$TauriBinaries = Join-Path $ProjectRoot "src-tauri\binaries"

# Binary name
$SidecarName = "openvoicy-sidecar"

function Write-Info { param($Message) Write-Host "[INFO] $Message" -ForegroundColor Green }
function Write-Warn { param($Message) Write-Host "[WARN] $Message" -ForegroundColor Yellow }
function Write-Error2 { param($Message) Write-Host "[ERROR] $Message" -ForegroundColor Red }
function Write-Step { param($Message) Write-Host "[STEP] $Message" -ForegroundColor Cyan }

# Detect target triple
function Get-TargetTriple {
    $arch = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "i686" }
    return "$arch-pc-windows-msvc"
}

# Main
Write-Host "=================================="
Write-Host "  Bundle Sidecar for Tauri"
Write-Host "=================================="
Write-Host ""

# Auto-detect target if not specified
if (-not $Target) {
    $Target = Get-TargetTriple
    Write-Info "Auto-detected target: $Target"
}

$SourceBin = Join-Path $SidecarDist "$SidecarName.exe"
$DestBin = Join-Path $TauriBinaries "$SidecarName-$Target.exe"

Write-Host "Target:      $Target"
Write-Host "Source:      $SourceBin"
Write-Host "Destination: $DestBin"
Write-Host ""

# Check source exists
Write-Step "Checking source binary..."
if (-not (Test-Path $SourceBin)) {
    Write-Error2 "Source binary not found: $SourceBin"
    Write-Error2 "Run .\scripts\build-sidecar.ps1 first"
    exit 1
}

$SourceSize = (Get-Item $SourceBin).Length
$SourceSizeMB = [math]::Round($SourceSize / 1MB, 1)
Write-Info "Source binary: ${SourceSizeMB} MB"

# Create destination directory
Write-Step "Creating Tauri binaries directory..."
if (-not (Test-Path $TauriBinaries)) {
    New-Item -ItemType Directory -Path $TauriBinaries -Force | Out-Null
}

# Copy binary
Write-Step "Copying binary..."
Copy-Item -Path $SourceBin -Destination $DestBin -Force

# Verify copy
Write-Step "Verifying..."
if (-not (Test-Path $DestBin)) {
    Write-Error2 "Failed to copy binary"
    exit 1
}

$DestSize = (Get-Item $DestBin).Length
if ($SourceSize -ne $DestSize) {
    Write-Error2 "Size mismatch after copy!"
    exit 1
}

# Quick self-check
Write-Step "Running sidecar self-check..."
try {
    $PingRequest = '{"jsonrpc":"2.0","id":1,"method":"system.ping","params":{}}'
    $Result = $PingRequest | & $DestBin 2>$null | Select-Object -First 1
    if ($Result -match '"protocol":"v1"') {
        Write-Info "Sidecar self-check passed"
    } else {
        Write-Warn "Sidecar responded but protocol check unclear"
    }
} catch {
    Write-Warn "Could not verify sidecar: $_"
}

Write-Host ""
Write-Info "Sidecar bundled successfully!"
Write-Host ""
Write-Host "Bundled binary: $DestBin"
Write-Host ""
Write-Host "Next steps:"
Write-Host "  1. Build Tauri app: cd src-tauri && cargo tauri build"
Write-Host "  2. The sidecar will be included in the app bundle"
Write-Host ""

# List all bundled binaries
if (Test-Path $TauriBinaries) {
    Write-Host "Bundled sidecars:"
    Get-ChildItem $TauriBinaries | Format-Table Name, Length -AutoSize
}
