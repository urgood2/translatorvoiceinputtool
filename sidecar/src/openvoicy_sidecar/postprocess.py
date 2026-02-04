"""Text postprocessing: normalization and cleanup.

This module provides text normalization for ASR output,
cleaning up common artifacts and normalizing whitespace.

Pipeline Position: Stage 1 (before macro expansion and replacements)
"""

from __future__ import annotations

import re


def normalize_whitespace(text: str) -> str:
    """Normalize whitespace in text.

    - Collapse multiple spaces to single space
    - Remove leading/trailing whitespace
    - Normalize various Unicode spaces to regular space
    """
    # Normalize various Unicode whitespace to regular space
    text = re.sub(r"[\u00a0\u2000-\u200a\u202f\u205f\u3000]", " ", text)

    # Collapse multiple whitespace to single space
    text = re.sub(r" +", " ", text)

    # Remove leading/trailing whitespace
    text = text.strip()

    return text


def fix_asr_artifacts(text: str) -> str:
    """Fix common ASR transcription artifacts.

    Handles:
    - Repeated punctuation (... becomes ...)
    - Space before punctuation (word , → word,)
    - Missing space after punctuation (word.word → word. word)
    """
    # Fix space before punctuation
    text = re.sub(r" ([,.!?;:])", r"\1", text)

    # Add space after punctuation if missing (but not for abbreviations like "e.g.")
    # Only for sentence-ending punctuation followed by uppercase
    text = re.sub(r"([.!?])([A-Z])", r"\1 \2", text)

    # Normalize repeated punctuation (... stays as ...)
    text = re.sub(r"\.{4,}", "...", text)
    text = re.sub(r"!{2,}", "!", text)
    text = re.sub(r"\?{2,}", "?", text)

    return text


def normalize(text: str) -> str:
    """Apply all normalization to text.

    This is the main entry point for Stage 1 of the pipeline.

    Pipeline:
    1. Fix ASR artifacts
    2. Normalize whitespace

    Args:
        text: Raw text from ASR or user input.

    Returns:
        Normalized text.
    """
    text = fix_asr_artifacts(text)
    text = normalize_whitespace(text)
    return text
