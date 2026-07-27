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
    BrowserPrivacyPolicy, BrowserSourceReport, BrowserStorageDisposition, CleanLinkRequest,
    ClipAccessContext, ClipAccessFilter, EditorCaptureMetadata, EditorPasteContext,
    EditorTargetKind, HistoryQuery, IntegrationContractError, LauncherCandidate, LauncherClient,
    LauncherRankSignals, LauncherRankedResult, LauncherRequest, McpReadPolicy, McpSessionLease,
    Osc52Decision, Osc52Observation, Osc52Policy, Osc52Target, RemotePasteLease,
    RemotePasteRequest, RemoteReplayWindow, SelectedLinkMetadata, ShareDraft, ShareDraftState,
    SignedWebhookEvent, SnippetBridgeCursor, SnippetMirrorAction, SnippetMirrorOperation,
    SnippetMirrorRecord, TargetedSendRequest, VimRegisterAction, VimRegisterRequest, WebhookEvent,
    WebhookEventKind, WebhookReplayWindow, WebhookSigner, adapt_text_for_editor,
    plan_snippet_mirror, rank_launcher_candidates,
};
