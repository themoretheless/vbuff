//! Versioned, transport-independent contracts for local vbuff clients.
#![forbid(unsafe_code)]

pub mod api_token;
pub mod batch;
pub mod dry_run;
pub mod event;
pub mod handshake;

pub use api_token::{ApiScope, ApiTokenClaims, ApiTokenError, ApiTokenIssuer};
pub use batch::{BatchMutation, BatchRequest, BatchResponse};
pub use dry_run::{DryRunPreview, DryRunRequest};
pub use event::{EventEnvelope, EventFilter, EventKind};
pub use handshake::{
    Capability, ClientHello, HandshakeError, ProtocolRange, ServerPolicy, ServerWelcome, negotiate,
};
