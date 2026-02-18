"""Tests for text replacements: macros and rule-based transformations."""

from __future__ import annotations

import json
import logging
import re
import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path
from typing import Any
from unittest.mock import patch

import pytest

from openvoicy_sidecar.protocol import Request
from openvoicy_sidecar.replacements import (
    MAX_OUTPUT_LENGTH,
    MAX_PATTERN_LENGTH,
    MAX_REPLACEMENT_LENGTH,
    MAX_RULES,
    Preset,
    ReplacementError,
    ReplacementRule,
    ValidationError,
    apply_literal_rule,
    apply_regex_rule,
    apply_replacements,
    apply_single_rule,
    expand_macros,
    get_active_rules,
    get_all_presets,
    get_current_rules,
    get_preset,
    get_preset_rules,
    handle_replacements_get_presets,
    handle_replacements_get_preset_rules,
    handle_replacements_get_rules,
    handle_replacements_preview,
    handle_replacements_set_rules,
    load_presets_from_file,
    process_text,
    set_active_rules,
    validate_rules,
)

logger = logging.getLogger(__name__)


# === Fixtures ===


@pytest.fixture
def sample_rules() -> list[ReplacementRule]:
    """Sample replacement rules for testing."""
    return [
        ReplacementRule(
            id="1",
            enabled=True,
            kind="literal",
            pattern="BTW",
            replacement="by the way",
            word_boundary=True,
            case_sensitive=False,
        ),
        ReplacementRule(
            id="2",
            enabled=True,
            kind="literal",
            pattern="FYI",
            replacement="for your information",
            word_boundary=True,
            case_sensitive=False,
        ),
    ]


@pytest.fixture
def reset_active_rules():
    """Reset active rules after each test."""
    import openvoicy_sidecar.replacements as rep_module

    original = rep_module._active_rules.copy()
    yield
    rep_module._active_rules = original


@pytest.fixture
def reset_presets():
    """Reset presets after each test."""
    import openvoicy_sidecar.replacements as rep_module

    original = rep_module._presets.copy()
    yield
    rep_module._presets = original


# === Unit Tests: Macro Expansion ===


class TestMacroExpansion:
    """Tests for macro expansion."""

    def test_date_macro(self):
        """Should expand {{date}} to current date."""
        result = expand_macros("Today is {{date}}")
        # Check format
        assert re.match(r"Today is \d{4}-\d{2}-\d{2}$", result)
        # Check it's today
        assert datetime.now().strftime("%Y-%m-%d") in result

    def test_time_macro(self):
        """Should expand {{time}} to current time."""
        result = expand_macros("Time is {{time}}")
        assert re.match(r"Time is \d{2}:\d{2}$", result)

    def test_datetime_macro(self):
        """Should expand {{datetime}} to current date and time."""
        result = expand_macros("Now is {{datetime}}")
        assert re.match(r"Now is \d{4}-\d{2}-\d{2} \d{2}:\d{2}$", result)

    def test_macros_are_case_sensitive(self):
        """Should not expand uppercase macros."""
        result = expand_macros("{{DATE}} vs {{date}}")
        assert "{{DATE}}" in result
        assert "{{DATE}}" in result

    def test_unknown_macro_passes_through(self):
        """Should pass through unknown macros unchanged."""
        result = expand_macros("{{unknown}} macro")
        assert result == "{{unknown}} macro"

    def test_multiple_macros(self):
        """Should expand all recognized macros."""
        result = expand_macros("Date: {{date}}, Time: {{time}}")
        assert "{{date}}" not in result
        assert "{{time}}" not in result

    def test_no_macros(self):
        """Should return text unchanged if no macros."""
        result = expand_macros("Hello world")
        assert result == "Hello world"


# === Unit Tests: Rule Validation ===


class TestRuleValidation:
    """Tests for rule validation."""

    def test_valid_rules_pass(self, sample_rules):
        """Should not raise for valid rules."""
        validate_rules(sample_rules)  # Should not raise

    def test_too_many_rules(self):
        """Should reject more than MAX_RULES."""
        rules = [
            ReplacementRule(
                id=str(i),
                enabled=True,
                kind="literal",
                pattern=f"pattern{i}",
                replacement="replacement",
            )
            for i in range(MAX_RULES + 1)
        ]

        with pytest.raises(ValidationError, match="Too many rules"):
            validate_rules(rules)

    def test_pattern_too_long(self):
        """Should reject patterns exceeding MAX_PATTERN_LENGTH."""
        rules = [
            ReplacementRule(
                id="1",
                enabled=True,
                kind="literal",
                pattern="x" * (MAX_PATTERN_LENGTH + 1),
                replacement="y",
            )
        ]

        with pytest.raises(ValidationError, match="pattern too long"):
            validate_rules(rules)

    def test_replacement_too_long(self):
        """Should reject replacements exceeding MAX_REPLACEMENT_LENGTH."""
        rules = [
            ReplacementRule(
                id="1",
                enabled=True,
                kind="literal",
                pattern="x",
                replacement="y" * (MAX_REPLACEMENT_LENGTH + 1),
            )
        ]

        with pytest.raises(ValidationError, match="replacement too long"):
            validate_rules(rules)

    def test_empty_pattern(self):
        """Should reject empty patterns."""
        rules = [
            ReplacementRule(
                id="1",
                enabled=True,
                kind="literal",
                pattern="",
                replacement="y",
            )
        ]

        with pytest.raises(ValidationError, match="pattern is empty"):
            validate_rules(rules)

    def test_invalid_regex(self):
        """Should reject invalid regex patterns."""
        rules = [
            ReplacementRule(
                id="1",
                enabled=True,
                kind="regex",
                pattern="[invalid",  # Unclosed bracket
                replacement="y",
            )
        ]

        with pytest.raises(ValidationError, match="invalid regex"):
            validate_rules(rules)

    def test_invalid_rule_kind(self):
        """Should reject unsupported rule kinds."""
        rules = [
            ReplacementRule(
                id="1",
                enabled=True,
                kind="invalid",  # type: ignore[arg-type]
                pattern="foo",
                replacement="bar",
            )
        ]

        with pytest.raises(ValidationError, match="invalid kind"):
            validate_rules(rules)


# === Unit Tests: Literal Rules ===


class TestLiteralRules:
    """Tests for literal replacement rules."""

    def test_simple_replacement(self):
        """Should replace literal text."""
        rule = ReplacementRule(
            id="1",
            enabled=True,
            kind="literal",
            pattern="hello",
            replacement="hi",
        )
        result = apply_literal_rule("hello world", rule)
        assert result == "hi world"

    def test_case_insensitive(self):
        """Should handle case-insensitive matching."""
        rule = ReplacementRule(
            id="1",
            enabled=True,
            kind="literal",
            pattern="HELLO",
            replacement="hi",
            case_sensitive=False,
        )
        result = apply_literal_rule("Hello World", rule)
        assert result == "hi World"

    def test_case_sensitive(self):
        """Should respect case-sensitive setting."""
        rule = ReplacementRule(
            id="1",
            enabled=True,
            kind="literal",
            pattern="HELLO",
            replacement="hi",
            case_sensitive=True,
        )
        result = apply_literal_rule("Hello World", rule)
        assert result == "Hello World"  # No match

    def test_word_boundary(self):
        """Should respect word boundary setting."""
        rule = ReplacementRule(
            id="1",
            enabled=True,
            kind="literal",
            pattern="cat",
            replacement="dog",
            word_boundary=True,
        )
        result = apply_literal_rule("the cat sat on the category", rule)
        # Only "cat" as word should match, not "cat" in "category"
        assert result == "the dog sat on the category"

    def test_no_word_boundary(self):
        """Should match partial words without boundary."""
        rule = ReplacementRule(
            id="1",
            enabled=True,
            kind="literal",
            pattern="cat",
            replacement="dog",
            word_boundary=False,
        )
        result = apply_literal_rule("the cat sat on the category", rule)
        assert result == "the dog sat on the dogegory"

    def test_special_regex_characters(self):
        """Should escape special regex characters in literal patterns."""
        rule = ReplacementRule(
            id="1",
            enabled=True,
            kind="literal",
            pattern="[test]",
            replacement="(result)",
        )
        result = apply_literal_rule("This is [test] text", rule)
        assert result == "This is (result) text"


# === Unit Tests: Regex Rules ===


class TestRegexRules:
    """Tests for regex replacement rules."""

    def test_simple_regex(self):
        """Should apply regex pattern."""
        rule = ReplacementRule(
            id="1",
            enabled=True,
            kind="regex",
            pattern=r"\d+",
            replacement="[NUM]",
        )
        result = apply_regex_rule("There are 42 apples", rule)
        assert result == "There are [NUM] apples"

    def test_capture_groups(self):
        """Should support capture group references."""
        rule = ReplacementRule(
            id="1",
            enabled=True,
            kind="regex",
            pattern=r"(\d{3})-(\d{4})",
            replacement=r"(\1) \2",
        )
        result = apply_regex_rule("Call 555-1234", rule)
        assert result == "Call (555) 1234"

    def test_case_insensitive_regex(self):
        """Should support case-insensitive regex."""
        rule = ReplacementRule(
            id="1",
            enabled=True,
            kind="regex",
            pattern=r"hello",
            replacement="hi",
            case_sensitive=False,
        )
        result = apply_regex_rule("HELLO World", rule)
        assert result == "hi World"


# === Unit Tests: Apply Replacements ===


class TestApplyReplacements:
    """Tests for apply_replacements function."""

    def test_multiple_rules_in_order(self):
        """Rules should apply in array order over evolving text."""
        rules = [
            ReplacementRule(id="1", enabled=True, kind="literal", pattern="foo", replacement="bar"),
            ReplacementRule(id="2", enabled=True, kind="literal", pattern="bar", replacement="baz"),
        ]
        input_text = "foo"
        result, _ = apply_replacements(input_text, rules)
        logger.info(
            "input=%s rules=%s output=%s",
            input_text,
            json.dumps([rule.to_dict() for rule in rules]),
            result,
        )
        assert result == "baz", f"rules should apply in order: rules={rules!r}, result={result!r}"

    def test_disabled_rules_skipped(self):
        """Should skip disabled rules."""
        rules = [
            ReplacementRule(id="1", enabled=False, kind="literal", pattern="A", replacement="B"),
            ReplacementRule(id="2", enabled=True, kind="literal", pattern="C", replacement="D"),
        ]
        result, _ = apply_replacements("AC", rules)
        assert result == "AD"  # A unchanged, C -> D

    def test_single_pass_no_recursion(self):
        """Should not recursively apply rules."""
        rules = [
            ReplacementRule(id="1", enabled=True, kind="literal", pattern="A", replacement="AA"),
        ]
        result, _ = apply_replacements("A", rules)
        # Single pass: A -> AA, not A -> AA -> AAAA -> ...
        assert result == "AA"

    def test_output_truncation(self):
        """Should truncate output exceeding MAX_OUTPUT_LENGTH."""
        # Create a rule that produces long output
        rules = [
            ReplacementRule(
                id="1",
                enabled=True,
                kind="literal",
                pattern="X",
                replacement="X" * 10000,
            )
        ]
        text = "X" * 10  # Will expand to 100,000 chars
        result, truncated = apply_replacements(text, rules)

        assert len(result) == MAX_OUTPUT_LENGTH
        assert truncated is True

    def test_no_truncation_for_short_output(self):
        """Should not truncate short output."""
        rules = [
            ReplacementRule(id="1", enabled=True, kind="literal", pattern="A", replacement="B"),
        ]
        result, truncated = apply_replacements("AAA", rules)
        assert result == "BBB"
        assert truncated is False


# === Unit Tests: Full Pipeline ===


class TestProcessText:
    """Tests for full text processing pipeline."""

    def test_pipeline_order(self):
        """Should apply stages in correct order: normalize, macros, replacements."""
        rules = [
            ReplacementRule(
                id="1",
                enabled=True,
                kind="literal",
                pattern=" period",
                replacement=".",
            ),
        ]
        # Input has multiple spaces and spoken punctuation
        result, _ = process_text("Hello   period  world", rules=rules)
        # Should normalize whitespace THEN apply rules
        assert result == "Hello. world"

    def test_skip_normalize(self):
        """Should skip normalization when requested."""
        result, _ = process_text("  Hello   world  ", skip_normalize=True)
        assert result == "  Hello   world  "

    def test_skip_macros(self):
        """Should skip macros when requested."""
        result, _ = process_text("Date: {{date}}", skip_macros=True)
        assert result == "Date: {{date}}"

    def test_replacements_applied_exactly_once(self):
        """A single rule should apply once per pass."""
        rules = [
            ReplacementRule(
                id="btw",
                enabled=True,
                kind="literal",
                pattern="BTW",
                replacement="by the way",
                word_boundary=True,
                case_sensitive=False,
            )
        ]
        input_text = "BTW"
        result, _ = process_text(input_text, rules=rules, skip_normalize=True, skip_macros=True)
        logger.info(
            "input=%s rules=%s output=%s",
            input_text,
            json.dumps([rule.to_dict() for rule in rules]),
            result,
        )
        assert result.count("by the way") == 1, (
            f"expected exactly one replacement; rules={rules!r}, output={result!r}"
        )

    def test_no_rules_pass_through_unchanged(self):
        """No replacement rules should leave text unchanged."""
        input_text = "No rules here"
        result, _ = process_text(input_text, rules=[], skip_normalize=True, skip_macros=True)
        logger.info("input=%s rules=[] output=%s", input_text, result)
        assert result == input_text

    def test_process_text_returns_string_output(self):
        """Tuple return bug guard: processed payload must be a string."""
        rules = [
            ReplacementRule(
                id="foo",
                enabled=True,
                kind="literal",
                pattern="foo",
                replacement="bar",
            )
        ]
        processed, truncated = process_text("foo", rules=rules, skip_normalize=True, skip_macros=True)
        logger.info(
            "input=%s rules=%s output=%s truncated=%s",
            "foo",
            json.dumps([rule.to_dict() for rule in rules]),
            processed,
            truncated,
        )
        assert isinstance(processed, str), f"processed output is not string: {type(processed).__name__}"
        assert not isinstance(processed, tuple), "processed output must not be a tuple"


# === Unit Tests: Preset Loading ===


class TestPresetLoading:
    """Tests for preset loading."""

    def test_load_presets_from_file(self, tmp_path, reset_presets):
        """Should load presets from JSON file."""
        presets_file = tmp_path / "PRESETS.json"
        presets_file.write_text(json.dumps({
            "presets": [
                {
                    "id": "test-preset",
                    "name": "Test Preset",
                    "description": "A test preset",
                    "rules": [
                        {
                            "id": "rule1",
                            "enabled": True,
                            "kind": "literal",
                            "pattern": "foo",
                            "replacement": "bar",
                        }
                    ]
                }
            ]
        }))

        presets = load_presets_from_file(presets_file)

        assert "test-preset" in presets
        assert presets["test-preset"].name == "Test Preset"
        assert len(presets["test-preset"].rules) == 1
        # Rule ID should be prefixed with preset ID
        assert presets["test-preset"].rules[0].id == "test-preset:rule1"
        # Origin should be set to "preset"
        assert presets["test-preset"].rules[0].origin == "preset"

    def test_load_presets_file_not_found(self, tmp_path, reset_presets):
        """Should return empty dict if file not found."""
        presets = load_presets_from_file(tmp_path / "nonexistent.json")
        assert presets == {}

    def test_load_actual_presets_file(self, reset_presets):
        """Should load the actual PRESETS.json file."""
        presets_path = Path(__file__).parent.parent.parent / "shared" / "replacements" / "PRESETS.json"
        if presets_path.exists():
            presets = load_presets_from_file(presets_path)
            assert len(presets) > 0
            # Check punctuation preset exists
            assert "punctuation" in presets

    def test_preset_rules_apply_once_without_chaining(self, reset_presets):
        """Preset-based rules should produce stable output."""
        presets_path = Path(__file__).parent.parent.parent / "shared" / "replacements" / "PRESETS.json"
        presets = load_presets_from_file(presets_path)
        assert "common-abbreviations" in presets

        rules = get_preset_rules(["common-abbreviations"])
        input_text = "BTW"
        result, _ = process_text(input_text, rules=rules, skip_normalize=True, skip_macros=True)
        logger.info(
            "input=%s rules=%s output=%s",
            input_text,
            json.dumps([rule.to_dict() for rule in rules]),
            result,
        )
        assert result == "by the way", f"preset replacement mismatch: rules={rules!r}, output={result!r}"


# === Unit Tests: JSON-RPC Handlers ===


class TestReplacementHandlers:
    """Tests for JSON-RPC handlers."""

    def test_get_rules_empty(self, reset_active_rules):
        """Should return empty rules list."""
        request = Request(method="replacements.get_rules", id=1)
        result = handle_replacements_get_rules(request)
        assert result == {"rules": []}

    def test_set_and_get_rules(self, reset_active_rules):
        """Should set and retrieve rules."""
        set_request = Request(
            method="replacements.set_rules",
            id=1,
            params={
                "rules": [
                    {"id": "1", "enabled": True, "kind": "literal", "pattern": "foo", "replacement": "bar"}
                ]
            },
        )
        result = handle_replacements_set_rules(set_request)
        assert result["count"] == 1

        get_request = Request(method="replacements.get_rules", id=2)
        result = handle_replacements_get_rules(get_request)
        assert len(result["rules"]) == 1
        assert result["rules"][0]["pattern"] == "foo"

    def test_set_rules_validation_error(self, reset_active_rules):
        """Should reject invalid rules."""
        request = Request(
            method="replacements.set_rules",
            id=1,
            params={
                "rules": [
                    {"id": "1", "enabled": True, "kind": "literal", "pattern": "", "replacement": "bar"}
                ]
            },
        )
        with pytest.raises(ReplacementError):
            handle_replacements_set_rules(request)

    def test_set_rules_rejects_invalid_kind(self, reset_active_rules):
        """Should reject unsupported rule kinds."""
        request = Request(
            method="replacements.set_rules",
            id=1,
            params={
                "rules": [
                    {
                        "id": "1",
                        "enabled": True,
                        "kind": "invalid",
                        "pattern": "foo",
                        "replacement": "bar",
                    }
                ]
            },
        )
        with pytest.raises(ReplacementError):
            handle_replacements_set_rules(request)

    def test_preview(self, reset_active_rules):
        """Should preview text processing."""
        request = Request(
            method="replacements.preview",
            id=1,
            params={
                "text": "Hello BTW world",
                "rules": [
                    {"id": "1", "enabled": True, "kind": "literal", "pattern": "BTW", "replacement": "by the way", "word_boundary": True}
                ],
            },
        )
        result = handle_replacements_preview(request)
        assert result["result"] == "Hello by the way world"
        assert result["truncated"] is False

    def test_get_current_rules_alias(self, reset_active_rules):
        """get_current_rules should remain available as alias to active rules."""
        set_request = Request(
            method="replacements.set_rules",
            id=1,
            params={
                "rules": [
                    {"id": "1", "enabled": True, "kind": "literal", "pattern": "foo", "replacement": "bar"}
                ]
            },
        )
        handle_replacements_set_rules(set_request)

        current = get_current_rules()
        assert len(current) == 1
        assert current[0].pattern == "foo"
        assert current[0].replacement == "bar"


# === Shared Test Vectors ===


class TestSharedVectors:
    """Tests using shared test vectors for cross-platform consistency."""

    @pytest.fixture
    def test_vectors(self):
        """Load shared test vectors."""
        # Path from sidecar/tests to shared/replacements
        vectors_path = Path(__file__).parent.parent.parent / "shared" / "replacements" / "TEST_VECTORS.json"
        with open(vectors_path) as f:
            return json.load(f)

    def test_shared_vectors(self, test_vectors, reset_active_rules):
        """Should produce expected results for all shared test vectors."""
        for case in test_vectors["test_cases"]:
            name = case["name"]
            input_text = case["input"]
            rules_data = case.get("rules", [])
            expected = case.get("expected")
            expected_pattern = case.get("expected_pattern")

            # Build rules
            rules = [ReplacementRule.from_dict(r) for r in rules_data]

            # Process text
            result, _ = process_text(input_text, rules=rules)

            # Check result
            if expected is not None:
                assert result == expected, f"Test '{name}' failed: expected '{expected}', got '{result}'"
            elif expected_pattern is not None:
                assert re.match(expected_pattern, result), f"Test '{name}' failed: result '{result}' doesn't match pattern '{expected_pattern}'"


# === Performance Tests ===


class TestPerformance:
    """Performance tests for replacement rules."""

    def test_500_rules_scales_without_extreme_regression(self, reset_active_rules):
        """500-rule throughput should remain in line with smaller rule sets.

        This avoids a hard wall-clock threshold that flakes on slower CI runners
        while still catching major algorithmic regressions.
        """

        def make_rules(count: int) -> list[ReplacementRule]:
            return [
                ReplacementRule(
                    id=str(i),
                    enabled=True,
                    kind="literal",
                    pattern=f"word{i}",
                    replacement=f"replaced{i}",
                    word_boundary=True,
                )
                for i in range(count)
            ]

        # Shared input with a few matches.
        text = "The word0 and word100 and word499 are replaced."

        # Warmup to reduce one-time regex/cache effects.
        apply_replacements(text, make_rules(10))

        small_rules = make_rules(50)
        large_rules = make_rules(500)

        small_start = time.monotonic()
        _, _ = apply_replacements(text, small_rules)
        small_elapsed_ms = (time.monotonic() - small_start) * 1000

        large_start = time.monotonic()
        result, _ = apply_replacements(text, large_rules)
        large_elapsed_ms = (time.monotonic() - large_start) * 1000

        # Verify correctness.
        assert "replaced0" in result
        assert "replaced100" in result
        assert "replaced499" in result

        small_per_rule_ms = small_elapsed_ms / len(small_rules)
        large_per_rule_ms = large_elapsed_ms / len(large_rules)

        # Allow headroom for CI noise while guarding against extreme slowdowns.
        assert large_per_rule_ms <= (small_per_rule_ms * 4) + 0.1, (
            "Unexpected scaling regression: "
            f"50-rule={small_per_rule_ms:.4f}ms/rule, "
            f"500-rule={large_per_rule_ms:.4f}ms/rule"
        )


# === Integration Tests ===


class TestReplacementIntegration:
    """Integration tests with sidecar process."""

    @pytest.fixture
    def sidecar_process(self):
        """Start a sidecar process for integration testing."""
        proc = subprocess.Popen(
            [sys.executable, "-m", "openvoicy_sidecar"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        yield proc
        if proc.poll() is None:
            proc.terminate()
            proc.wait(timeout=5)

    def _send_request(
        self, proc, method: str, params: dict[str, Any] | None = None, req_id: int = 1
    ) -> dict[str, Any]:
        """Send a JSON-RPC request and get the response."""
        request: dict[str, Any] = {"jsonrpc": "2.0", "id": req_id, "method": method}
        if params:
            request["params"] = params

        proc.stdin.write(json.dumps(request) + "\n")
        proc.stdin.flush()

        response_line = proc.stdout.readline()
        return json.loads(response_line)

    def test_get_rules_integration(self, sidecar_process):
        """Integration test: get_rules should work."""
        response = self._send_request(sidecar_process, "replacements.get_rules")

        assert response.get("jsonrpc") == "2.0"
        assert "result" in response
        assert "rules" in response["result"]

    def test_set_rules_integration(self, sidecar_process):
        """Integration test: set_rules should work."""
        response = self._send_request(
            sidecar_process,
            "replacements.set_rules",
            {
                "rules": [
                    {"id": "1", "enabled": True, "kind": "literal", "pattern": "foo", "replacement": "bar"}
                ]
            },
        )

        assert response.get("jsonrpc") == "2.0"
        assert "result" in response
        assert response["result"]["count"] == 1

    def test_preview_integration(self, sidecar_process):
        """Integration test: preview should work."""
        response = self._send_request(
            sidecar_process,
            "replacements.preview",
            {
                "text": "Hello foo world",
                "rules": [
                    {"id": "1", "enabled": True, "kind": "literal", "pattern": "foo", "replacement": "bar"}
                ],
            },
        )

        assert response.get("jsonrpc") == "2.0"
        assert "result" in response
        assert response["result"]["result"] == "Hello bar world"
