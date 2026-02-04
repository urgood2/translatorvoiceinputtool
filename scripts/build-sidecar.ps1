#Requires -Version 5.1
<#
.SYNOPSIS
    Build OpenVoicy sidecar as standalone executable using PyInstaller on Windows.

.DESCRIPTION
    Creates a standalone Windows executable of the OpenVoicy sidecar that includes
    the Python runtime and all dependencies.

.PARAMETER Clean
    Remove build artifacts before building.

.PARAMETER NoVerify
    Skip binary verification step.

.EXAMPLE
    .\scripts\build-sidecar.ps1

.EXAMPLE
    .\scripts\build-sidecar.ps1 -Clean -NoVerify
#>
[CmdletBinding()]
param(
    [switch]$Clean,
    [switch]$NoVerify
)

$ErrorActionPreference = "Stop"

# Paths
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir
$SidecarDir = Join-Path $ProjectRoot "sidecar"
$DistDir = Join-Path $SidecarDir "dist"

# Platform info
$Platform = "windows"
$Arch = if ([Environment]::Is64BitOperatingSystem) { "x64" } else { "x86" }
$PlatformTag = "$Platform-$Arch"
$ArtifactName = "openvoicy-sidecar.exe"

Write-Host "=== Building OpenVoicy Sidecar ===" -ForegroundColor Cyan
Write-Host "Platform: $PlatformTag"
Write-Host "Sidecar dir: $SidecarDir"
Write-Host ""

Set-Location $SidecarDir

# Clean if requested
if ($Clean) {
    Write-Host "Cleaning build artifacts..."
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue build, dist, __pycache__
}

# Ensure virtual environment exists
$VenvPath = Join-Path $SidecarDir ".venv"
if (-not (Test-Path $VenvPath)) {
    Write-Host "Creating virtual environment..."
    python -m venv .venv
}

# Activate venv
$ActivateScript = Join-Path $VenvPath "Scripts\Activate.ps1"
. $ActivateScript

# Install/upgrade dependencies
Write-Host "Installing dependencies..."
pip install --quiet --upgrade pip
pip install --quiet -e .
pip install --quiet pyinstaller

# Run PyInstaller
Write-Host ""
Write-Host "Running PyInstaller..."
$BuildStart = Get-Date
pyinstaller --noconfirm openvoicy_sidecar.spec
$BuildEnd = Get-Date
$BuildTime = [int]($BuildEnd - $BuildStart).TotalSeconds
Write-Host "Build completed in ${BuildTime}s"

# Check binary exists
$BinaryPath = Join-Path $DistDir $ArtifactName
if (-not (Test-Path $BinaryPath)) {
    Write-Error "Binary not found at $BinaryPath"
    exit 1
}

# Get binary size
$BinaryInfo = Get-Item $BinaryPath
$BinarySize = $BinaryInfo.Length
$BinarySizeMB = [math]::Round($BinarySize / 1MB, 2)
Write-Host "Binary size: $BinarySizeMB MB ($BinarySize bytes)"

# Verify binary (unless skipped)
$StartupTimeMs = 0
if (-not $NoVerify) {
    Write-Host ""
    Write-Host "Verifying binary..."

    # Test system.ping
    $VerifyStart = Get-Date
    try {
        $PingResult = '{"jsonrpc":"2.0","id":1,"method":"system.ping"}' | & $BinaryPath 2>$null
        $VerifyEnd = Get-Date
        $StartupTimeMs = [int]($VerifyEnd - $VerifyStart).TotalMilliseconds

        if ($PingResult -match '"result"') {
            Write-Host "✓ system.ping: OK (${StartupTimeMs}ms)" -ForegroundColor Green
        } else {
            Write-Host "✗ system.ping: FAILED" -ForegroundColor Red
            Write-Host "  Response: $PingResult"
            exit 1
        }
    } catch {
        Write-Host "✗ system.ping: FAILED with exception" -ForegroundColor Red
        Write-Host "  Error: $_"
        exit 1
    }

    # Test audio.list_devices
    try {
        $DevicesResult = '{"jsonrpc":"2.0","id":2,"method":"audio.list_devices"}' | & $BinaryPath 2>$null
        if ($DevicesResult -match '"result"') {
            Write-Host "✓ audio.list_devices: OK" -ForegroundColor Green
        } else {
            Write-Host "✗ audio.list_devices: FAILED" -ForegroundColor Red
            Write-Host "  Response: $DevicesResult"
            exit 1
        }
    } catch {
        Write-Host "✗ audio.list_devices: FAILED with exception" -ForegroundColor Red
        exit 1
    }

    Write-Host ""
    Write-Host "Verification passed!" -ForegroundColor Green
}

# Generate manifest
Write-Host ""
Write-Host "Generating manifest..."

try {
    $GitSha = git rev-parse --short HEAD 2>$null
    if (-not $GitSha) { $GitSha = "unknown" }
} catch {
    $GitSha = "unknown"
}

$BuildTimestamp = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
$PythonVersion = (python --version 2>&1).ToString().Split(" ")[1]

# Get version from pyproject.toml
$PyProjectContent = Get-Content (Join-Path $SidecarDir "pyproject.toml") -Raw
if ($PyProjectContent -match 'version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    $Version = "unknown"
}

# Check ONNX
try {
    $OnnxVersion = pip show onnxruntime 2>$null | Select-String "Version" | ForEach-Object { $_.ToString().Split(":")[1].Trim() }
    if (-not $OnnxVersion) { $OnnxVersion = "not-included" }
} catch {
    $OnnxVersion = "not-included"
}

$Manifest = @{
    artifact_name = $ArtifactName
    version = $Version
    platform = $PlatformTag
    python_version = $PythonVersion
    build_timestamp = $BuildTimestamp
    git_sha = $GitSha
    binary_size_bytes = $BinarySize
    startup_time_ms = $StartupTimeMs
    gpu_support = "none"
    onnxruntime_version = $OnnxVersion
    build_time_seconds = $BuildTime
}

$ManifestPath = Join-Path $DistDir "manifest.json"
$Manifest | ConvertTo-Json -Depth 10 | Set-Content $ManifestPath
Write-Host "Manifest written to: $ManifestPath"

# Summary
Write-Host ""
Write-Host "=== Build Summary ===" -ForegroundColor Cyan
Write-Host "Artifact: $BinaryPath"
Write-Host "Size: $BinarySizeMB MB"
if ($StartupTimeMs -gt 0) {
    $StartupSec = [math]::Round($StartupTimeMs / 1000, 2)
    Write-Host "Startup time: ${StartupSec}s"
}
Write-Host "Manifest: $ManifestPath"
Write-Host ""

# Check against targets
Write-Host "=== Target Compliance ===" -ForegroundColor Cyan
if ($BinarySize -lt 524288000) {
    Write-Host "✓ Binary size: $BinarySizeMB MB < 500 MB limit" -ForegroundColor Green
} else {
    Write-Host "✗ Binary size: $BinarySizeMB MB exceeds 500 MB limit" -ForegroundColor Red
}

if ($StartupTimeMs -gt 0 -and $StartupTimeMs -lt 5000) {
    Write-Host "✓ Startup time: ${StartupTimeMs}ms < 5000ms limit" -ForegroundColor Green
} elseif ($StartupTimeMs -gt 0) {
    Write-Host "✗ Startup time: ${StartupTimeMs}ms exceeds 5000ms limit" -ForegroundColor Red
}

Write-Host ""
Write-Host "Build complete!" -ForegroundColor Green
