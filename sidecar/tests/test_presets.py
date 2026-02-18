"""Regression tests for preset loading in dev and packaged paths."""

from __future__ import annotations

import json
import logging
from pathlib import Path

import pytest

import openvoicy_sidecar.server as server_mod
from openvoicy_sidecar.protocol import Request
from openvoicy_sidecar.replacements import (
    ReplacementRule,
    get_all_presets,
    get_preset_rules,
    handle_replacements_get_preset_rules,
    handle_replacements_get_presets,
    merge_preset_and_user_rules,
    process_text,
)
from openvoicy_sidecar.server import load_startup_presets

logger = logging.getLogger(__name__)


@pytest.fixture(autouse=True)
def reset_state(monkeypatch):
    """Reset global preset/rule state for deterministic tests."""
    import openvoicy_sidecar.replacements as rep_module

    original_presets = rep_module._presets.copy()
    original_rules = rep_module._active_rules.copy()
    monkeypatch.delenv("OPENVOICY_PRESETS_PATH", raising=False)
    yield
    rep_module._presets = original_presets
    rep_module._active_rules = original_rules


def _write_presets_file(path: Path) -> Path:
    path.write_text(
        json.dumps(
            {
                "presets": [
                    {
                        "id": "preset-a",
                        "name": "Preset A",
                        "description": "A preset for tests",
                        "rules": [
                            {
                                "id": "a1",
                                "enabled": True,
                                "kind": "literal",
                                "pattern": "foo",
                                "replacement": "bar",
                                "word_boundary": True,
                                "case_sensitive": False,
                            }
                        ],
                    },
                    {
                        "id": "preset-b",
                        "name": "Preset B",
                        "description": "Second preset for tests",
                        "rules": [
                            {
                                "id": "b1",
                                "enabled": True,
                                "kind": "literal",
                                "pattern": "hello",
                                "replacement": "hi",
                                "word_boundary": True,
                                "case_sensitive": False,
                            }
                        ],
                    },
                ]
            }
        )
    )
    return path


def test_presets_load_in_dev_mode():
    """Presets should load from shared/replacements/PRESETS.json in dev mode."""
    load_startup_presets()
    presets = get_all_presets()
    logger.info("Loaded %d preset(s): %s", len(presets), [p.id for p in presets])

    assert len(presets) > 0
    assert any(p.id == "punctuation" for p in presets)


def test_shared_presets_json_has_expected_runtime_structure():
    """Shared PRESETS.json should load into usable preset/rule objects."""
    load_startup_presets()
    presets = get_all_presets()
    preset_map = {preset.id: preset for preset in presets}
    logger.info(
        "loaded preset count=%d preset_ids=%s",
        len(presets),
        sorted(preset_map),
    )

    assert "punctuation" in preset_map
    punctuation = preset_map["punctuation"]
    assert punctuation.name
    assert punctuation.description
    assert len(punctuation.rules) > 0

    first_rule = punctuation.rules[0]
    assert isinstance(first_rule.id, str)
    assert isinstance(first_rule.pattern, str)
    assert isinstance(first_rule.replacement, str)
    assert first_rule.kind in {"literal", "regex"}


def test_presets_load_from_packaged_env_path(tmp_path, monkeypatch):
    """Presets should load from packaged-style path via OPENVOICY_PRESETS_PATH."""
    preset_file = _write_presets_file(tmp_path / "PRESETS.json")
    monkeypatch.setenv("OPENVOICY_PRESETS_PATH", str(preset_file))

    load_startup_presets()
    presets = get_all_presets()
    logger.info("Loaded %d preset(s): %s", len(presets), [p.id for p in presets])

    ids = [preset.id for preset in presets]
    assert ids == ["preset-a", "preset-b"]


def test_each_loaded_preset_has_valid_structure(tmp_path, monkeypatch):
    """Each preset should contain required shape: name, description, rules[]."""
    preset_file = _write_presets_file(tmp_path / "PRESETS.json")
    monkeypatch.setenv("OPENVOICY_PRESETS_PATH", str(preset_file))

    load_startup_presets()
    presets = get_all_presets()

    for preset in presets:
        assert isinstance(preset.name, str)
        assert isinstance(preset.description, str)
        assert isinstance(preset.rules, list)
        for rule in preset.rules:
            assert isinstance(rule.id, str)
            assert isinstance(rule.pattern, str)
            assert isinstance(rule.replacement, str)


def test_replacements_handlers_return_loaded_presets_and_rules(tmp_path, monkeypatch):
    """replacements.get_presets and get_preset_rules should expose loaded data."""
    preset_file = _write_presets_file(tmp_path / "PRESETS.json")
    monkeypatch.setenv("OPENVOICY_PRESETS_PATH", str(preset_file))
    load_startup_presets()

    presets_result = handle_replacements_get_presets(Request(method="replacements.get_presets", id=1))
    assert len(presets_result["presets"]) == 2
    assert {preset["id"] for preset in presets_result["presets"]} == {"preset-a", "preset-b"}

    rules_result = handle_replacements_get_preset_rules(
        Request(
            method="replacements.get_preset_rules",
            id=2,
            params={"preset_id": "preset-a"},
        )
    )
    assert rules_result["preset"]["id"] == "preset-a"
    assert isinstance(rules_result["rules"], list)
    assert len(rules_result["rules"]) == 1
    assert rules_result["rules"][0]["id"] == "preset-a:a1"


def test_preset_rules_are_usable_in_process_text_pipeline(tmp_path, monkeypatch):
    """Loaded preset rules should drive replacement output in the text pipeline."""
    preset_file = _write_presets_file(tmp_path / "PRESETS.json")
    monkeypatch.setenv("OPENVOICY_PRESETS_PATH", str(preset_file))
    load_startup_presets()

    rules = get_preset_rules(["preset-b"])
    processed, truncated = process_text("hello there", rules=rules, skip_normalize=True, skip_macros=True)
    logger.info(
        "pipeline input=%r preset_ids=%s output=%r truncated=%s",
        "hello there",
        ["preset-b"],
        processed,
        truncated,
    )

    assert processed == "hi there"
    assert truncated is False


def test_invalid_or_missing_preset_file_is_graceful(tmp_path, monkeypatch, capsys):
    """Missing/invalid PRESETS.json should not crash and should log attempted paths."""
    missing = tmp_path / "missing.json"
    invalid = tmp_path / "invalid.json"
    invalid.write_text("{ this is not valid json }")

    monkeypatch.setattr(server_mod, "get_startup_preset_candidates", lambda: [missing, invalid])
    load_startup_presets()

    presets = get_all_presets()
    assert presets == []

    stderr_output = capsys.readouterr().err
    logger.info("startup preset logs: %s", stderr_output)
    assert str(missing) in stderr_output
    assert str(invalid) in stderr_output
    assert "Checking preset path:" in stderr_output


def test_preset_and_user_rules_merge_no_duplicates_and_ordering(tmp_path, monkeypatch):
    """Preset rules should merge with user rules without duplicates in stable order."""
    preset_file = _write_presets_file(tmp_path / "PRESETS.json")
    monkeypatch.setenv("OPENVOICY_PRESETS_PATH", str(preset_file))
    load_startup_presets()

    preset_rules = get_preset_rules(["preset-a", "preset-b"])
    user_rules = [
        ReplacementRule(
            id="preset-a:a1",
            enabled=True,
            kind="literal",
            pattern="foo",
            replacement="foo-user-override",
            word_boundary=True,
            case_sensitive=False,
        ),
        ReplacementRule(
            id="user-extra",
            enabled=True,
            kind="literal",
            pattern="tail",
            replacement="tail-user",
            word_boundary=True,
            case_sensitive=False,
        ),
    ]

    merged = merge_preset_and_user_rules(preset_rules, user_rules)
    merged_ids = [rule.id for rule in merged]
    logger.info("Merged rule IDs: %s", merged_ids)

    assert merged_ids == ["preset-a:a1", "preset-b:b1", "user-extra"]
    assert merged[0].replacement == "foo-user-override"
