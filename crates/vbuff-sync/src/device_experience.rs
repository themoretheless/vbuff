//! Content-free planning primitives for paired-device UX.

mod outbox;
mod policy;
mod revocation;
mod snippets;
mod timeline;
mod travel;

pub use outbox::{OutboxEntry, OutboxStatus, SyncOutbox};
pub use policy::{
    BandwidthMode, DeviceExperiencePolicy, DeviceTrustLevel, NearbyTarget, PairingRehearsal,
    ReplaySelection, SelectiveReplayPlan, SyncDryRunEstimate, SyncItemSummary,
    TransferMaterialization, effective_retention_deadline_ms, estimate_sync, nearby_send_targets,
    plan_selective_replay, rehearse_pairing, transfer_materialization,
};
pub use revocation::{RevocationTombstone, sealed_revocation_tombstones};
pub use snippets::{SharedSnippetProposal, SharedSnippetState};
pub use timeline::{ConflictTimelineOutcome, ConflictTimelinePoint, conflict_timeline};
pub use travel::{QrHandoffToken, TravelMode};

const MAX_DEVICE_ID_BYTES: usize = 128;
const MAX_COLLECTION_BYTES: usize = 256;
const MAX_PLAN_ITEMS: usize = 10_000;
const MAX_DEVICE_COUNT: usize = 1_024;
const MAX_SHARED_APPROVERS: usize = 1_024;
const MAX_NEARBY_TARGETS: usize = 16;
const MAX_NEARBY_AGE_MS: u64 = 5 * 60 * 1_000;
const MAX_QR_TOKEN_TTL_MS: u64 = 5 * 60 * 1_000;

fn valid_identifier(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn all_zero<const N: usize>(value: &[u8; N]) -> bool {
    value.iter().all(|byte| *byte == 0)
}

#[cfg(test)]
mod tests;
