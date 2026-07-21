use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use vbuff_types::ContentKind;

use super::{
    MAX_COLLECTION_BYTES, MAX_DEVICE_COUNT, MAX_DEVICE_ID_BYTES, MAX_NEARBY_AGE_MS,
    MAX_NEARBY_TARGETS, MAX_PLAN_ITEMS, valid_identifier,
};
use crate::{Result, SyncError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceTrustLevel {
    FullTrust,
    ReceiveOnly,
    SendOnly,
    Untrusted,
}

impl DeviceTrustLevel {
    pub const fn accepts_from_local(self) -> bool {
        matches!(self, Self::FullTrust | Self::ReceiveOnly)
    }

    pub const fn may_send_to_local(self) -> bool {
        matches!(self, Self::FullTrust | Self::SendOnly)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandwidthMode {
    #[default]
    Full,
    MetadataFirst,
    OnDemand,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceExperiencePolicy {
    pub device_id: String,
    pub trust: DeviceTrustLevel,
    pub clipboard_write_allowed: bool,
    pub retention_days: Option<u16>,
    pub mask_sensitive_history: bool,
    pub bandwidth_mode: BandwidthMode,
    pub nearby_verified: bool,
    pub last_seen_ms: u64,
}

impl fmt::Debug for DeviceExperiencePolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeviceExperiencePolicy")
            .field("device_id", &"[redacted]")
            .field("trust", &self.trust)
            .field("clipboard_write_allowed", &self.clipboard_write_allowed)
            .field("retention_days", &self.retention_days)
            .field("mask_sensitive_history", &self.mask_sensitive_history)
            .field("bandwidth_mode", &self.bandwidth_mode)
            .field("nearby_verified", &self.nearby_verified)
            .field("last_seen_ms", &self.last_seen_ms)
            .finish()
    }
}

impl DeviceExperiencePolicy {
    pub fn validate(&self) -> Result<()> {
        if !valid_identifier(&self.device_id, MAX_DEVICE_ID_BYTES)
            || self
                .retention_days
                .is_some_and(|days| days == 0 || days > 3_650)
            || (self.trust == DeviceTrustLevel::Untrusted && self.clipboard_write_allowed)
        {
            return Err(SyncError::Invalid("invalid device policy".into()));
        }
        Ok(())
    }

    pub fn allows_live_clipboard_write(&self) -> bool {
        self.validate().is_ok() && self.clipboard_write_allowed && self.trust.may_send_to_local()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncItemSummary {
    pub item_id: String,
    pub collection: Option<String>,
    pub kind: ContentKind,
    pub byte_size: u64,
    pub sensitive: bool,
    pub sync_eligible: bool,
    pub has_thumbnail: bool,
    pub created_at_ms: u64,
}

impl fmt::Debug for SyncItemSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SyncItemSummary")
            .field("item_id", &"[redacted]")
            .field("has_collection", &self.collection.is_some())
            .field("kind", &self.kind)
            .field("byte_size", &self.byte_size)
            .field("sensitive", &self.sensitive)
            .field("sync_eligible", &self.sync_eligible)
            .field("has_thumbnail", &self.has_thumbnail)
            .field("created_at_ms", &self.created_at_ms)
            .finish()
    }
}

impl SyncItemSummary {
    pub fn validate(&self) -> Result<()> {
        if !valid_identifier(&self.item_id, 128)
            || self.collection.as_ref().is_some_and(|collection| {
                collection.is_empty()
                    || collection.len() > MAX_COLLECTION_BYTES
                    || collection.chars().any(char::is_control)
            })
        {
            return Err(SyncError::Invalid("invalid sync item summary".into()));
        }
        Ok(())
    }

    pub const fn local_only_by_default(&self) -> bool {
        self.sensitive || !self.sync_eligible
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingRehearsal {
    pub transferable_items: usize,
    pub transferable_bytes: u64,
    pub local_only_items: usize,
    pub metadata_first_items: usize,
}

pub fn rehearse_pairing(
    policy: &DeviceExperiencePolicy,
    items: &[SyncItemSummary],
) -> Result<PairingRehearsal> {
    policy.validate()?;
    bounded_items(items)?;
    let mut rehearsal = PairingRehearsal::default();
    for item in items {
        item.validate()?;
        if item.local_only_by_default() || !policy.trust.accepts_from_local() {
            rehearsal.local_only_items += 1;
            continue;
        }
        rehearsal.transferable_items += 1;
        rehearsal.transferable_bytes = rehearsal.transferable_bytes.saturating_add(item.byte_size);
        if policy.bandwidth_mode != BandwidthMode::Full {
            rehearsal.metadata_first_items += 1;
        }
    }
    Ok(rehearsal)
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct ReplaySelection {
    pub item_ids: BTreeSet<String>,
    pub collections: BTreeSet<String>,
    pub maximum_items: usize,
}

impl fmt::Debug for ReplaySelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReplaySelection")
            .field("item_count", &self.item_ids.len())
            .field("collection_count", &self.collections.len())
            .field("maximum_items", &self.maximum_items)
            .finish()
    }
}

impl ReplaySelection {
    fn validate(&self) -> Result<()> {
        if self.maximum_items == 0
            || self.maximum_items > MAX_PLAN_ITEMS
            || self.item_ids.len() > MAX_PLAN_ITEMS
            || self.collections.len() > MAX_PLAN_ITEMS
            || self.item_ids.len().saturating_add(self.collections.len()) > MAX_PLAN_ITEMS
            || self
                .item_ids
                .iter()
                .any(|item_id| !valid_identifier(item_id, 128))
            || self.collections.iter().any(|collection| {
                collection.is_empty()
                    || collection.len() > MAX_COLLECTION_BYTES
                    || collection.chars().any(char::is_control)
            })
        {
            return Err(SyncError::Invalid(
                "invalid selective replay selection".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SelectiveReplayPlan {
    pub item_ids: Vec<String>,
    pub total_bytes: u64,
    pub skipped_local_only: usize,
}

impl fmt::Debug for SelectiveReplayPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SelectiveReplayPlan")
            .field("item_count", &self.item_ids.len())
            .field("total_bytes", &self.total_bytes)
            .field("skipped_local_only", &self.skipped_local_only)
            .finish()
    }
}

pub fn plan_selective_replay(
    policy: &DeviceExperiencePolicy,
    items: &[SyncItemSummary],
    selection: &ReplaySelection,
) -> Result<SelectiveReplayPlan> {
    policy.validate()?;
    bounded_items(items)?;
    selection.validate()?;
    let mut plan = SelectiveReplayPlan {
        item_ids: Vec::new(),
        total_bytes: 0,
        skipped_local_only: 0,
    };
    for item in items {
        item.validate()?;
        let selected = selection.item_ids.contains(&item.item_id)
            || item
                .collection
                .as_ref()
                .is_some_and(|collection| selection.collections.contains(collection));
        if !selected {
            continue;
        }
        if item.local_only_by_default() || !policy.trust.accepts_from_local() {
            plan.skipped_local_only += 1;
            continue;
        }
        if plan.item_ids.len() == selection.maximum_items {
            break;
        }
        plan.total_bytes = plan.total_bytes.saturating_add(item.byte_size);
        plan.item_ids.push(item.item_id.clone());
    }
    Ok(plan)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferMaterialization {
    Denied,
    MetadataOnly,
    MetadataAndThumbnail,
    FullPayload,
}

pub fn transfer_materialization(
    policy: &DeviceExperiencePolicy,
    item: &SyncItemSummary,
    payload_requested: bool,
) -> TransferMaterialization {
    if policy.validate().is_err()
        || item.validate().is_err()
        || item.local_only_by_default()
        || !policy.trust.accepts_from_local()
    {
        return TransferMaterialization::Denied;
    }
    match policy.bandwidth_mode {
        BandwidthMode::Full => TransferMaterialization::FullPayload,
        BandwidthMode::MetadataFirst if item.has_thumbnail && !payload_requested => {
            TransferMaterialization::MetadataAndThumbnail
        }
        BandwidthMode::MetadataFirst | BandwidthMode::OnDemand if !payload_requested => {
            TransferMaterialization::MetadataOnly
        }
        BandwidthMode::MetadataFirst | BandwidthMode::OnDemand => {
            TransferMaterialization::FullPayload
        }
    }
}

pub fn effective_retention_deadline_ms(
    policy: &DeviceExperiencePolicy,
    item: &SyncItemSummary,
    global_retention_days: u16,
) -> Result<Option<u64>> {
    policy.validate()?;
    item.validate()?;
    if global_retention_days > 3_650 {
        return Err(SyncError::Invalid("invalid global retention".into()));
    }
    let device_days = policy.retention_days.unwrap_or(global_retention_days);
    let days = if item.sensitive && policy.mask_sensitive_history {
        device_days.min(1)
    } else {
        device_days
    };
    Ok((days > 0).then(|| {
        item.created_at_ms
            .saturating_add(u64::from(days) * 24 * 60 * 60 * 1_000)
    }))
}

#[derive(Clone, PartialEq, Eq)]
pub struct NearbyTarget {
    pub device_id: String,
    pub trust: DeviceTrustLevel,
    pub last_seen_ms: u64,
}

impl fmt::Debug for NearbyTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NearbyTarget")
            .field("device_id", &"[redacted]")
            .field("trust", &self.trust)
            .field("last_seen_ms", &self.last_seen_ms)
            .finish()
    }
}

pub fn nearby_send_targets(
    devices: &[DeviceExperiencePolicy],
    now_ms: u64,
) -> Result<Vec<NearbyTarget>> {
    validate_devices(devices)?;
    let mut targets = Vec::new();
    for device in devices {
        if device.nearby_verified
            && device.trust.accepts_from_local()
            && device.last_seen_ms <= now_ms
            && now_ms - device.last_seen_ms <= MAX_NEARBY_AGE_MS
        {
            targets.push(NearbyTarget {
                device_id: device.device_id.clone(),
                trust: device.trust,
                last_seen_ms: device.last_seen_ms,
            });
        }
    }
    targets.sort_by(|left, right| {
        right
            .last_seen_ms
            .cmp(&left.last_seen_ms)
            .then_with(|| left.device_id.cmp(&right.device_id))
    });
    targets.truncate(MAX_NEARBY_TARGETS);
    Ok(targets)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncDryRunEstimate {
    pub item_count: usize,
    pub payload_bytes: u64,
    pub sensitive_local_only: usize,
    pub affected_devices: usize,
    pub metadata_first_items: usize,
}

pub fn estimate_sync(
    devices: &[DeviceExperiencePolicy],
    items: &[SyncItemSummary],
) -> Result<SyncDryRunEstimate> {
    bounded_items(items)?;
    validate_devices(devices)?;
    let mut estimate = SyncDryRunEstimate::default();
    for item in items {
        item.validate()?;
        if item.local_only_by_default() {
            estimate.sensitive_local_only += 1;
            continue;
        }
        estimate.item_count += 1;
        estimate.payload_bytes = estimate.payload_bytes.saturating_add(item.byte_size);
    }
    for device in devices {
        if device.trust.accepts_from_local() {
            estimate.affected_devices += 1;
            if device.bandwidth_mode != BandwidthMode::Full {
                estimate.metadata_first_items = estimate
                    .metadata_first_items
                    .saturating_add(estimate.item_count);
            }
        }
    }
    Ok(estimate)
}

fn bounded_items(items: &[SyncItemSummary]) -> Result<()> {
    if items.len() > MAX_PLAN_ITEMS {
        return Err(SyncError::Invalid("sync plan exceeds item limit".into()));
    }
    Ok(())
}

fn validate_devices(devices: &[DeviceExperiencePolicy]) -> Result<()> {
    if devices.len() > MAX_DEVICE_COUNT {
        return Err(SyncError::Invalid("too many devices".into()));
    }
    let mut ids = BTreeSet::new();
    for device in devices {
        device.validate()?;
        if !ids.insert(device.device_id.as_str()) {
            return Err(SyncError::Invalid("duplicate device policy".into()));
        }
    }
    Ok(())
}
