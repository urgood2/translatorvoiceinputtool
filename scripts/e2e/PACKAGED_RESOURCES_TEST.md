# Packaged Resources Verification Test

## Overview

The `test-packaged-resources.sh` script verifies that sidecar resource resolution works correctly in packaged application contexts. This ensures that when the app is distributed/packaged, the sidecar can still locate required shared resources.

## What It Tests

### Phase 1: Static Resource Resolution
- ✅ **Replacement presets** (`PRESETS.json`)
- ✅ **Model manifest** (`MODEL_MANIFEST.json`)
- ✅ **Model catalog** (`MODEL_CATALOG.json`)
- ✅ **Contracts directory** (optional)
- ✅ **Model manifests directory** (optional)

### Phase 2: Live Sidecar Process Validation
- ✅ **system.ping** - Basic connectivity
- ✅ **system.info** - Runtime capabilities
- ✅ **status.get** - Sidecar status
- ✅ **replacements.get_rules** - Replacement functionality

## How It Works

The test simulates a packaged environment by:

1. **Creating temporary staging area** - Mimics packaged app structure
2. **Copying shared resources** - Stages all required files
3. **Setting environment override** - Uses `OPENVOICY_SHARED_ROOT`
4. **Running self-test** - Executes `python -m openvoicy_sidecar.self_test`
5. **Validating results** - Ensures all phases pass

## Usage

Run individually:
```bash
./scripts/e2e/test-packaged-resources.sh
```

Run as part of e2e suite:
```bash
./scripts/e2e/run-all.sh --filter packaged
```

## Resource Resolution Priority

The sidecar searches for shared resources in this order:

1. **Explicit override**: `OPENVOICY_SHARED_ROOT` env var
2. **PyInstaller onefile**: `sys._MEIPASS/shared`
3. **Dev mode**: `<repo>/shared`
4. **Executable-relative**: `<exe-dir>/shared`
5. **macOS app bundle**: `<exe-dir>/../Resources/shared`
6. **Working directory**: `./shared`

## Expected Output

When successful, the test shows:
```
[SELF_TEST] Testing shared resource resolution... OK
[SELF_TEST] Testing presets loadable... OK
[SELF_TEST] Testing model manifest loadable... OK
[SELF_TEST] Testing model catalog loadable... OK
[SELF_TEST] Testing system.ping... OK
[SELF_TEST] Testing system.info... OK
[SELF_TEST] Testing status.get... OK
[SELF_TEST] Testing replacements.get_rules... OK
[SELF_TEST] All checks passed
```

This confirms that packaged applications can successfully locate and use all required sidecar resources.