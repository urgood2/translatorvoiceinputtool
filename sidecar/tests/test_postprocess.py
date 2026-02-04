"""Tests for text postprocessing and normalization."""

from __future__ import annotations

import pytest

from openvoicy_sidecar.postprocess import (
    fix_asr_artifacts,
    normalize,
    normalize_whitespace,
)


class TestNormalizeWhitespace:
    """Tests for whitespace normalization."""

    def test_collapse_multiple_spaces(self):
        """Should collapse multiple spaces to single space."""
        assert normalize_whitespace("Hello    world") == "Hello world"
        assert normalize_whitespace("a  b  c") == "a b c"

    def test_strip_leading_trailing(self):
        """Should remove leading and trailing whitespace."""
        assert normalize_whitespace("  Hello world  ") == "Hello world"
        assert normalize_whitespace("   ") == ""

    def test_normalize_unicode_spaces(self):
        """Should normalize various Unicode spaces."""
        # Non-breaking space
        assert normalize_whitespace("Hello\u00a0world") == "Hello world"
        # En space
        assert normalize_whitespace("Hello\u2002world") == "Hello world"
        # Em space
        assert normalize_whitespace("Hello\u2003world") == "Hello world"

    def test_empty_string(self):
        """Should handle empty string."""
        assert normalize_whitespace("") == ""


class TestFixAsrArtifacts:
    """Tests for ASR artifact correction."""

    def test_space_before_punctuation(self):
        """Should remove space before punctuation."""
        assert fix_asr_artifacts("Hello , world") == "Hello, world"
        assert fix_asr_artifacts("Hello . world") == "Hello. world"
        assert fix_asr_artifacts("What ?") == "What?"
        assert fix_asr_artifacts("Yes !") == "Yes!"

    def test_missing_space_after_punctuation(self):
        """Should add space after sentence-ending punctuation before uppercase."""
        assert fix_asr_artifacts("Hello.World") == "Hello. World"
        assert fix_asr_artifacts("What?Yes") == "What? Yes"
        assert fix_asr_artifacts("Wow!Nice") == "Wow! Nice"

    def test_preserve_abbreviations(self):
        """Should not add space in abbreviations."""
        # These should stay unchanged (lowercase after period)
        assert fix_asr_artifacts("e.g. example") == "e.g. example"
        assert fix_asr_artifacts("i.e. that is") == "i.e. that is"

    def test_normalize_repeated_punctuation(self):
        """Should normalize excessive punctuation."""
        assert fix_asr_artifacts("What....") == "What..."
        assert fix_asr_artifacts("Wow!!!!!") == "Wow!"
        assert fix_asr_artifacts("Really????") == "Really?"

    def test_preserve_valid_ellipsis(self):
        """Should preserve valid ellipsis."""
        assert fix_asr_artifacts("Wait...") == "Wait..."
        assert fix_asr_artifacts("...and then") == "...and then"


class TestNormalize:
    """Tests for the full normalize function."""

    def test_combined_normalization(self):
        """Should apply all normalization steps."""
        # Space before punctuation + multiple spaces + leading/trailing
        result = normalize("  Hello  , world  . ")
        assert result == "Hello, world."

    def test_asr_artifacts_then_whitespace(self):
        """Should fix artifacts before normalizing whitespace."""
        result = normalize("Hello ,  world .  How are you ?")
        assert result == "Hello, world. How are you?"

    def test_preserves_content(self):
        """Should not alter actual content."""
        result = normalize("The quick brown fox")
        assert result == "The quick brown fox"
