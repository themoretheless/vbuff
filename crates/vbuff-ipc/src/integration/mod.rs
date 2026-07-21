//! Capability-honest contracts shared by optional integration adapters.

mod access;
mod automation;
mod browser;
mod editor;
mod error;
mod query;
mod terminal;
mod webhook;

pub use access::{ClipAccessContext, ClipAccessFilter, McpReadPolicy, McpSessionLease};
pub use automation::{
    AutomationCommand, AutomationSurface, RemotePasteLease, RemotePasteRequest, RemoteReplayWindow,
    ShareDraft, ShareDraftState, SnippetBridgeCursor, SnippetMirrorAction, SnippetMirrorOperation,
    SnippetMirrorRecord, TargetedSendRequest, VimRegisterAction, VimRegisterRequest,
    plan_snippet_mirror,
};
pub use browser::{
    BrowserIngress, BrowserIngressDecision, BrowserSourceReport, CleanLinkRequest,
    SelectedLinkMetadata,
};
pub use editor::{
    EditorCaptureMetadata, EditorPasteContext, EditorTargetKind, adapt_text_for_editor,
};
pub use error::IntegrationContractError;
pub use query::{
    HistoryQuery, LauncherCandidate, LauncherClient, LauncherRankSignals, LauncherRankedResult,
    LauncherRequest, rank_launcher_candidates,
};
pub use terminal::{Osc52Decision, Osc52Observation, Osc52Policy, Osc52Target};
pub use webhook::{
    SignedWebhookEvent, WebhookEvent, WebhookEventKind, WebhookReplayWindow, WebhookSigner,
};
