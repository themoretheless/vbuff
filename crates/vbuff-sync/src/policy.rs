//! Selective-sync policy DSL and asymmetric per-device lanes.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use vbuff_types::ContentKind;

use crate::crypto::{SealedEnvelope, seal_to};
use crate::wire::pad_to_bucket;
use crate::{Result, SyncError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncDecision {
    Allow,
    Deny,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncContext {
    pub kind: ContentKind,
    pub byte_size: u64,
    pub source_app: Option<String>,
    pub target_device: String,
    pub collection: Option<String>,
    pub sensitive: bool,
    pub sync_eligible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Rule {
    decision: SyncDecision,
    kind: Option<ContentKind>,
    max_bytes: Option<u64>,
    source_contains: Option<String>,
    target: Option<String>,
    collection: Option<String>,
}

impl Rule {
    fn matches(&self, context: &SyncContext) -> bool {
        self.kind.is_none_or(|kind| kind == context.kind)
            && self
                .max_bytes
                .is_none_or(|maximum| context.byte_size <= maximum)
            && self.source_contains.as_ref().is_none_or(|needle| {
                context
                    .source_app
                    .as_deref()
                    .is_some_and(|source| source.to_lowercase().contains(&needle.to_lowercase()))
            })
            && self
                .target
                .as_ref()
                .is_none_or(|target| target == &context.target_device)
            && self
                .collection
                .as_ref()
                .is_none_or(|collection| context.collection.as_deref() == Some(collection.as_str()))
    }
}

#[derive(Clone, Debug, Default)]
pub struct SyncPolicy {
    rules: Vec<Rule>,
}

impl SyncPolicy {
    /// One rule per line, e.g. `allow kind=text max_bytes=4096 target=phone`.
    pub fn parse(source: &str) -> Result<Self> {
        let mut rules = Vec::new();
        for (line_number, raw) in source.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let mut tokens = line.split_whitespace();
            let decision = match tokens.next() {
                Some("allow") => SyncDecision::Allow,
                Some("deny") => SyncDecision::Deny,
                _ => {
                    return Err(SyncError::Invalid(format!(
                        "policy line {} must start with allow or deny",
                        line_number + 1
                    )));
                }
            };
            let mut rule = Rule {
                decision,
                kind: None,
                max_bytes: None,
                source_contains: None,
                target: None,
                collection: None,
            };
            for token in tokens {
                let (key, value) = token
                    .split_once('=')
                    .ok_or_else(|| SyncError::Invalid(format!("invalid policy token {token:?}")))?;
                match key {
                    "kind" => rule.kind = Some(parse_kind(value)?),
                    "max_bytes" => {
                        rule.max_bytes = Some(value.parse().map_err(|_| {
                            SyncError::Invalid(format!("invalid max_bytes {value:?}"))
                        })?)
                    }
                    "source_contains" => rule.source_contains = Some(value.into()),
                    "target" => rule.target = Some(value.into()),
                    "collection" => rule.collection = Some(value.into()),
                    _ => return Err(SyncError::Invalid(format!("unknown policy key {key:?}"))),
                }
            }
            rules.push(rule);
        }
        Ok(Self { rules })
    }

    pub fn evaluate(&self, context: &SyncContext) -> SyncDecision {
        if context.sensitive || !context.sync_eligible {
            return SyncDecision::Deny;
        }
        self.rules
            .iter()
            .find(|rule| rule.matches(context))
            .map_or(SyncDecision::Deny, |rule| rule.decision)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceLane {
    pub device_id: String,
    pub kinds: BTreeSet<ContentKind>,
    pub collections: BTreeSet<String>,
    pub max_bytes: u64,
}

impl DeviceLane {
    pub fn accepts(&self, context: &SyncContext) -> bool {
        self.device_id == context.target_device
            && self.kinds.contains(&context.kind)
            && context.byte_size <= self.max_bytes
            && (self.collections.is_empty()
                || context
                    .collection
                    .as_ref()
                    .is_some_and(|collection| self.collections.contains(collection)))
    }
}

pub fn seal_if_allowed(
    policy: &SyncPolicy,
    lane: &DeviceLane,
    context: &SyncContext,
    recipient_public_key: &[u8; 32],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Option<SealedEnvelope>> {
    if policy.evaluate(context) != SyncDecision::Allow || !lane.accepts(context) {
        return Ok(None);
    }
    let padded = pad_to_bucket(plaintext)?;
    Ok(Some(seal_to(recipient_public_key, &padded, aad)?))
}

fn parse_kind(value: &str) -> Result<ContentKind> {
    match value {
        "text" => Ok(ContentKind::Text),
        "url" => Ok(ContentKind::Url),
        "color" => Ok(ContentKind::Color),
        "code" => Ok(ContentKind::Code),
        "image" => Ok(ContentKind::Image),
        "file" => Ok(ContentKind::File),
        "rtf" => Ok(ContentKind::Rtf),
        "html" => Ok(ContentKind::Html),
        "other" => Ok(ContentKind::Other),
        _ => Err(SyncError::Invalid(format!(
            "unknown content kind {value:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::open_sealed;
    use crate::wire::unpad;
    use x25519_dalek::{PublicKey, StaticSecret};

    fn context() -> SyncContext {
        SyncContext {
            kind: ContentKind::Text,
            byte_size: 12,
            source_app: Some("editor".into()),
            target_device: "phone".into(),
            collection: Some("pinned".into()),
            sensitive: false,
            sync_eligible: true,
        }
    }

    #[test]
    fn policy_and_lane_are_enforced_before_encryption() {
        let policy = SyncPolicy::parse(
            "deny source_contains=1password\nallow kind=text max_bytes=4096 target=phone",
        )
        .unwrap();
        let lane = DeviceLane {
            device_id: "phone".into(),
            kinds: BTreeSet::from([ContentKind::Text]),
            collections: BTreeSet::from(["pinned".into()]),
            max_bytes: 4_096,
        };
        let secret = StaticSecret::from([8; 32]);
        let public = PublicKey::from(&secret).to_bytes();
        let sealed = seal_if_allowed(&policy, &lane, &context(), &public, b"clip", b"aad")
            .unwrap()
            .unwrap();
        let opened = open_sealed(&secret, &sealed, b"aad").unwrap();
        assert_eq!(unpad(&opened).unwrap(), b"clip");

        let mut sensitive = context();
        sensitive.sensitive = true;
        assert!(
            seal_if_allowed(&policy, &lane, &sensitive, &public, b"secret", b"aad")
                .unwrap()
                .is_none()
        );
    }
}
