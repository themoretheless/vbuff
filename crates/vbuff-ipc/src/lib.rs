//! Versioned, transport-independent contracts for local vbuff clients.
#![forbid(unsafe_code)]

pub mod api_token;
pub mod batch;
pub mod callback;
pub mod dry_run;
pub mod event;
pub mod handshake;
pub mod integration;

/// The bounded replay window is shared with `vbuff-sync`, so it lives in
/// `vbuff-types` (the crate both depend on) rather than here. Re-exported
/// under the old path so this crate's guards read unchanged.
pub(crate) use vbuff_types::replay;

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
    SnippetMirrorRecord, SnippetSyncManifest, SnippetSyncedState, TargetedSendRequest,
    VimRegisterAction, VimRegisterRequest, WebhookEvent, WebhookEventKind, WebhookReplayWindow,
    WebhookSigner, adapt_text_for_editor, plan_snippet_mirror, rank_launcher_candidates,
};

/// Lowercase hex, for pinning MAC bytes readably in the freeze tests.
#[cfg(test)]
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `HMAC-SHA256(key, domain_bytes ‖ parts…)`, where `domain_bytes` is a
/// mechanism's domain constant exactly as it used to be written by hand -
/// terminator included, because the terminator used to live inside the
/// constant.
///
/// Every mechanism in this crate now authenticates through the single
/// `vbuff_types::mac::hmac_proof` primitive. This is the *other*
/// implementation: the duplicated `Hmac::update` sequence that primitive
/// replaced, kept alive only in tests so the freeze assertions compare the
/// shared primitive against something other than itself. It is the executable
/// form of "these bytes are already issued and may not move".
#[cfg(test)]
pub(crate) fn legacy_mac(domain_bytes: &[u8], key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(domain_bytes);
    for part in parts {
        mac.update(part);
    }
    mac.finalize().into_bytes().into()
}
