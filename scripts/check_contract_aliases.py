#!/usr/bin/env python3
"""
Guard compatibility-window aliases from accidental removal.

When shared/contracts/MIGRATION.md marks compat_window_active: true, this script enforces:
1. Required alias mappings are documented.
2. Legacy alias hooks/constants remain present in code.
3. Minimum compatibility timeline language is present.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


DOC_REQUIRED_MAPPINGS = [
    "state_changed` | `state:changed",
    "transcription:complete` | `transcript:complete",
    "transcription:error` | `transcript:error",
    "status:changed` | `sidecar:status",
    "model:status` legacy shape | `model:status` canonical shape",
]

CODE_GUARDS = [
    (Path("src/hooks/useTauriEvents.ts"), "state_changed"),
    (Path("src-tauri/src/integration.rs"), "status:changed"),
    (Path("src-tauri/src/integration.rs"), "transcription:complete"),
    (Path("src-tauri/src/integration.rs"), "transcription:error"),
    (Path("src-tauri/src/integration.rs"), "legacy string model state"),
]

MARKER_RE = re.compile(
    r"<!-- COMPATIBILITY_WINDOW_MARKER_START -->(.*?)<!-- COMPATIBILITY_WINDOW_MARKER_END -->",
    re.DOTALL,
)


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def parse_marker_block(text: str) -> dict[str, str]:
    match = MARKER_RE.search(text)
    if not match:
        raise ValueError("Missing compatibility marker block")

    values: dict[str, str] = {}
    for raw_line in match.group(1).splitlines():
        line = raw_line.strip()
        if not line or ":" not in line:
            continue
        key, value = line.split(":", 1)
        values[key.strip()] = value.strip()
    return values


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    migration_path = repo_root / "shared" / "contracts" / "MIGRATION.md"

    if not migration_path.exists():
        print(f"ERROR: missing migration document: {migration_path}", file=sys.stderr)
        return 1

    migration_text = read_text(migration_path)
    errors: list[str] = []

    try:
        marker = parse_marker_block(migration_text)
    except ValueError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1

    active = marker.get("compat_window_active", "").lower() == "true"

    if "at least one release cycle" not in migration_text:
        errors.append("MIGRATION.md must include minimum timeline language: 'at least one release cycle'")

    for mapping in DOC_REQUIRED_MAPPINGS:
        if mapping not in migration_text:
            errors.append(f"MIGRATION.md missing mapping row containing: {mapping}")

    if active:
        for rel_path, token in CODE_GUARDS:
            file_path = repo_root / rel_path
            if not file_path.exists():
                errors.append(f"Guarded file missing: {rel_path}")
                continue
            content = read_text(file_path)
            if token not in content:
                errors.append(
                    f"Missing guarded token '{token}' in {rel_path}. "
                    "If this is intentional alias retirement, update MIGRATION marker and criteria in the same PR."
                )

    if errors:
        print("Compatibility alias guard failed:", file=sys.stderr)
        for err in errors:
            print(f"  - {err}", file=sys.stderr)
        return 1

    mode = "active" if active else "inactive"
    print(f"✓ Compatibility alias guard passed (window {mode})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
