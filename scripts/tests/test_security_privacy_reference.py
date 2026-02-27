"""Regression checks for shared/SECURITY_PRIVACY_REQUIREMENTS.md drift."""

import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
REFERENCE = REPO_ROOT / "shared" / "SECURITY_PRIVACY_REQUIREMENTS.md"
CONFIG_RS = REPO_ROOT / "src-tauri" / "src" / "config.rs"
COMMANDS_RS = REPO_ROOT / "src-tauri" / "src" / "commands.rs"


class SecurityPrivacyReferenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.reference_text = REFERENCE.read_text(encoding="utf-8")
        cls.config_text = CONFIG_RS.read_text(encoding="utf-8")
        cls.commands_text = COMMANDS_RS.read_text(encoding="utf-8")

    def test_reference_requires_env_only_hf_token_and_no_token_persistence(self) -> None:
        self.assertIn("Never store tokens in app config.", self.reference_text)
        self.assertIn("`HF_TOKEN` is environment-only and must never be persisted.", self.reference_text)

    def test_reference_lists_required_sensitive_redaction_keywords(self) -> None:
        self.assertIn("`token`", self.reference_text)
        self.assertIn("`key`", self.reference_text)
        self.assertIn("`secret`", self.reference_text)
        self.assertIn("`password`", self.reference_text)

    def test_runtime_config_rejects_unknown_secret_bearing_fields(self) -> None:
        self.assertIn(
            'const SENSITIVE_FIELD_KEYWORDS: [&str; 4] = ["token", "key", "secret", "password"];',
            self.config_text,
        )
        self.assertIn("Rejecting unknown sensitive config field", self.config_text)

    def test_diagnostics_environment_redacts_sensitive_values(self) -> None:
        self.assertIn('upper_key.contains("TOKEN")', self.commands_text)
        self.assertIn('upper_key.contains("SECRET")', self.commands_text)
        self.assertIn('upper_key.contains("PASSWORD")', self.commands_text)
        self.assertIn('upper_key.contains("API_KEY")', self.commands_text)
        self.assertIn('"[REDACTED]"', self.commands_text)


if __name__ == "__main__":
    unittest.main()
