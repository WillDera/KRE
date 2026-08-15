//! Error types for KIR and koma-core.

use prost::DecodeError;

/// Error converting bytes into KIR.
#[derive(Debug, thiserror::Error)]
pub enum KirError {
    #[error("KIR decode error: {0}")]
    Decode(#[from] DecodeError),
    #[error("unsupported KIR version `{0}` (supported: {1})")]
    UnsupportedVersion(String, &'static str),
    #[error("invalid KIR document: {0}")]
    Invalid(String),
}

/// Error encoding KIR to bytes.
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    #[error("KIR encode error: {0}")]
    Encode(#[from] prost::EncodeError),
}

/// Validate that a document's version matches the supported KIR version.
///
/// Breaking changes require version migration (AGENTS.md: Compatibility).
pub fn validate_version(version: &str) -> Result<(), KirError> {
    if version != super::kir::KIR_VERSION {
        return Err(KirError::UnsupportedVersion(
            version.to_owned(),
            super::kir::KIR_VERSION,
        ));
    }
    Ok(())
}
