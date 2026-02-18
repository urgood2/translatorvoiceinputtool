//! Shared model default resolution for Rust-side components.

use once_cell::sync::Lazy;
use serde::Deserialize;

/// Fallback model ID if manifest parsing fails.
const FALLBACK_MODEL_ID: &str = "parakeet-tdt-0.6b-v3";

/// Canonical manifest path shared with sidecar and docs.
const MODEL_MANIFEST_JSON: &str = include_str!("../../shared/model/MODEL_MANIFEST.json");

#[derive(Debug, Deserialize)]
struct ManifestModelDefaults {
    model_id: String,
}

static DEFAULT_MODEL_ID: Lazy<String> =
    Lazy::new(
        || match serde_json::from_str::<ManifestModelDefaults>(MODEL_MANIFEST_JSON) {
            Ok(manifest) => {
                let trimmed = manifest.model_id.trim();
                if trimmed.is_empty() {
                    log::warn!(
                        "MODEL_MANIFEST.json contains empty model_id; using fallback '{}'",
                        FALLBACK_MODEL_ID
                    );
                    FALLBACK_MODEL_ID.to_string()
                } else {
                    trimmed.to_string()
                }
            }
            Err(error) => {
                log::warn!(
                "Failed to parse MODEL_MANIFEST.json for default model_id: {}; using fallback '{}'",
                error,
                FALLBACK_MODEL_ID
            );
                FALLBACK_MODEL_ID.to_string()
            }
        },
    );

/// Return the canonical default model ID for Rust-side flows.
pub fn default_model_id() -> &'static str {
    DEFAULT_MODEL_ID.as_str()
}

#[cfg(test)]
mod tests {
    use super::default_model_id;

    #[test]
    fn test_default_model_id_comes_from_manifest() {
        assert_eq!(default_model_id(), "parakeet-tdt-0.6b-v3");
    }
}
