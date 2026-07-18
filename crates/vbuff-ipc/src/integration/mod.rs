//! Capability-honest contracts shared by optional integration adapters.

mod access;
mod automation;
mod browser;
mod editor;
mod query;

pub use access::{ClipAccessContext, ClipAccessFilter, McpReadPolicy};
pub use automation::{
    AutomationCommand, AutomationSurface, RemotePasteRequest, ShareDraft, ShareDraftState,
    SnippetBridgeCursor, TargetedSendRequest,
};
pub use browser::{BrowserIngress, BrowserIngressDecision};
pub use editor::{EditorCaptureMetadata, EditorPasteContext, EditorTargetKind};
pub use query::{HistoryQuery, IntegrationContractError, LauncherRankSignals};
