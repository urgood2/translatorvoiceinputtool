"""Regression tests for process_text idempotency and single-pass behavior."""

from __future__ import annotations

import logging
from pathlib import Path

import pytest

import openvoicy_sidecar.replacements as replacements_module
from openvoicy_sidecar.replacements import (
    ReplacementRule,
    get_preset_rules,
    load_presets_from_file,
    process_text,
)

logger = logging.getLogger(__name__)


@pytest.fixture
def fixed_macros(monkeypatch: pytest.MonkeyPatch) -> None:
    """Freeze macro outputs so idempotency assertions are deterministic."""
    monkeypatch.setattr(
        replacements_module,
        "MACROS",
        {
            "{{date}}": lambda: "2026-02-18",
            "{{time}}": lambda: "16:00",
            "{{datetime}}": lambda: "2026-02-18 16:00",
        },
    )


def _assert_processed_text_case(
    *,
    input_text: str,
    rules: list[ReplacementRule],
    expected_text: str,
    skip_normalize: bool = True,
    skip_macros: bool = True,
) -> None:
    """Assert that processed text remains a plain string value."""
    raw_return = process_text(
        input_text,
        rules=rules,
        skip_normalize=skip_normalize,
        skip_macros=skip_macros,
    )
    logger.info(
        "case input=%r rules=%r expected=%r raw_return=%r raw_type=%s",
        input_text,
        [rule.to_dict() for rule in rules],
        expected_text,
        raw_return,
        type(raw_return).__name__,
    )
    processed_text, truncated = raw_return
    logger.info(
        "result actual=%r truncated=%s processed_type=%s",
        processed_text,
        truncated,
        type(processed_text).__name__,
    )
    assert type(processed_text) is str, (
        f"processed text must be str; raw return={raw_return!r}"
    )
    assert not isinstance(processed_text, tuple), (
        f"processed text must not be tuple; raw return={raw_return!r}"
    )
    assert processed_text == expected_text, (
        f"unexpected processed text; raw return={raw_return!r}"
    )


def test_process_text_string_not_tuple_identity_case() -> None:
    _assert_processed_text_case(
        input_text="hello world",
        rules=[],
        expected_text="hello world",
    )


def test_process_text_string_not_tuple_single_replacement() -> None:
    _assert_processed_text_case(
        input_text="hello world",
        rules=[
            ReplacementRule(
                id="hello-hi",
                enabled=True,
                kind="literal",
                pattern="hello",
                replacement="hi",
                case_sensitive=False,
            )
        ],
        expected_text="hi world",
    )


def test_process_text_string_type_is_str() -> None:
    raw_return = process_text("hello world", rules=[], skip_normalize=True, skip_macros=True)
    logger.info("type-check raw_return=%r raw_type=%s", raw_return, type(raw_return).__name__)
    processed_text, _ = raw_return
    assert type(processed_text) is str, f"type check failed; raw return={raw_return!r}"
    assert not isinstance(processed_text, tuple), f"must not be tuple; raw return={raw_return!r}"


def test_process_text_string_not_tuple_multiple_rules() -> None:
    _assert_processed_text_case(
        input_text="hello world",
        rules=[
            ReplacementRule(
                id="hello-hi",
                enabled=True,
                kind="literal",
                pattern="hello",
                replacement="hi",
                case_sensitive=False,
            ),
            ReplacementRule(
                id="world-earth",
                enabled=True,
                kind="literal",
                pattern="world",
                replacement="earth",
                case_sensitive=False,
            ),
        ],
        expected_text="hi earth",
    )


def test_process_text_string_not_tuple_with_preset_rules() -> None:
    presets_path = Path(__file__).parent.parent.parent / "shared" / "replacements" / "PRESETS.json"
    presets = load_presets_from_file(presets_path)
    assert "common-abbreviations" in presets
    rules = get_preset_rules(["common-abbreviations"])
    _assert_processed_text_case(
        input_text="BTW",
        rules=rules,
        expected_text="by the way",
    )


def test_process_text_string_not_tuple_empty_input() -> None:
    _assert_processed_text_case(
        input_text="",
        rules=[],
        expected_text="",
        skip_normalize=False,
        skip_macros=False,
    )


def test_process_text_string_not_tuple_no_matches() -> None:
    _assert_processed_text_case(
        input_text="hello world",
        rules=[
            ReplacementRule(
                id="absent",
                enabled=True,
                kind="literal",
                pattern="goodbye",
                replacement="bye",
            )
        ],
        expected_text="hello world",
    )


def test_process_text_idempotent_literal_rules() -> None:
    rules = [
        ReplacementRule(
            id="literal-1",
            enabled=True,
            kind="literal",
            pattern="hello",
            replacement="world",
            case_sensitive=False,
        )
    ]
    once, _ = process_text("hello there", rules=rules, skip_normalize=True, skip_macros=True)
    twice, _ = process_text(once, rules=rules, skip_normalize=True, skip_macros=True)
    logger.info("idempotency literal: once=%r twice=%r", once, twice)
    assert once == "world there"
    assert twice == once


def test_process_text_regex_idempotent() -> None:
    rules = [
        ReplacementRule(
            id="regex-1",
            enabled=True,
            kind="regex",
            pattern=r"\b(\d{2})/(\d{2})/(\d{4})\b",
            replacement=r"\3-\1-\2",
        )
    ]
    once, _ = process_text("Date: 02/18/2026", rules=rules, skip_normalize=True, skip_macros=True)
    twice, _ = process_text(once, rules=rules, skip_normalize=True, skip_macros=True)
    assert once == "Date: 2026-02-18"
    assert twice == once


def test_process_text_macro_idempotent(fixed_macros: None) -> None:
    once, _ = process_text("Today {{date}}")
    twice, _ = process_text(once)
    assert once == "Today 2026-02-18"
    assert twice == once


def test_process_text_applies_once_without_rule_chaining() -> None:
    rules = [
        ReplacementRule(id="r1", enabled=True, kind="literal", pattern="hello", replacement="world"),
        ReplacementRule(id="r2", enabled=True, kind="literal", pattern="world", replacement="planet"),
    ]
    result, _ = process_text("hello", rules=rules, skip_normalize=True, skip_macros=True)
    assert result == "world"


def test_process_text_with_presets_is_idempotent() -> None:
    presets_path = Path(__file__).parent.parent.parent / "shared" / "replacements" / "PRESETS.json"
    presets = load_presets_from_file(presets_path)
    assert "common-abbreviations" in presets

    rules = get_preset_rules(["common-abbreviations"])
    once, _ = process_text("BTW", rules=rules, skip_normalize=True, skip_macros=True)
    twice, _ = process_text(once, rules=rules, skip_normalize=True, skip_macros=True)
    assert once == "by the way"
    assert twice == once


def test_process_text_ordering_normalize_macros_replacements(fixed_macros: None) -> None:
    rules = [
        ReplacementRule(
            id="date-token",
            enabled=True,
            kind="literal",
            pattern="2026-02-18",
            replacement="[DATE]",
        )
    ]
    result, _ = process_text("  Meeting on {{date}}  ", rules=rules)
    assert result == "Meeting on [DATE]"


def test_process_text_empty() -> None:
    result, truncated = process_text("")
    assert result == ""
    assert truncated is False


def test_process_text_no_rules_unchanged() -> None:
    result, truncated = process_text("No rules here", rules=[], skip_normalize=True, skip_macros=True)
    assert result == "No rules here"
    assert truncated is False


def test_process_text_unicode() -> None:
    rules = [
        ReplacementRule(id="jp", enabled=True, kind="literal", pattern="こんにちは", replacement="こんばんは")
    ]
    result, _ = process_text("こんにちは 世界", rules=rules, skip_normalize=True, skip_macros=True)
    assert result == "こんばんは 世界"


def test_process_text_overlapping_rules_single_pass() -> None:
    rules = [
        ReplacementRule(id="ov-1", enabled=True, kind="literal", pattern="abc", replacement="x"),
        ReplacementRule(id="ov-2", enabled=True, kind="literal", pattern="bc", replacement="y"),
    ]
    result, _ = process_text("abc", rules=rules, skip_normalize=True, skip_macros=True)
    assert result == "x"
