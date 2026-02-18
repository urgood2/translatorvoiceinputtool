# Security and Privacy Requirements

This reference captures non-negotiable security and privacy guardrails from plan §6.
Use this before handling transcripts, credentials, diagnostics, or model download flows.

## Security Requirements

1. Token handling
- Never store tokens in app config.
- `HF_TOKEN` is environment-only and must never be persisted.

2. Logging defaults
- Never log full transcript text by default.
- Default logging should record metadata only (lengths, hashes, IDs, state).

3. Diagnostics redaction
- Diagnostics exports must redact sensitive content by default.
- Redact tokens, API keys, passwords, secrets, and full transcript text unless explicit opt-in is enabled.

4. Attribution and licensing
- Maintain model and license attribution in `docs/THIRD_PARTY_NOTICES.md`.

5. Mirror auth defaults
- Keep default mirror authentication optional (`auth_required=false`).
- Optional Hugging Face auth remains environment-based (`HF_TOKEN`).

6. Config schema constraints
- Config must not contain fields for auth tokens, API keys, or passwords.
- Schema and runtime validation should reject unknown secret-bearing fields.

## Privacy Requirements

1. Transcript persistence defaults
- Transcript persistence is opt-in only.
- Default behavior is in-memory history that resets on restart.

2. Disk persistence mode
- `history.persistence_mode="disk"` is explicit user opt-in.
- When enabled, use OS keychain-managed encryption keys.

3. Keychain fallback behavior
- If keychain is unavailable, unencrypted disk fallback requires explicit user permission.
- No silent downgrade from encrypted to unencrypted persistence.

4. User controls
- Export and purge controls must always remain available.
- Purge must remove persisted transcript payloads, not just UI references.

## Logging and Diagnostics Audit Checklist

1. Rust host logging (`log::info!`, `log::warn!`, `log::error!`)
- Verify no transcript body or credential values are logged by default.
- Prefer session IDs, sizes, durations, and hashed identifiers.

2. Python sidecar logging
- Verify no transcript text leakage in normal logging paths.
- Keep debug-only transcript visibility behind explicit user opt-in.

3. Diagnostics export behavior
- Redact transcript text by default.
- Redact env vars containing case-insensitive keys:
  - `token`
  - `key`
  - `secret`
  - `password`

## Dependency and Supply-Chain Security

1. Trusted sources only
- Do not download or execute code from untrusted sources.

2. Model integrity checks
- Model downloads must come from configured mirrors.
- Verify SHA256 and expected file sizes before activation.

3. Sidecar integrity
- Verify sidecar resource integrity at startup.

4. CI dependency scanning
- Phase 7 adds dependency vulnerability scanning to CI.

## Implementation Notes

- This is a reference artifact and does not replace runtime validation, tests, or security review.
- Any implementation that weakens these defaults must be treated as a policy change and explicitly reviewed.
