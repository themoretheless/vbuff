use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use vbuff_types::ContentKind;
use vbuff_types::mac::{MacDomain, MacProof, hmac_proof};
use vbuff_types::validation::{all_zero, is_valid_label};

use super::IntegrationContractError;

const MAX_ACCESS_LABELS: usize = 256;
const MAX_TAG_BYTES: usize = 128;
const MAX_COLLECTION_BYTES: usize = 256;
const MAX_MCP_LEASE_TTL_MS: u64 = 60 * 60 * 1_000;
/// Frozen framing: every part of a lease preimage is fixed-width, so the
/// missing terminator cannot make two leases collide, and adding one would
/// invalidate the leases held by any process already running against this
/// build (`docs/domain-separation-convention.md` §6.1, §7.3). New parts must
/// stay fixed-width or the domain has to be bumped to `-v2`.
const MCP_LEASE_DOMAIN: MacDomain = MacDomain::legacy_unterminated("vbuff-mcp-lease-v1");

#[derive(Clone, PartialEq, Eq)]
pub struct ClipAccessContext<'a> {
    pub kind: ContentKind,
    pub tags: &'a BTreeSet<String>,
    pub collection: Option<&'a str>,
    pub sensitive: bool,
    pub concealed: bool,
    pub sync_eligible: bool,
}

impl fmt::Debug for ClipAccessContext<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClipAccessContext")
            .field("kind", &self.kind)
            .field("tag_count", &self.tags.len())
            .field("has_collection", &self.collection.is_some())
            .field("sensitive", &self.sensitive)
            .field("concealed", &self.concealed)
            .field("sync_eligible", &self.sync_eligible)
            .finish()
    }
}

/// Empty allowlists deny access. Sensitive, concealed, and sync-excluded clips
/// remain non-exportable even if a filter otherwise matches.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClipAccessFilter {
    pub kinds: BTreeSet<ContentKind>,
    pub tags: BTreeSet<String>,
    pub collections: BTreeSet<String>,
}

impl fmt::Debug for ClipAccessFilter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClipAccessFilter")
            .field("kinds", &self.kinds)
            .field("tag_count", &self.tags.len())
            .field("collection_count", &self.collections.len())
            .finish()
    }
}

impl ClipAccessFilter {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        if self.tags.len() > MAX_ACCESS_LABELS
            || self.collections.len() > MAX_ACCESS_LABELS
            || self
                .tags
                .iter()
                .any(|tag| !is_valid_label(tag, MAX_TAG_BYTES))
            || self
                .collections
                .iter()
                .any(|collection| !is_valid_label(collection, MAX_COLLECTION_BYTES))
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }

    pub fn allows(&self, context: &ClipAccessContext<'_>) -> bool {
        if self.validate().is_err()
            || !valid_context(context)
            || context.sensitive
            || context.concealed
            || !context.sync_eligible
        {
            return false;
        }
        let kind_allowed = !self.kinds.is_empty() && self.kinds.contains(&context.kind);
        let tag_allowed =
            !self.tags.is_empty() && self.tags.iter().any(|tag| context.tags.contains(tag));
        let collection_allowed = !self.collections.is_empty()
            && context
                .collection
                .is_some_and(|collection| self.collections.contains(collection));
        kind_allowed && (tag_allowed || collection_allowed)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpReadPolicy {
    pub required_tag: String,
    pub read_only: bool,
    pub maximum_results: u16,
}

impl fmt::Debug for McpReadPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpReadPolicy")
            .field("required_tag", &"[redacted]")
            .field("read_only", &self.read_only)
            .field("maximum_results", &self.maximum_results)
            .finish()
    }
}

impl Default for McpReadPolicy {
    fn default() -> Self {
        Self {
            required_tag: "AI-shareable".into(),
            read_only: true,
            maximum_results: 20,
        }
    }
}

impl McpReadPolicy {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        if !self.read_only
            || !is_valid_label(&self.required_tag, MAX_TAG_BYTES)
            || !(1..=100).contains(&self.maximum_results)
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }

    pub fn allows(&self, context: &ClipAccessContext<'_>) -> bool {
        self.validate().is_ok()
            && valid_context(context)
            && context.tags.contains(&self.required_tag)
            && !context.sensitive
            && !context.concealed
            && context.sync_eligible
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpSessionLease {
    pub session_id: [u8; 16],
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub policy_hash: [u8; 32],
    pub user_consented: bool,
    proof: [u8; 32],
}

impl fmt::Debug for McpSessionLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpSessionLease")
            .field("session_id", &"[redacted]")
            .field("issued_at_ms", &self.issued_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("policy_hash", &"[redacted]")
            .field("user_consented", &self.user_consented)
            .field("proof", &"[redacted]")
            .finish()
    }
}

impl McpSessionLease {
    pub fn issue(
        policy: &McpReadPolicy,
        session_key: &[u8; 32],
        issued_at_ms: u64,
        ttl_ms: u64,
        user_consented: bool,
    ) -> Result<Self, IntegrationContractError> {
        policy.validate()?;
        if all_zero(session_key) || !user_consented || ttl_ms == 0 || ttl_ms > MAX_MCP_LEASE_TTL_MS
        {
            return Err(IntegrationContractError::InvalidField);
        }
        let mut session_id = [0_u8; 16];
        getrandom::fill(&mut session_id).map_err(|_| IntegrationContractError::InvalidField)?;
        let policy_hash = policy_hash(policy)?;
        let expires_at_ms = issued_at_ms
            .checked_add(ttl_ms)
            .ok_or(IntegrationContractError::InvalidField)?;
        // The lease is assembled first and self-signed second, so that
        // `lease_proof` reads the very fields that will be handed out. A
        // signing path that rebuilt the field list separately is how the two
        // copies used to drift apart.
        let mut lease = Self {
            session_id,
            issued_at_ms,
            expires_at_ms,
            policy_hash,
            user_consented,
            proof: [0; 32],
        };
        lease.proof = lease_proof(&lease, session_key)?.finish();
        Ok(lease)
    }

    pub fn allows(
        &self,
        policy: &McpReadPolicy,
        context: &ClipAccessContext<'_>,
        session_key: &[u8; 32],
        now_ms: u64,
    ) -> bool {
        self.user_consented
            && now_ms >= self.issued_at_ms
            && now_ms < self.expires_at_ms
            && policy_hash(policy).is_ok_and(|hash| hash == self.policy_hash)
            // `lease_proof` rejects an all-zero session key on both paths, so
            // the check no longer has to be repeated here.
            && lease_proof(self, session_key).is_ok_and(|proof| proof.verify(&self.proof))
            && policy.allows(context)
    }
}

fn policy_hash(policy: &McpReadPolicy) -> Result<[u8; 32], IntegrationContractError> {
    policy.validate()?;
    serde_json::to_vec(policy)
        .map(|bytes| *blake3::hash(&bytes).as_bytes())
        .map_err(|_| IntegrationContractError::InvalidField)
}

/// The one statement of what an MCP lease proof covers.
///
/// Every field of [`McpSessionLease`] except `proof` itself is in here, in a
/// fixed order and all fixed-width. Issuance and [`McpSessionLease::allows`]
/// both reach their tag through this function, so a field cannot be added to
/// the lease and signed without also being verified. An all-zero session key
/// is refused here rather than at the call sites, so the refusal applies to
/// both directions at once.
fn lease_proof(
    lease: &McpSessionLease,
    session_key: &[u8; 32],
) -> Result<MacProof, IntegrationContractError> {
    if all_zero(session_key) {
        return Err(IntegrationContractError::InvalidField);
    }
    Ok(hmac_proof(
        MCP_LEASE_DOMAIN,
        session_key,
        &[
            &lease.session_id,
            &lease.issued_at_ms.to_be_bytes(),
            &lease.expires_at_ms.to_be_bytes(),
            &lease.policy_hash,
            &[u8::from(lease.user_consented)],
        ],
    ))
}

fn valid_context(context: &ClipAccessContext<'_>) -> bool {
    context.tags.len() <= MAX_ACCESS_LABELS
        && context
            .tags
            .iter()
            .all(|tag| is_valid_label(tag, MAX_TAG_BYTES))
        && context
            .collection
            .is_none_or(|collection| is_valid_label(collection, MAX_COLLECTION_BYTES))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hex, legacy_mac};

    /// Freeze test for the MCP lease proof, and the executable statement of
    /// which fields it covers. A field added to [`McpSessionLease`] but not to
    /// `lease_proof` changes nothing here, which is exactly why the second
    /// assertion spells the preimage out field by field: the pin catches an
    /// accidental change, and the field list catches an omission.
    #[test]
    fn mcp_lease_mac_bytes_are_frozen_and_cover_every_field() {
        let session_key = [7_u8; 32];
        let mut lease = McpSessionLease {
            session_id: [1; 16],
            issued_at_ms: 100,
            expires_at_ms: 200,
            policy_hash: [2; 32],
            user_consented: true,
            proof: [0; 32],
        };
        lease.proof = lease_proof(&lease, &session_key).unwrap().finish();

        assert_eq!(
            hex(&lease.proof),
            "d0f77cbb49008be9fbfb2018dccbc4036c258001eff68ce5b54ddf4dde3be93c"
        );
        // Byte-identical to the hand-rolled `mac.update(MCP_LEASE_DOMAIN)`
        // sequence this replaced, in the same field order.
        assert_eq!(
            lease.proof,
            legacy_mac(
                b"vbuff-mcp-lease-v1",
                &session_key,
                &[
                    &[1_u8; 16],
                    &100_u64.to_be_bytes(),
                    &200_u64.to_be_bytes(),
                    &[2_u8; 32],
                    &[1_u8],
                ]
            )
        );

        // Every covered field is genuinely covered: flipping any one of them
        // must break verification.
        for tampered in [
            McpSessionLease {
                session_id: [9; 16],
                ..lease.clone()
            },
            McpSessionLease {
                issued_at_ms: 101,
                ..lease.clone()
            },
            McpSessionLease {
                expires_at_ms: 201,
                ..lease.clone()
            },
            McpSessionLease {
                policy_hash: [9; 32],
                ..lease.clone()
            },
            McpSessionLease {
                user_consented: false,
                ..lease.clone()
            },
        ] {
            assert!(
                !lease_proof(&tampered, &session_key)
                    .unwrap()
                    .verify(&tampered.proof)
            );
        }

        // A tag from the same label under the NUL convention is a foreign
        // domain and must be refused.
        let foreign = hmac_proof(
            MacDomain::new("vbuff-mcp-lease-v1"),
            &session_key,
            &[
                &lease.session_id,
                &lease.issued_at_ms.to_be_bytes(),
                &lease.expires_at_ms.to_be_bytes(),
                &lease.policy_hash,
                &[u8::from(lease.user_consented)],
            ],
        )
        .finish();
        assert_ne!(foreign, lease.proof);
        assert!(!lease_proof(&lease, &session_key).unwrap().verify(&foreign));

        // The all-zero key is refused on the verifying path too, not only at
        // issuance.
        assert_eq!(
            lease_proof(&lease, &[0; 32]).err(),
            Some(IntegrationContractError::InvalidField)
        );
        assert!(!lease.allows(
            &McpReadPolicy::default(),
            &context(&BTreeSet::from(["AI-shareable".into()])),
            &[0; 32],
            150
        ));
    }

    fn context<'a>(tags: &'a BTreeSet<String>) -> ClipAccessContext<'a> {
        ClipAccessContext {
            kind: ContentKind::Text,
            tags,
            collection: Some("work"),
            sensitive: false,
            concealed: false,
            sync_eligible: true,
        }
    }

    #[test]
    fn empty_filters_deny_and_mcp_requires_explicit_shareable_tag() {
        let tags = BTreeSet::from(["AI-shareable".into()]);
        assert!(!ClipAccessFilter::default().allows(&context(&tags)));
        let filter = ClipAccessFilter {
            kinds: BTreeSet::from([ContentKind::Text]),
            tags: BTreeSet::from(["AI-shareable".into()]),
            collections: BTreeSet::new(),
        };
        assert!(filter.allows(&context(&tags)));
        assert!(McpReadPolicy::default().allows(&context(&tags)));
        let mut sensitive = context(&tags);
        sensitive.sensitive = true;
        assert!(!filter.allows(&sensitive));
        assert!(!McpReadPolicy::default().allows(&sensitive));
        assert!(!format!("{filter:?}").contains("AI-shareable"));
        assert!(!format!("{sensitive:?}").contains("AI-shareable"));
    }

    #[test]
    fn mcp_lease_requires_consent_expiry_and_exact_policy() {
        let tags = BTreeSet::from(["AI-shareable".into()]);
        let policy = McpReadPolicy::default();
        let session_key = [7; 32];
        let lease = McpSessionLease::issue(&policy, &session_key, 100, 1_000, true).unwrap();
        assert!(lease.allows(&policy, &context(&tags), &session_key, 500));
        assert!(!lease.allows(&policy, &context(&tags), &session_key, 1_100));
        let changed = McpReadPolicy {
            required_tag: "other".into(),
            ..policy.clone()
        };
        assert!(!lease.allows(&changed, &context(&tags), &session_key, 500));
        assert!(!lease.allows(&policy, &context(&tags), &[8; 32], 500));
        assert!(McpSessionLease::issue(&policy, &session_key, 100, 1_000, false).is_err());
        assert!(!format!("{lease:?}").contains("AI-shareable"));
    }

    #[test]
    fn access_contracts_reject_unbounded_or_malformed_labels() {
        let oversized = "x".repeat(MAX_TAG_BYTES + 1);
        let filter = ClipAccessFilter {
            kinds: BTreeSet::from([ContentKind::Text]),
            tags: BTreeSet::from([oversized.clone()]),
            collections: BTreeSet::new(),
        };
        let tags = BTreeSet::from([oversized]);
        assert!(filter.validate().is_err());
        assert!(!filter.allows(&context(&tags)));

        let invalid_policy = McpReadPolicy {
            required_tag: "bad\nlabel".into(),
            ..McpReadPolicy::default()
        };
        assert!(!invalid_policy.allows(&context(&tags)));
    }
}
