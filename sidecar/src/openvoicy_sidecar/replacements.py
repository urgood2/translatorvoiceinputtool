"""Text replacements: macros and rule-based transformations.

This module provides macro expansion and replacement rules for
transforming transcribed text.

Pipeline Position:
- Stage 2: Macro expansion ({{date}}, {{time}}, {{datetime}})
- Stage 3: Replacement rules (single pass, in-order)

Key Features:
- Single-pass replacement (rules run once, in array order)
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
ALLOWED_RULE_KINDS = {"literal", "regex"}


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
        # Validate rule kind
        if rule.kind not in ALLOWED_RULE_KINDS:
            raise ValidationError(
                f"Rule {i} has invalid kind: {rule.kind}",
                {"rule_index": i, "rule_id": rule.id, "kind": rule.kind},
            )

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

    Single-pass, in-order: each enabled rule runs once over the current text.
    Later rules can operate on output created by earlier rules, but there is
    no recursive re-entry into previously-applied rules.

    Args:
        text: Input text.
        rules: List of replacement rules to apply.

    Returns:
        Tuple of (result_text, was_truncated).
    """
    result, truncated, _ = apply_replacements_with_stats(text, rules)
    return result, truncated


def apply_replacements_with_stats(
    text: str, rules: list[ReplacementRule]
) -> tuple[str, bool, int]:
    """Apply all replacement rules and return applied rule count.

    Returns:
        Tuple of (result_text, was_truncated, applied_rules_count).
    """
    result, truncated, applied_rules_count, _ = apply_replacements_with_full_stats(text, rules)
    return result, truncated, applied_rules_count


def apply_replacements_with_full_stats(
    text: str, rules: list[ReplacementRule]
) -> tuple[str, bool, int, list[str]]:
    """Apply all replacement rules and return full statistics.

    Returns:
        Tuple of (result_text, was_truncated, applied_rules_count, applied_presets).
    """
    result = text
    applied_rules_count = 0
    applied_preset_ids = set()

    for rule in rules:
        if not rule.enabled:
            continue

        next_result = apply_single_rule(result, rule)
        if next_result != result:
            applied_rules_count += 1
            # Track preset if this rule came from a preset
            if rule.origin == "preset" and ":" in rule.id:
                preset_id = rule.id.split(":", 1)[0]
                applied_preset_ids.add(preset_id)
        result = next_result

    # Check output length
    truncated = False
    if len(result) > MAX_OUTPUT_LENGTH:
        result = result[:MAX_OUTPUT_LENGTH]
        truncated = True
        log(f"Output truncated to {MAX_OUTPUT_LENGTH} chars")

    return result, truncated, applied_rules_count, sorted(applied_preset_ids)


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
    3. Apply replacements (single pass, in-order)

    Args:
        text: Input text.
        rules: Replacement rules to apply (default: none).
        skip_normalize: Skip normalization stage.
        skip_macros: Skip macro expansion stage.

    Returns:
        Tuple of (processed_text, was_truncated).
    """
    from .postprocess import normalize

    processed, truncated, _ = process_text_with_stats(
        text,
        rules=rules,
        skip_normalize=skip_normalize,
        skip_macros=skip_macros,
    )
    return processed, truncated


def process_text_with_stats(
    text: str,
    rules: list[ReplacementRule] | None = None,
    skip_normalize: bool = False,
    skip_macros: bool = False,
) -> tuple[str, bool, int]:
    """Apply the full pipeline and report applied replacement rule count.

    Returns:
        Tuple of (processed_text, was_truncated, applied_rules_count).
    """
    processed, truncated, applied_rules_count, _ = process_text_with_full_stats(
        text, rules=rules, skip_normalize=skip_normalize, skip_macros=skip_macros
    )
    return processed, truncated, applied_rules_count


def process_text_with_full_stats(
    text: str,
    rules: list[ReplacementRule] | None = None,
    skip_normalize: bool = False,
    skip_macros: bool = False,
) -> tuple[str, bool, int, list[str]]:
    """Apply the full pipeline and report full replacement statistics.

    Returns:
        Tuple of (processed_text, was_truncated, applied_rules_count, applied_presets).
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
    text, truncated, applied_rules_count, applied_presets = apply_replacements_with_full_stats(text, rules)

    return text, truncated, applied_rules_count, applied_presets


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
        _presets = {}
        return {}
    except json.JSONDecodeError as e:
        log(f"Error parsing presets file: {e}")
        _presets = {}
        return {}
    except Exception as e:
        log(f"Error loading presets: {e}")
        _presets = {}
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


def merge_preset_and_user_rules(
    preset_rules: list[ReplacementRule], user_rules: list[ReplacementRule]
) -> list[ReplacementRule]:
    """Merge preset and user rules with stable ordering and no duplicate IDs.

    Behavior:
    - Preset order is preserved
    - User rules are applied in order
    - If a user rule ID matches a preset rule ID, the user rule overrides in place
    - New user rule IDs are appended
    """
    merged = list(preset_rules)
    index_by_id = {rule.id: idx for idx, rule in enumerate(merged)}

    for user_rule in user_rules:
        existing_index = index_by_id.get(user_rule.id)
        if existing_index is not None:
            merged[existing_index] = user_rule
            continue

        index_by_id[user_rule.id] = len(merged)
        merged.append(user_rule)

    return merged


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

    Apply rules to input text without saving. Uses the EXACT SAME pipeline as
    transcription to ensure preview/apply parity.

    Params:
        text: Input text to process.
        rules: Rules to apply (optional, defaults to active rules).
        skip_normalize: Skip normalization stage.
        skip_macros: Skip macro expansion stage.

    Returns:
        result: Processed text.
        truncated: Whether output was truncated.
        applied_rules_count: Number of rules that made changes.
        applied_presets: List of preset IDs that contributed rules.
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

    # Use the same pipeline as transcription for perfect parity
    result, truncated, applied_rules_count, applied_presets = process_text_with_full_stats(
        text,
        rules=rules,
        skip_normalize=skip_normalize,
        skip_macros=skip_macros,
    )

    return {
        "result": result,
        "truncated": truncated,
        "applied_rules_count": applied_rules_count,
        "applied_presets": applied_presets,
    }
