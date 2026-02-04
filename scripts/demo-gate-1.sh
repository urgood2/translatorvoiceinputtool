#!/bin/bash
# scripts/demo-gate-1.sh
# Gate 1: IPC Contract Locked
#
# Verifies that the IPC contract is complete and valid.
# This gate must pass before M1 and M2 work can proceed.

set -e

echo "=== Gate 1: IPC Contract Verification ==="
echo ""

# Check protocol document exists
echo -n "Checking IPC_PROTOCOL_V1.md exists... "
if [ -f shared/ipc/IPC_PROTOCOL_V1.md ]; then
    echo "PASS"
else
    echo "FAIL: Protocol doc missing"
    exit 1
fi

# Check examples file exists
echo -n "Checking IPC_V1_EXAMPLES.jsonl exists... "
if [ -f shared/ipc/examples/IPC_V1_EXAMPLES.jsonl ]; then
    echo "PASS"
else
    echo "FAIL: Examples file missing"
    exit 1
fi

# Validate examples parse as JSON
echo -n "Validating examples parse as valid JSON... "
python3 -c "
import json
import sys

with open('shared/ipc/examples/IPC_V1_EXAMPLES.jsonl') as f:
    for i, line in enumerate(f, 1):
        line = line.strip()
        if not line or line.startswith('//'):
            continue
        try:
            json.loads(line)
        except json.JSONDecodeError as e:
            print(f'FAIL: Line {i} invalid JSON: {e}', file=sys.stderr)
            sys.exit(1)
print('PASS')
"

# Check protocol document has required sections
echo -n "Checking protocol has required sections... "
required_sections=(
    "Methods"
    "Notifications"
    "Error Codes"
)

for section in "${required_sections[@]}"; do
    if ! grep -q "$section" shared/ipc/IPC_PROTOCOL_V1.md; then
        echo "FAIL: Missing section '$section'"
        exit 1
    fi
done
echo "PASS"

# Check protocol defines core methods
echo -n "Checking core methods are defined... "
core_methods=(
    "system.ping"
    "system.info"
    "audio.list_devices"
    "asr.initialize"
    "recording.start"
    "recording.stop"
)

for method in "${core_methods[@]}"; do
    if ! grep -q "$method" shared/ipc/IPC_PROTOCOL_V1.md; then
        echo "FAIL: Missing method '$method'"
        exit 1
    fi
done
echo "PASS"

echo ""
echo "=== Gate 1: PASSED ==="
echo ""
echo "The IPC contract is complete and valid."
echo "M1 and M2 work can now proceed in parallel."
