#!/usr/bin/env python3
"""
Generate/check derived contract fixture corpora from the canonical IPC examples.

Canonical source of truth:
  shared/ipc/examples/IPC_V1_EXAMPLES.jsonl

Derived artifact (optional, generated-only):
  shared/contracts/examples/IPC_V1_EXAMPLES.jsonl
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
CANONICAL_REL = Path("shared/ipc/examples/IPC_V1_EXAMPLES.jsonl")
DERIVED_DIR_REL = Path("shared/contracts/examples")
DERIVED_FILE_REL = DERIVED_DIR_REL / "IPC_V1_EXAMPLES.jsonl"


def normalize_text(text: str) -> str:
    return text if text.endswith("\n") else f"{text}\n"


def generate(repo_root: Path) -> int:
    canonical = repo_root / CANONICAL_REL
    derived = repo_root / DERIVED_FILE_REL

    if not canonical.exists():
        print(f"ERROR: missing canonical fixture corpus: {CANONICAL_REL}", file=sys.stderr)
        return 1

    derived.parent.mkdir(parents=True, exist_ok=True)
    derived.write_text(normalize_text(canonical.read_text(encoding="utf-8")), encoding="utf-8")
    print(
        "[gen_contract_examples] generated "
        f"{DERIVED_FILE_REL} from {CANONICAL_REL}"
    )
    return 0


def check(repo_root: Path) -> int:
    canonical = repo_root / CANONICAL_REL
    derived_dir = repo_root / DERIVED_DIR_REL
    derived = repo_root / DERIVED_FILE_REL

    if not canonical.exists():
        print(f"ERROR: missing canonical fixture corpus: {CANONICAL_REL}", file=sys.stderr)
        return 1

    if not derived_dir.exists():
        print(
            "[gen_contract_examples] shared/contracts/examples is absent; "
            "no derived fixture corpus to verify"
        )
        return 0

    derived_files = sorted(path for path in derived_dir.glob("*.jsonl") if path.is_file())
    if not derived_files:
        print(
            "[gen_contract_examples] shared/contracts/examples exists but has no *.jsonl; "
            "no derived fixture corpus to verify"
        )
        return 0

    errors: list[str] = []
    expected_name = DERIVED_FILE_REL.name
    for path in derived_files:
        if path.name != expected_name:
            rel = path.relative_to(repo_root)
            errors.append(
                f"unexpected derived fixture file {rel}; "
                f"only {DERIVED_FILE_REL} is supported"
            )

    if not derived.exists():
        errors.append(f"missing expected derived fixture file: {DERIVED_FILE_REL}")
    else:
        canonical_text = normalize_text(canonical.read_text(encoding="utf-8"))
        derived_text = normalize_text(derived.read_text(encoding="utf-8"))
        if canonical_text != derived_text:
            errors.append(
                f"{DERIVED_FILE_REL} is out of date with {CANONICAL_REL}; "
                "run scripts/gen_contract_examples.py to regenerate"
            )

    if errors:
        for err in errors:
            print(f"ERROR: {err}", file=sys.stderr)
        return 1

    print(
        "[gen_contract_examples] verified derived fixture corpus "
        f"{DERIVED_FILE_REL} matches canonical {CANONICAL_REL}"
    )
    return 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=REPO_ROOT, help="Repository root path")
    parser.add_argument(
        "--check",
        action="store_true",
        help="Verify derived fixture corpus (if present) matches canonical source",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    repo_root = args.repo_root.resolve()
    return check(repo_root) if args.check else generate(repo_root)


if __name__ == "__main__":
    raise SystemExit(main())
