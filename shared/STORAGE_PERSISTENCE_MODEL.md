# Storage and Persistence Model

This reference captures the storage and persistence model from plan §7.
Use this before changing config handling, model cache behavior, transcript history, logs, or diagnostics exports.

## Storage Locations

1. Config files
- Platform config directory: `OpenVoicy/config.json`.
- Lifecycle companions: `config.json.tmp` for atomic write staging and `config.json.corrupt` for failed-load backup.

2. Model cache
- Cache root: `~/.cache/openvoicy/models/<model_id>/...`.
- Install path is model-id scoped so cache operations are atomic per model.

3. Transcript history
- Default (current): in-memory ring buffer (`history.rs`) sized by `history.max_entries`. No disk persistence; cleared on app quit.
- Planned: encrypted JSONL disk persistence when `history.persistence_mode="disk"` is explicitly enabled. Config fields (`persistence_mode`, `encrypt_at_rest`) are validated at load time but the disk write path is **not yet implemented**.

4. Embedded packaged assets
- Presets, manifests, and contracts ship inside app/sidecar package artifacts.

5. Logging storage
- Default: in-memory ring buffer (`log_buffer.rs`).
- Optional persistent logs: rotated file logs used only for diagnostics/debug workflows.

6. Frontend build artifacts
- Main UI build output in `dist/`.
- Overlay support adds an additional built HTML entry in the packaged frontend bundle.

## Config File Lifecycle

1. Atomic writes
- Write new content to `.tmp` first.
- Rename `.tmp` to `config.json` only after successful write/flush.

2. Corruption handling
- If config load/parse fails, rename bad file to `.corrupt`.
- Start from safe defaults rather than deleting state blindly.

3. Migration behavior
- Missing new fields are populated with safe defaults.
- Migration must be additive and avoid destructive data loss.

4. Safety rule
- Never hard-delete user config during recovery.
- Worst-case recovery state is preserved via `.corrupt` backup.

## Model Cache Lifecycle

1. Download staging
- Download model artifacts into a `.partial` directory.

2. Integrity verification
- Validate SHA256 digests and expected file sizes before activation.

3. Atomic activation
- Rename `.partial` to final `<model_id>` directory only after verification succeeds.
- Prevent half-installed model states from becoming visible.

4. Purge semantics
- Purge removes the full `<model_id>` directory tree.

## Log Buffer and Diagnostics

1. In-memory default
- Keep recent logs in the in-memory ring buffer by default.

2. Diagnostics export
- `generate_diagnostics` includes ring buffer data plus system/runtime context.

3. Privacy-first default
- Persistent log files are disabled by default.

4. Optional debug logging
- If enabled for debugging, file logs must be rotated and excluded from source control.

## Implementation Constraints

- Keep persistence behavior explicit and additive.
- Preserve privacy-first defaults unless the user explicitly opts in.
- Treat storage format changes as migration-impacting and test with recovery paths.
