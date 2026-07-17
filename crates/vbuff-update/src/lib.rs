//! Verification-first release and update contracts.
#![forbid(unsafe_code)]

mod attestation;
mod manifest;

pub use attestation::{
    BuildAttestation, SignedBuildAttestation, parse_sha256_hex, sha256_bytes,
    verify_artifact_checksum, verify_reader_checksum,
};
pub use manifest::{
    Artifact, KeyRotation, SignedUpdateManifest, TrustedKey, UpdateKeyring, UpdateManifest,
    UpdateVerifier, VerifiedUpdate,
};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, UpdateError>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UpdateError {
    #[error("invalid update manifest: {0}")]
    InvalidManifest(String),
    #[error("unknown or inactive signing key")]
    UntrustedKey,
    #[error("update signature verification failed")]
    InvalidSignature,
    #[error("update would downgrade or replay an accepted release")]
    DowngradeOrReplay,
    #[error("this client is too old for the update")]
    IncompatibleClient,
    #[error("artifact checksum mismatch")]
    ChecksumMismatch,
    #[error("release serialization failed: {0}")]
    Serialization(String),
    #[error("release artifact I/O failed: {0}")]
    Io(String),
}
