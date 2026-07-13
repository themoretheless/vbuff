//! Capability-scoped plugin contracts with deterministic host-side execution.
#![forbid(unsafe_code)]

pub mod adapter;
pub mod bundle;
pub mod component;
pub mod manifest;
pub mod pipeline;
pub mod recognizer;

use thiserror::Error;

pub use adapter::{ExportAdapter, ExportRecord, ImportAdapter, ImportRecord};
pub use bundle::{LockedPlugin, PluginBundle, PluginLock, SignedBundle};
pub use manifest::{CapabilityGrant, PluginCapability, PluginManifest};
pub use pipeline::{Pipeline, PipelinePreview, TransformSpec, TypedValue, ValueType};
pub use recognizer::{ActionCandidate, Recognizer, RecognizerInput, TypedAction, run_recognizer};

pub type Result<T> = std::result::Result<T, PluginError>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PluginError {
    #[error("invalid plugin manifest: {0}")]
    InvalidManifest(String),
    #[error("plugin capability was not granted: {0}")]
    CapabilityDenied(String),
    #[error("transform type mismatch: {0}")]
    TypeMismatch(String),
    #[error("invalid transform input: {0}")]
    InvalidInput(String),
    #[error("invalid plugin bundle: {0}")]
    InvalidBundle(String),
    #[error("plugin signature verification failed")]
    InvalidSignature,
    #[error("plugin serialization failed: {0}")]
    Serialization(String),
}
