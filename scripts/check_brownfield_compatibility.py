#!/usr/bin/env python3
"""
Validate brownfield compatibility reference integrity.

This guard ensures shared/BROWNFIELD_COMPATIBILITY.md remains actionable by enforcing:
1. Required module rows are still documented in the impact map.
2. Documented required module paths still exist in the repository.
3. Critical implementation rules remain present.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


REQUIRED_MODULE_PATHS = [
    "src-tauri/src/state.rs",
    "src-tauri/src/config.rs",
    "src-tauri/src/history.rs",
    "src-tauri/src/integration.rs",
    "src-tauri/src/commands.rs",
    "src-tauri/src/watchdog.rs",
    "src-tauri/src/injection.rs",
    "src-tauri/src/tray.rs",
    "src/hooks/useTauriEvents.ts",
    "src/types.ts",
    "shared/ipc/IPC_PROTOCOL_V1.md",
    "shared/schema/AppConfig.schema.json",
    "sidecar/",
]

REQUIRED_RULE_PREFIXES = [
    "1. Do not propose greenfield rewrites when extension/refactor is feasible.",
    "2. Map every planned task to existing files/modules before implementation.",
    "3. Include migration, risk, and testing steps for any runtime behavior change.",
    "4. Every additive IPC change must update both `shared/ipc/IPC_PROTOCOL_V1.md` and `shared/contracts/sidecar.rpc.v1.json` in the same PR.",
    "5. Generated files (`src/types.contracts.ts`, `src-tauri/src/contracts.rs`) are committed read-only artifacts; manual edits belong in wrappers/modules around them.",
]

MODULE_ROW_RE = re.compile(r"^\|\s*`([^`]+)`\s*\|", re.MULTILINE)


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def extract_module_paths(text: str) -> set[str]:
    paths: set[str] = set()
    for match in MODULE_ROW_RE.finditer(text):
        path = match.group(1).strip()
        if path and "/" in path:
            paths.add(path)
    return paths


def validate_brownfield_compatibility(repo_root: Path) -> list[str]:
    errors: list[str] = []
    doc_path = repo_root / "shared" / "BROWNFIELD_COMPATIBILITY.md"

    if not doc_path.exists():
        return [f"Missing brownfield compatibility reference: {doc_path}"]

    text = read_text(doc_path)
    documented_paths = extract_module_paths(text)

    if "planning/PLAN.md" not in text:
        errors.append("BROWNFIELD_COMPATIBILITY.md must reference planning/PLAN.md as source of truth")

    for required in REQUIRED_MODULE_PATHS:
        if required not in documented_paths:
            errors.append(f"Missing required module mapping in brownfield reference: `{required}`")

        target = repo_root / required
        if required.endswith("/"):
            if not target.is_dir():
                errors.append(f"Required mapped directory does not exist: {required}")
        else:
            if not target.is_file():
                errors.append(f"Required mapped file does not exist: {required}")

    for rule in REQUIRED_RULE_PREFIXES:
        if rule not in text:
            errors.append(f"Missing critical implementation rule: {rule}")

    return errors


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    errors = validate_brownfield_compatibility(repo_root)
    if errors:
        print("Brownfield compatibility integrity guard failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1

    print("✓ Brownfield compatibility integrity guard passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
