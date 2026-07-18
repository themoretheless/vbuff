//! Typed encrypted artifacts that can accompany a replicated clip.

use serde::{Deserialize, Serialize};

use crate::crypto::SealedEnvelope;
use crate::policy::{DeviceLane, SyncContext, SyncPolicy, seal_if_allowed};
use crate::{Result, SyncError};

const MAX_EMBEDDING_DIMENSIONS: usize = 8_192;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingArtifact {
    pub content_hash: [u8; 32],
    pub backend_id: String,
    pub dimensions: u16,
    pub scale: f32,
    pub vector: Vec<i8>,
}

impl std::fmt::Debug for EmbeddingArtifact {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EmbeddingArtifact")
            .field("content_hash", &"[redacted]")
            .field("backend_id", &self.backend_id)
            .field("dimensions", &self.dimensions)
            .field("scale", &self.scale)
            .field(
                "vector",
                &format_args!("[redacted; {} values]", self.vector.len()),
            )
            .finish()
    }
}

impl EmbeddingArtifact {
    pub fn validate(&self) -> Result<()> {
        if self.backend_id.is_empty()
            || self.backend_id.len() > 128
            || !self
                .backend_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            || self.dimensions == 0
            || usize::from(self.dimensions) > MAX_EMBEDDING_DIMENSIONS
            || self.vector.len() != usize::from(self.dimensions)
            || !self.scale.is_finite()
            || self.scale <= 0.0
        {
            return Err(SyncError::Invalid("invalid embedding artifact".into()));
        }
        Ok(())
    }
}

pub fn seal_embedding_if_allowed(
    artifact: &EmbeddingArtifact,
    ai_allowed: bool,
    policy: &SyncPolicy,
    lane: &DeviceLane,
    context: &SyncContext,
    recipient_public_key: &[u8; 32],
    aad: &[u8],
) -> Result<Option<SealedEnvelope>> {
    if !ai_allowed || context.sensitive || !context.sync_eligible {
        return Ok(None);
    }
    artifact.validate()?;
    let plaintext =
        serde_json::to_vec(artifact).map_err(|error| SyncError::Invalid(error.to_string()))?;
    seal_if_allowed(policy, lane, context, recipient_public_key, &plaintext, aad)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use vbuff_types::ContentKind;
    use x25519_dalek::{PublicKey, StaticSecret};

    use crate::crypto::open_sealed;
    use crate::wire::unpad;

    use super::*;

    #[test]
    fn embeddings_are_gated_and_only_appear_inside_ciphertext() {
        let artifact = EmbeddingArtifact {
            content_hash: [9; 32],
            backend_id: "local-feature-hash-v1".into(),
            dimensions: 4,
            scale: 0.01,
            vector: vec![1, -2, 3, -4],
        };
        let policy = SyncPolicy::parse("allow kind=text target=phone").unwrap();
        let lane = DeviceLane {
            device_id: "phone".into(),
            kinds: BTreeSet::from([ContentKind::Text]),
            collections: BTreeSet::new(),
            max_bytes: 4096,
        };
        let context = SyncContext {
            kind: ContentKind::Text,
            byte_size: 4,
            source_app: None,
            target_device: "phone".into(),
            collection: None,
            sensitive: false,
            sync_eligible: true,
        };
        let secret = StaticSecret::from([7; 32]);
        let public = PublicKey::from(&secret).to_bytes();
        assert!(
            seal_embedding_if_allowed(
                &artifact,
                false,
                &policy,
                &lane,
                &context,
                &public,
                b"embedding"
            )
            .unwrap()
            .is_none()
        );
        let sealed = seal_embedding_if_allowed(
            &artifact,
            true,
            &policy,
            &lane,
            &context,
            &public,
            b"embedding",
        )
        .unwrap()
        .unwrap();
        assert!(
            !sealed
                .ciphertext
                .windows(artifact.backend_id.len())
                .any(|window| window == artifact.backend_id.as_bytes())
        );
        let opened = open_sealed(&secret, &sealed, b"embedding").unwrap();
        let decoded: EmbeddingArtifact = serde_json::from_slice(unpad(&opened).unwrap()).unwrap();
        assert_eq!(decoded, artifact);
    }
}
