"""Tests for shared resource resolution across dev and packaged layouts.

Verifies that resolve_shared_path finds resources in:
- Dev repo layout (Path(__file__) traversal)
- PyInstaller onefile (_MEIPASS)
- Executable-relative directory
- macOS Resources bundle
- Environment override (OPENVOICY_SHARED_ROOT)
- Working-directory fallback
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

from openvoicy_sidecar.resources import (
    CONTRACTS_DIR_REL,
    MODEL_CATALOG_REL,
    MODEL_MANIFEST_REL,
    MODEL_MANIFESTS_DIR_REL,
    PRESETS_REL,
    _shared_candidates,
    list_shared_candidates,
    resolve_shared_path,
    resolve_shared_path_optional,
)


# ── Helpers ──────────────────────────────────────────────────────────


def _create_shared_tree(root: Path) -> None:
    """Populate a minimal shared/ directory structure."""
    (root / "replacements").mkdir(parents=True, exist_ok=True)
    (root / "replacements" / "PRESETS.json").write_text("[]")
    (root / "model").mkdir(parents=True, exist_ok=True)
    (root / "model" / "MODEL_MANIFEST.json").write_text(
        json.dumps({"model_id": "test-model"})
    )
    (root / "model" / "MODEL_CATALOG.json").write_text(
        json.dumps({"models": []})
    )
    (root / "model" / "manifests").mkdir(exist_ok=True)
    (root / "contracts").mkdir(exist_ok=True)
    (root / "contracts" / "sidecar.rpc.v1.json").write_text("{}")


# ── Dev-mode resolution ──────────────────────────────────────────────


class TestDevModeResolution:
    """In dev mode, shared/ lives at <repo>/shared/ relative to the module."""

    def test_presets_found_in_dev_layout(self) -> None:
        path = resolve_shared_path(PRESETS_REL)
        assert path.exists()
        assert path.name == "PRESETS.json"

    def test_model_manifest_found(self) -> None:
        path = resolve_shared_path(MODEL_MANIFEST_REL)
        assert path.exists()
        assert path.name == "MODEL_MANIFEST.json"

    def test_model_catalog_found(self) -> None:
        path = resolve_shared_path(MODEL_CATALOG_REL)
        assert path.exists()
        assert path.name == "MODEL_CATALOG.json"

    def test_contracts_dir_found(self) -> None:
        path = resolve_shared_path(CONTRACTS_DIR_REL)
        assert path.exists()
        assert path.is_dir()

    def test_manifests_dir_found(self) -> None:
        path = resolve_shared_path(MODEL_MANIFESTS_DIR_REL)
        assert path.exists()
        assert path.is_dir()


# ── PyInstaller _MEIPASS resolution ──────────────────────────────────


class TestMeipassResolution:
    """Simulated PyInstaller onefile mode via _MEIPASS."""

    def test_meipass_takes_priority(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        meipass_shared = tmp_path / "meipass" / "shared"
        _create_shared_tree(meipass_shared)

        monkeypatch.setattr(sys, "_MEIPASS", str(tmp_path / "meipass"), raising=False)
        # Clear env override to avoid interference
        monkeypatch.delenv("OPENVOICY_SHARED_ROOT", raising=False)

        path = resolve_shared_path(PRESETS_REL)
        assert str(tmp_path / "meipass") in str(path)

    def test_meipass_model_manifest(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        meipass_shared = tmp_path / "meipass" / "shared"
        _create_shared_tree(meipass_shared)

        monkeypatch.setattr(sys, "_MEIPASS", str(tmp_path / "meipass"), raising=False)
        monkeypatch.delenv("OPENVOICY_SHARED_ROOT", raising=False)

        path = resolve_shared_path(MODEL_MANIFEST_REL)
        assert str(tmp_path / "meipass") in str(path)
        data = json.loads(path.read_text())
        assert data["model_id"] == "test-model"

    def test_meipass_contracts_dir(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        meipass_shared = tmp_path / "meipass" / "shared"
        _create_shared_tree(meipass_shared)

        monkeypatch.setattr(sys, "_MEIPASS", str(tmp_path / "meipass"), raising=False)
        monkeypatch.delenv("OPENVOICY_SHARED_ROOT", raising=False)

        path = resolve_shared_path(CONTRACTS_DIR_REL)
        assert path.is_dir()
        assert (path / "sidecar.rpc.v1.json").exists()


# ── Environment override ─────────────────────────────────────────────


class TestEnvOverride:
    """OPENVOICY_SHARED_ROOT takes top priority."""

    def test_env_override_wins(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        custom_shared = tmp_path / "custom_shared"
        _create_shared_tree(custom_shared)

        monkeypatch.setenv("OPENVOICY_SHARED_ROOT", str(custom_shared))

        path = resolve_shared_path(PRESETS_REL)
        assert str(custom_shared) in str(path)


# ── Missing resource ─────────────────────────────────────────────────


class TestMissingResource:
    """Verify FileNotFoundError on unresolvable resources."""

    def test_nonexistent_raises(self) -> None:
        with pytest.raises(FileNotFoundError, match="not found"):
            resolve_shared_path("nonexistent/DOES_NOT_EXIST.json")

    def test_optional_returns_none(self) -> None:
        result = resolve_shared_path_optional("nonexistent/DOES_NOT_EXIST.json")
        assert result is None


# ── Candidate listing ─────────────────────────────────────────────────


class TestCandidateListing:
    """list_shared_candidates returns all candidate paths for diagnostics."""

    def test_returns_multiple_candidates(self) -> None:
        candidates = list_shared_candidates(PRESETS_REL)
        assert len(candidates) >= 3  # dev, exe-relative, cwd at minimum

    def test_candidates_are_path_objects(self) -> None:
        candidates = list_shared_candidates(PRESETS_REL)
        assert all(isinstance(c, Path) for c in candidates)

    def test_all_end_with_relative(self) -> None:
        candidates = list_shared_candidates(PRESETS_REL)
        for c in candidates:
            assert str(c).endswith(PRESETS_REL)


# ── Self-test resource validators ─────────────────────────────────────


class TestSelfTestResourceValidators:
    """Verify self_test resource validators work against dev layout."""

    def test_validate_shared_resources_passes(self) -> None:
        from openvoicy_sidecar.self_test import validate_shared_resources

        # Should not raise in dev mode
        validate_shared_resources()

    def test_validate_presets_loadable_passes(self) -> None:
        from openvoicy_sidecar.self_test import validate_presets_loadable

        validate_presets_loadable()

    def test_validate_model_manifest_loadable_passes(self) -> None:
        from openvoicy_sidecar.self_test import validate_model_manifest_loadable

        validate_model_manifest_loadable()

    def test_validate_model_catalog_loadable_passes(self) -> None:
        from openvoicy_sidecar.self_test import validate_model_catalog_loadable

        validate_model_catalog_loadable()

    def test_validate_presets_loadable_rejects_invalid_json(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        from openvoicy_sidecar.self_test import SelfTestError, validate_presets_loadable

        bad_shared = tmp_path / "shared"
        (bad_shared / "replacements").mkdir(parents=True)
        (bad_shared / "replacements" / "PRESETS.json").write_text("not json{{{")

        monkeypatch.setenv("OPENVOICY_SHARED_ROOT", str(bad_shared))

        with pytest.raises(SelfTestError, match="Failed to parse presets"):
            validate_presets_loadable()

    def test_validate_model_manifest_rejects_missing_model_id(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        from openvoicy_sidecar.self_test import SelfTestError, validate_model_manifest_loadable

        bad_shared = tmp_path / "shared"
        (bad_shared / "model").mkdir(parents=True)
        (bad_shared / "model" / "MODEL_MANIFEST.json").write_text("{}")

        monkeypatch.setenv("OPENVOICY_SHARED_ROOT", str(bad_shared))

        with pytest.raises(SelfTestError, match="model_id"):
            validate_model_manifest_loadable()

    def test_validate_model_catalog_rejects_missing_models_array(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        from openvoicy_sidecar.self_test import SelfTestError, validate_model_catalog_loadable

        bad_shared = tmp_path / "shared"
        (bad_shared / "model").mkdir(parents=True)
        (bad_shared / "model" / "MODEL_CATALOG.json").write_text('{"version": 1}')

        monkeypatch.setenv("OPENVOICY_SHARED_ROOT", str(bad_shared))

        with pytest.raises(SelfTestError, match="models"):
            validate_model_catalog_loadable()


# ── Well-known relative paths ────────────────────────────────────────


class TestWellKnownPaths:
    """Verify the constant relative paths match expected structure."""

    def test_presets_rel(self) -> None:
        assert PRESETS_REL == "replacements/PRESETS.json"

    def test_model_manifest_rel(self) -> None:
        assert MODEL_MANIFEST_REL == "model/MODEL_MANIFEST.json"

    def test_model_catalog_rel(self) -> None:
        assert MODEL_CATALOG_REL == "model/MODEL_CATALOG.json"

    def test_contracts_dir_rel(self) -> None:
        assert CONTRACTS_DIR_REL == "contracts"

    def test_manifests_dir_rel(self) -> None:
        assert MODEL_MANIFESTS_DIR_REL == "model/manifests"
