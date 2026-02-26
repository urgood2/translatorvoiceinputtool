"""Centralized shared-resource resolution for dev and packaged environments.

In dev mode, shared resources live at ``<repo>/shared/``.
In PyInstaller onefile mode they are extracted to ``sys._MEIPASS/shared/``.
In Tauri-bundled mode they may sit next to the executable or under
``<exe>/../Resources/`` (macOS app bundle).

All lookup goes through :func:`resolve_shared_path` which returns the
first matching path from the candidate list, or raises ``FileNotFoundError``.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path
from typing import Optional

_THIS_DIR = Path(__file__).resolve().parent  # openvoicy_sidecar/


def _shared_candidates() -> list[Path]:
    """Return candidate root directories for ``shared/``, ordered by priority."""
    roots: list[Path] = []

    # 1. Explicit override via environment variable
    env_root = os.environ.get("OPENVOICY_SHARED_ROOT")
    if env_root:
        roots.append(Path(env_root).expanduser())

    # 2. PyInstaller onefile extraction directory
    meipass = getattr(sys, "_MEIPASS", None)
    if meipass:
        roots.append(Path(meipass) / "shared")

    # 3. Dev mode: repository layout
    #    __file__ = <repo>/sidecar/src/openvoicy_sidecar/resources.py
    #    parents:  [0] openvoicy_sidecar/  [1] src/  [2] sidecar/  [3] <repo>
    dev_root = _THIS_DIR.parents[2] / "shared"
    roots.append(dev_root)

    # 4. Executable-relative (Tauri bundles sidecar next to resources)
    exe_dir = Path(sys.executable).resolve().parent
    roots.append(exe_dir / "shared")

    # 5. macOS app-bundle Resources directory
    roots.append(exe_dir.parent / "Resources" / "shared")

    # 6. Working-directory fallback
    roots.append(Path.cwd() / "shared")

    return roots


def resolve_shared_path(relative: str) -> Path:
    """Resolve a path relative to ``shared/`` across all candidate layouts.

    Args:
        relative: Path relative to the ``shared/`` directory,
                  e.g. ``"model/MODEL_MANIFEST.json"`` or
                  ``"replacements/PRESETS.json"``.

    Returns:
        The first existing :class:`Path`.

    Raises:
        FileNotFoundError: If no candidate exists.
    """
    tried: list[str] = []
    for root in _shared_candidates():
        candidate = root / relative
        if candidate.exists():
            return candidate
        tried.append(str(candidate))

    raise FileNotFoundError(
        f"Shared resource '{relative}' not found. Searched:\n"
        + "\n".join(f"  - {p}" for p in tried)
    )


def resolve_shared_path_optional(relative: str) -> Optional[Path]:
    """Like :func:`resolve_shared_path` but returns ``None`` on miss."""
    try:
        return resolve_shared_path(relative)
    except FileNotFoundError:
        return None


def list_shared_candidates(relative: str) -> list[Path]:
    """Return all candidate paths for *relative* (some may not exist).

    Useful for diagnostics / self-test reporting.
    """
    return [root / relative for root in _shared_candidates()]


# ── Well-known resource keys ────────────────────────────────────────

PRESETS_REL = "replacements/PRESETS.json"
MODEL_MANIFEST_REL = "model/MODEL_MANIFEST.json"
MODEL_CATALOG_REL = "model/MODEL_CATALOG.json"
CONTRACTS_DIR_REL = "contracts"
MODEL_MANIFESTS_DIR_REL = "model/manifests"
