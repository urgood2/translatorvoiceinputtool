#!/usr/bin/env python3
"""Schema validation utilities.

This script validates JSON documents against the shared schemas and can be used
in CI to ensure type consistency across implementations.

Usage:
    # Validate a config file
    python validate.py AppConfig.schema.json /path/to/config.json

    # Run self-tests
    python validate.py --self-test

    # Validate all test vectors
    python validate.py --test-vectors
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

try:
    import jsonschema
    from jsonschema import Draft7Validator, RefResolver
except ImportError:
    print("Error: jsonschema library required. Install with: pip install jsonschema")
    sys.exit(2)


SCHEMA_DIR = Path(__file__).parent
REPLACEMENTS_DIR = SCHEMA_DIR.parent / "replacements"


def load_schema(schema_name: str) -> dict[str, Any]:
    """Load a schema file from the schema directory."""
    schema_path = SCHEMA_DIR / schema_name
    if not schema_path.exists():
        raise FileNotFoundError(f"Schema not found: {schema_path}")

    with open(schema_path) as f:
        return json.load(f)


def create_resolver() -> RefResolver:
    """Create a resolver that can handle local schema references."""
    # Load the main schemas
    app_config_schema = load_schema("AppConfig.schema.json")
    replacement_rule_schema = load_schema("ReplacementRule.schema.json")

    # Create a store with all schemas
    store = {
        app_config_schema.get("$id", "AppConfig.schema.json"): app_config_schema,
        replacement_rule_schema.get("$id", "ReplacementRule.schema.json"): replacement_rule_schema,
        "ReplacementRule.schema.json": replacement_rule_schema,
    }

    return RefResolver.from_schema(app_config_schema, store=store)


def validate_document(schema_name: str, document: dict[str, Any]) -> list[str]:
    """Validate a document against a schema.

    Returns:
        List of validation error messages (empty if valid).
    """
    schema = load_schema(schema_name)
    resolver = create_resolver()

    validator = Draft7Validator(schema, resolver=resolver)

    errors = []
    for error in validator.iter_errors(document):
        path = ".".join(str(p) for p in error.absolute_path) or "(root)"
        errors.append(f"{path}: {error.message}")

    return errors


def validate_file(schema_name: str, file_path: Path) -> list[str]:
    """Validate a JSON file against a schema."""
    with open(file_path) as f:
        document = json.load(f)
    return validate_document(schema_name, document)


def self_test() -> bool:
    """Run self-tests to verify schemas are valid and work correctly."""
    print("Running schema self-tests...")
    all_passed = True

    # Test 1: Schemas are valid JSON Schema
    print("\n1. Checking schemas are valid JSON Schema draft-07...")
    for schema_name in ["ReplacementRule.schema.json", "AppConfig.schema.json"]:
        try:
            schema = load_schema(schema_name)
            Draft7Validator.check_schema(schema)
            print(f"   {schema_name}: PASS")
        except jsonschema.SchemaError as e:
            print(f"   {schema_name}: FAIL - {e.message}")
            all_passed = False

    # Test 2: Valid ReplacementRule examples pass
    print("\n2. Testing valid ReplacementRule examples...")
    valid_rules = [
        {
            "id": "test-1",
            "enabled": True,
            "kind": "literal",
            "pattern": "BTW",
            "replacement": "by the way",
            "word_boundary": True,
            "case_sensitive": False
        },
        {
            "id": "test-2",
            "enabled": True,
            "kind": "regex",
            "pattern": "\\$\\d+",
            "replacement": "[PRICE]",
            "word_boundary": False,
            "case_sensitive": False,
            "description": "Price redaction",
            "origin": "user"
        }
    ]

    for i, rule in enumerate(valid_rules):
        errors = validate_document("ReplacementRule.schema.json", rule)
        if errors:
            print(f"   Rule {i+1}: FAIL - {errors}")
            all_passed = False
        else:
            print(f"   Rule {i+1}: PASS")

    # Test 3: Invalid ReplacementRule examples fail
    print("\n3. Testing invalid ReplacementRule examples are rejected...")
    invalid_rules = [
        ({"id": "x", "enabled": True}, "missing required fields"),
        ({"id": "", "enabled": True, "kind": "literal", "pattern": "a", "replacement": "", "word_boundary": False, "case_sensitive": True}, "empty id"),
        ({"id": "x", "enabled": True, "kind": "invalid", "pattern": "a", "replacement": "", "word_boundary": False, "case_sensitive": True}, "invalid kind"),
        ({"id": "x", "enabled": True, "kind": "literal", "pattern": "", "replacement": "", "word_boundary": False, "case_sensitive": True}, "empty pattern"),
        ({"id": "x", "enabled": "yes", "kind": "literal", "pattern": "a", "replacement": "", "word_boundary": False, "case_sensitive": True}, "enabled not boolean"),
    ]

    for rule, reason in invalid_rules:
        errors = validate_document("ReplacementRule.schema.json", rule)
        if errors:
            print(f"   '{reason}': PASS (rejected)")
        else:
            print(f"   '{reason}': FAIL (should have been rejected)")
            all_passed = False

    # Test 4: Valid AppConfig examples pass
    print("\n4. Testing valid AppConfig examples...")
    valid_configs = [
        ("minimal-existing-fields", {"schema_version": 1}),
        (
            "all-new-fields-populated",
            {
                "schema_version": 1,
                "audio": {
                    "device_uid": None,
                    "audio_cues_enabled": True,
                    "trim_silence": True,
                    "vad_enabled": True,
                    "vad_silence_ms": 1200,
                    "vad_min_speech_ms": 250,
                },
                "hotkeys": {
                    "primary": "Ctrl+Shift+Space",
                    "copy_last": "Ctrl+Shift+V",
                    "mode": "hold",
                },
                "injection": {
                    "paste_delay_ms": 40,
                    "restore_clipboard": True,
                    "suffix": " ",
                    "focus_guard_enabled": True,
                    "app_overrides": {
                        "slack": {"paste_delay_ms": 120, "use_clipboard_only": True}
                    },
                },
                "model": {
                    "model_id": "nvidia/parakeet-tdt-0.6b-v2",
                    "device": "auto",
                    "preferred_device": "gpu",
                    "language": "de",
                },
                "ui": {
                    "show_on_startup": True,
                    "window_width": 600,
                    "window_height": 500,
                    "theme": "dark",
                    "onboarding_completed": False,
                    "overlay_enabled": True,
                    "locale": "en-US",
                    "reduce_motion": True,
                },
                "history": {
                    "persistence_mode": "disk",
                    "max_entries": 100,
                    "encrypt_at_rest": True,
                },
                "presets": {"enabled_presets": ["punctuation"]},
            },
        ),
        (
            "null-values-accepted",
            {
                "schema_version": 1,
                "model": {
                    "model_id": "nvidia/parakeet-tdt-0.6b-v2",
                    "device": "auto",
                    "preferred_device": "auto",
                    "language": None,
                },
                "ui": {
                    "show_on_startup": True,
                    "window_width": 600,
                    "window_height": 500,
                    "theme": "system",
                    "onboarding_completed": True,
                    "overlay_enabled": True,
                    "locale": None,
                    "reduce_motion": False,
                },
            },
        ),
    ]

    for case_name, config in valid_configs:
        errors = validate_document("AppConfig.schema.json", config)
        if errors:
            print(f"   {case_name}: FAIL - {errors}")
            all_passed = False
        else:
            print(f"   {case_name}: PASS")

    # Test 5: Invalid AppConfig examples fail
    print("\n5. Testing invalid AppConfig examples are rejected...")
    invalid_configs = [
        ({}, "missing schema_version"),
        ({"schema_version": "1"}, "schema_version not integer"),
        ({"schema_version": 0}, "schema_version below minimum"),
        ({"schema_version": 1, "audio": {"device_uid": 123}}, "device_uid not string"),
        ({"schema_version": 1, "hotkeys": {"mode": "invalid"}}, "invalid hotkey mode"),
        ({"schema_version": 1, "ui": {"theme": "invalid"}}, "invalid ui.theme enum"),
        ({"schema_version": 1, "model": {"preferred_device": "tpu"}}, "invalid model.preferred_device enum"),
        ({"schema_version": 1, "history": {"persistence_mode": "remote"}}, "invalid history.persistence_mode enum"),
        ({"schema_version": 1, "injection": {"paste_delay_ms": 5}}, "paste_delay below minimum"),
        ({"schema_version": 1, "injection": {"paste_delay_ms": 1000}}, "paste_delay above maximum"),
        ({"schema_version": 1, "audio": {"vad_silence_ms": 50}}, "vad_silence below minimum"),
        ({"schema_version": 1, "audio": {"vad_min_speech_ms": 50}}, "vad_min_speech below minimum"),
        (
            {"schema_version": 1, "injection": {"app_overrides": {"slack": {"use_clipboard_only": "yes"}}}},
            "app_override use_clipboard_only not boolean",
        ),
    ]

    for config, reason in invalid_configs:
        errors = validate_document("AppConfig.schema.json", config)
        if errors:
            print(f"   '{reason}': PASS (rejected)")
        else:
            print(f"   '{reason}': FAIL (should have been rejected)")
            all_passed = False

    print()
    if all_passed:
        print("All self-tests PASSED")
    else:
        print("Some self-tests FAILED")

    return all_passed


def test_vectors() -> bool:
    """Validate rules from TEST_VECTORS.json against ReplacementRule schema."""
    print("Validating test vectors...")

    vectors_path = REPLACEMENTS_DIR / "TEST_VECTORS.json"
    if not vectors_path.exists():
        print(f"   Test vectors not found: {vectors_path}")
        return False

    with open(vectors_path) as f:
        vectors = json.load(f)

    all_passed = True
    test_cases = vectors.get("test_cases", [])

    for i, case in enumerate(test_cases):
        name = case.get("name", f"case {i}")
        rules = case.get("rules", [])

        for j, rule in enumerate(rules):
            errors = validate_document("ReplacementRule.schema.json", rule)
            if errors:
                print(f"   {name}, rule {j}: FAIL - {errors}")
                all_passed = False

    if all_passed:
        print(f"   All {len(test_cases)} test cases PASSED")

    return all_passed


def main():
    parser = argparse.ArgumentParser(description="Validate JSON documents against schemas")
    parser.add_argument("schema", nargs="?", help="Schema file name (e.g., AppConfig.schema.json)")
    parser.add_argument("document", nargs="?", help="JSON document to validate")
    parser.add_argument("--self-test", action="store_true", help="Run self-tests")
    parser.add_argument("--test-vectors", action="store_true", help="Validate test vectors")

    args = parser.parse_args()

    if args.self_test:
        success = self_test()
        sys.exit(0 if success else 1)

    if args.test_vectors:
        success = test_vectors()
        sys.exit(0 if success else 1)

    if not args.schema or not args.document:
        parser.print_help()
        sys.exit(2)

    doc_path = Path(args.document)
    if not doc_path.exists():
        print(f"Error: Document not found: {doc_path}")
        sys.exit(2)

    errors = validate_file(args.schema, doc_path)

    if errors:
        print(f"Validation FAILED for {doc_path}:")
        for error in errors:
            print(f"  - {error}")
        sys.exit(1)
    else:
        print(f"Validation PASSED for {doc_path}")
        sys.exit(0)


if __name__ == "__main__":
    main()
