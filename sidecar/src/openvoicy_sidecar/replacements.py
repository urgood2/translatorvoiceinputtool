"""Text replacements: macros and rule-based transformations.

This module provides macro expansion and replacement rules for
transforming transcribed text.

Pipeline Position:
- Stage 2: Macro expansion ({{date}}, {{time}}, {{datetime}})
- Stage 3: Replacement rules (single pass, no recursion)

Key Features:
- Single-pass replacement (no chaining/recursion)
- Literal and regex rule types
- Word boundary support for literals
- Case sensitivity options
- Preset rule loading
- Validation with limits enforcement
"""

from __future__ import annotations

import datetime
import json
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Literal, Optional

from .protocol import Request, log

# === Constants ===

MAX_RULES = 500
MAX_PATTERN_LENGTH = 256
MAX_REPLACEMENT_LENGTH = 1024
MAX_OUTPUT_LENGTH = 50_000


# === Data Structures ===


@dataclass
class ReplacementRule:
    """A single replacement rule."""

    id: str
    enabled: bool
    kind: Literal["literal", "regex"]
    pattern: str
    replacement: str
    word_boundary: bool = False  # Only applies to literal rules
    case_sensitive: bool = True
    description: Optional[str] = None
    origin: Optional[Literal["user", "preset"]] = None

    def to_dict(self) -> dict[str, Any]:
        """Convert to API response format."""
        result = {
            "id": self.id,
            "enabled": self.enabled,
            "kind": self.kind,
            "pattern": self.pattern,
            "replacement": self.replacement,
            "word_boundary": self.word_boundary,
            "case_sensitive": self.case_sensitive,
        }
        if self.description is not None:
            result["description"] = self.description
        if self.origin is not None:
            result["origin"] = self.origin
        return result

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ReplacementRule:
        """Create from dictionary."""
        return cls(
            id=data.get("id", ""),
            enabled=data.get("enabled", True),
            kind=data.get("kind", "literal"),
            pattern=data.get("pattern", ""),
            replacement=data.get("replacement", ""),
            word_boundary=data.get("word_boundary", False),
            case_sensitive=data.get("case_sensitive", True),
            description=data.get("description"),
            origin=data.get("origin"),
        )


@dataclass
class Preset:
    """A preset collection of rules."""

    id: str
    name: str
    description: str
    rules: list[ReplacementRule] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        """Convert to API response format."""
        return {
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "rule_count": len(self.rules),
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Preset:
        """Create from dictionary."""
        rules = []
        for rule_data in data.get("rules", []):
            rule = ReplacementRule.from_dict(rule_data)
            # Mark as preset origin and prefix ID
            rule.origin = "preset"
            if not rule.id.startswith(data["id"] + ":"):
                rule.id = f"{data['id']}:{rule.id}"
            rules.append(rule)

        return cls(
            id=data["id"],
            name=data["name"],
            description=data.get("description", ""),
            rules=rules,
        )


# === Macro Expansion (Stage 2) ===


def _get_date() -> str:
    """Get current date in ISO format (YYYY-MM-DD)."""
    return datetime.date.today().isoformat()


def _get_time() -> str:
    """Get current time in 24h format (HH:MM)."""
    return datetime.datetime.now().strftime("%H:%M")


def _get_datetime() -> str:
    """Get current date and time (YYYY-MM-DD HH:MM)."""
    return datetime.datetime.now().strftime("%Y-%m-%d %H:%M")


# Macro definitions - case-sensitive
MACROS: dict[str, Any] = {
    "{{date}}": _get_date,
    "{{time}}": _get_time,
    "{{datetime}}": _get_datetime,
}


def expand_macros(text: str) -> str:
    """Expand macros in text.

    Supported macros (case-sensitive):
    - {{date}} → YYYY-MM-DD (local timezone)
    - {{time}} → HH:MM (local timezone, 24h)
    - {{datetime}} → YYYY-MM-DD HH:MM

    Unknown macros pass through unchanged (no error).
    """
    result = text
    for pattern, replacement_fn in MACROS.items():
        if pattern in result:
            result = result.replace(pattern, replacement_fn())
    return result


# === Replacement Rules (Stage 3) ===


class ValidationError(Exception):
    """Raised when rule validation fails."""

    def __init__(self, message: str, details: dict[str, Any] | None = None):
        self.message = message
        self.details = details or {}
        super().__init__(message)


def validate_rules(rules: list[ReplacementRule]) -> None:
    """Validate replacement rules.

    Raises:
        ValidationError: If any validation constraint is violated.
    """
    if len(rules) > MAX_RULES:
        raise ValidationError(
            f"Too many rules: {len(rules)} > {MAX_RULES}",
            {"count": len(rules), "max": MAX_RULES},
        )

    for i, rule in enumerate(rules):
        # Check pattern length
        if len(rule.pattern) > MAX_PATTERN_LENGTH:
            raise ValidationError(
                f"Rule {i} pattern too long: {len(rule.pattern)} > {MAX_PATTERN_LENGTH}",
                {"rule_index": i, "rule_id": rule.id, "length": len(rule.pattern)},
            )

        # Check replacement length
        if len(rule.replacement) > MAX_REPLACEMENT_LENGTH:
            raise ValidationError(
                f"Rule {i} replacement too long: {len(rule.replacement)} > {MAX_REPLACEMENT_LENGTH}",
                {"rule_index": i, "rule_id": rule.id, "length": len(rule.replacement)},
            )

        # Check empty pattern
        if not rule.pattern:
            raise ValidationError(
                f"Rule {i} pattern is empty",
                {"rule_index": i, "rule_id": rule.id},
            )

        # Validate regex patterns
        if rule.kind == "regex":
            try:
                re.compile(rule.pattern)
            except re.error as e:
                raise ValidationError(
                    f"Rule {i} invalid regex: {e}",
                    {"rule_index": i, "rule_id": rule.id, "error": str(e)},
                )


def apply_literal_rule(text: str, rule: ReplacementRule) -> str:
    """Apply a literal replacement rule."""
    if rule.word_boundary:
        # Use regex for word boundary matching
        pattern = r"\b" + re.escape(rule.pattern) + r"\b"
    else:
        pattern = re.escape(rule.pattern)

    flags = 0 if rule.case_sensitive else re.IGNORECASE
    return re.sub(pattern, rule.replacement, text, flags=flags)


def apply_regex_rule(text: str, rule: ReplacementRule) -> str:
    """Apply a regex replacement rule."""
    flags = 0 if rule.case_sensitive else re.IGNORECASE
    try:
        return re.sub(rule.pattern, rule.replacement, text, flags=flags)
    except re.error as e:
        log(f"Regex error in rule {rule.id}: {e}")
        return text  # Return unchanged on error


def apply_single_rule(text: str, rule: ReplacementRule) -> str:
    """Apply a single replacement rule to text."""
    if rule.kind == "literal":
        return apply_literal_rule(text, rule)
    elif rule.kind == "regex":
        return apply_regex_rule(text, rule)
    else:
        log(f"Unknown rule kind: {rule.kind}")
        return text


def apply_replacements(
    text: str, rules: list[ReplacementRule]
) -> tuple[str, bool]:
    """Apply all replacement rules to text.

    Single-pass: if a replacement produces text containing another pattern,
    that pattern is NOT processed. This prevents infinite loops.

    Args:
        text: Input text.
        rules: List of replacement rules to apply.

    Returns:
        Tuple of (result_text, was_truncated).
    """
    result = text

    for rule in rules:
        if not rule.enabled:
            continue
        result = apply_single_rule(result, rule)

    # Check output length
    truncated = False
    if len(result) > MAX_OUTPUT_LENGTH:
        result = result[:MAX_OUTPUT_LENGTH]
        truncated = True
        log(f"Output truncated to {MAX_OUTPUT_LENGTH} chars")

    return result, truncated


# === Full Pipeline ===


def process_text(
    text: str,
    rules: list[ReplacementRule] | None = None,
    skip_normalize: bool = False,
    skip_macros: bool = False,
) -> tuple[str, bool]:
    """Apply the full text processing pipeline.

    Pipeline order (locked for MVP):
    1. Normalize (whitespace, ASR artifacts)
    2. Expand macros ({{date}}, {{time}}, {{datetime}})
    3. Apply replacements (single pass)

    Args:
        text: Input text.
        rules: Replacement rules to apply (default: none).
        skip_normalize: Skip normalization stage.
        skip_macros: Skip macro expansion stage.

    Returns:
        Tuple of (processed_text, was_truncated).
    """
    from .postprocess import normalize

    # Stage 1: Normalize
    if not skip_normalize:
        text = normalize(text)

    # Stage 2: Macro expansion
    if not skip_macros:
        text = expand_macros(text)

    # Stage 3: Replacements
    rules = rules or []
    text, truncated = apply_replacements(text, rules)

    return text, truncated


# === Preset Management ===

_presets: dict[str, Preset] = {}


def load_presets_from_file(path: Path) -> dict[str, Preset]:
    """Load presets from a JSON file.

    Args:
        path: Path to PRESETS.json file.

    Returns:
        Dictionary of preset ID to Preset.
    """
    global _presets

    try:
        with open(path) as f:
            data = json.load(f)

        presets = {}
        for preset_data in data.get("presets", []):
            preset = Preset.from_dict(preset_data)
            presets[preset.id] = preset

        _presets = presets
        log(f"Loaded {len(presets)} presets from {path}")
        return presets

    except FileNotFoundError:
        log(f"Presets file not found: {path}")
        return {}
    except json.JSONDecodeError as e:
        log(f"Error parsing presets file: {e}")
        return {}
    except Exception as e:
        log(f"Error loading presets: {e}")
        return {}


def get_preset(preset_id: str) -> Preset | None:
    """Get a preset by ID."""
    return _presets.get(preset_id)


def get_all_presets() -> list[Preset]:
    """Get all loaded presets."""
    return list(_presets.values())


def get_preset_rules(preset_ids: list[str]) -> list[ReplacementRule]:
    """Get combined rules from multiple presets.

    Args:
        preset_ids: List of preset IDs to include.

    Returns:
        Combined list of rules from all specified presets.
    """
    rules: list[ReplacementRule] = []
    for preset_id in preset_ids:
        preset = _presets.get(preset_id)
        if preset:
            rules.extend(preset.rules)
    return rules


# === Active Rules State ===

_active_rules: list[ReplacementRule] = []


def get_active_rules() -> list[ReplacementRule]:
    """Get the currently active replacement rules."""
    return _active_rules.copy()


def get_current_rules() -> list[ReplacementRule]:
    """Backward-compatible alias for active replacement rules."""
    return get_active_rules()


def set_active_rules(rules: list[ReplacementRule]) -> None:
    """Set the active replacement rules.

    Validates rules before setting.

    Raises:
        ValidationError: If rules fail validation.
    """
    global _active_rules
    validate_rules(rules)
    _active_rules = rules.copy()


# === JSON-RPC Handlers ===


class ReplacementError(Exception):
    """Replacement-specific error."""

    def __init__(self, message: str, code: str = "E_REPLACEMENT"):
        self.message = message
        self.code = code
        super().__init__(message)


def handle_replacements_get_rules(request: Request) -> dict[str, Any]:
    """Handle replacements.get_rules request.

    Returns the currently active replacement rules.
    """
    rules = get_active_rules()
    return {"rules": [r.to_dict() for r in rules]}


def handle_replacements_set_rules(request: Request) -> dict[str, Any]:
    """Handle replacements.set_rules request.

    Params:
        rules: List of replacement rules.

    Returns:
        count: Number of rules set.

    Errors:
        E_INVALID_PARAMS: Rules validation failed.
    """
    rules_data = request.params.get("rules", [])

    rules = [ReplacementRule.from_dict(r) for r in rules_data]

    try:
        set_active_rules(rules)
        return {"count": len(rules)}
    except ValidationError as e:
        raise ReplacementError(e.message, "E_INVALID_PARAMS")


def handle_replacements_get_presets(request: Request) -> dict[str, Any]:
    """Handle replacements.get_presets request.

    Returns available presets (without full rules).
    """
    presets = get_all_presets()
    return {"presets": [p.to_dict() for p in presets]}


def handle_replacements_get_preset_rules(request: Request) -> dict[str, Any]:
    """Handle replacements.get_preset_rules request.

    Params:
        preset_id: ID of preset to get rules for.

    Returns:
        rules: List of rules in the preset.
        preset: Preset metadata.

    Errors:
        E_NOT_FOUND: Preset not found.
    """
    preset_id = request.params.get("preset_id")
    if not preset_id:
        raise ReplacementError("preset_id is required", "E_INVALID_PARAMS")

    preset = get_preset(preset_id)
    if preset is None:
        raise ReplacementError(f"Preset not found: {preset_id}", "E_NOT_FOUND")

    return {
        "preset": preset.to_dict(),
        "rules": [r.to_dict() for r in preset.rules],
    }


def handle_replacements_preview(request: Request) -> dict[str, Any]:
    """Handle replacements.preview request.

    Apply rules to input text without saving.

    Params:
        text: Input text to process.
        rules: Rules to apply (optional, defaults to active rules).
        skip_normalize: Skip normalization stage.
        skip_macros: Skip macro expansion stage.

    Returns:
        result: Processed text.
        truncated: Whether output was truncated.
    """
    text = request.params.get("text", "")
    rules_data = request.params.get("rules")
    skip_normalize = request.params.get("skip_normalize", False)
    skip_macros = request.params.get("skip_macros", False)

    if rules_data is not None:
        rules = [ReplacementRule.from_dict(r) for r in rules_data]
        try:
            validate_rules(rules)
        except ValidationError as e:
            raise ReplacementError(e.message, "E_INVALID_PARAMS")
    else:
        rules = get_active_rules()

    result, truncated = process_text(
        text,
        rules=rules,
        skip_normalize=skip_normalize,
        skip_macros=skip_macros,
    )

    return {
        "result": result,
        "truncated": truncated,
    }
