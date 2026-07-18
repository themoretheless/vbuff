//! Versioned, transport-independent contracts for local vbuff clients.
#![forbid(unsafe_code)]

pub mod api_token;
pub mod batch;
pub mod callback;
pub mod dry_run;
pub mod event;
pub mod handshake;
pub mod integration;

pub use api_token::{ApiScope, ApiTokenClaims, ApiTokenError, ApiTokenIssuer};
pub use batch::{BatchMutation, BatchRequest, BatchResponse};
pub use callback::{
    CallbackError, CallbackInvocation, CallbackTarget, CallbackTokenIssuer, TransformAction,
};
pub use dry_run::{DryRunPreview, DryRunRequest};
pub use event::{EventEnvelope, EventFilter, EventKind};
pub use handshake::{
    Capability, ClientHello, HandshakeError, ProtocolRange, ServerPolicy, ServerWelcome, negotiate,
};
pub use integration::{
    AutomationCommand, AutomationSurface, BrowserIngress, BrowserIngressDecision,
    ClipAccessContext, ClipAccessFilter, EditorCaptureMetadata, EditorPasteContext,
    EditorTargetKind, HistoryQuery, IntegrationContractError, LauncherRankSignals, McpReadPolicy,
    RemotePasteRequest, ShareDraft, ShareDraftState, SnippetBridgeCursor, TargetedSendRequest,
};
