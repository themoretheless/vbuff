use thiserror::Error;
use vbuff_types::ClipMeta;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiOperation {
    Embed,
    Classify,
    Explain,
    DetectPii,
    CaptionImage,
    SuggestTag,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum AiRefusal {
    #[error("clip was not explicitly admitted by the capture gate")]
    NotAllowed,
    #[error("sensitive clips cannot enter inference")]
    Sensitive,
    #[error("network inference requires a separate explicit opt-in")]
    NetworkNotAllowed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AiGate {
    pub network_opt_in: bool,
}

impl AiGate {
    pub fn authorize(
        self,
        meta: &ClipMeta,
        _operation: AiOperation,
        requires_network: bool,
    ) -> Result<(), AiRefusal> {
        if meta.sensitive {
            return Err(AiRefusal::Sensitive);
        }
        if !meta.ai_allowed {
            return Err(AiRefusal::NotAllowed);
        }
        if requires_network && !self.network_opt_in {
            return Err(AiRefusal::NetworkNotAllowed);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::{ClipMeta, ContentKind};

    #[test]
    fn gate_fails_closed_for_legacy_sensitive_and_network_inputs() {
        let mut meta = ClipMeta::now(ContentKind::Text, 1, None);
        assert_eq!(
            AiGate::default().authorize(&meta, AiOperation::Embed, false),
            Err(AiRefusal::NotAllowed)
        );
        meta.ai_allowed = true;
        assert!(
            AiGate::default()
                .authorize(&meta, AiOperation::Embed, false)
                .is_ok()
        );
        assert_eq!(
            AiGate::default().authorize(&meta, AiOperation::Embed, true),
            Err(AiRefusal::NetworkNotAllowed)
        );
        meta.sensitive = true;
        assert_eq!(
            AiGate {
                network_opt_in: true
            }
            .authorize(&meta, AiOperation::Embed, false),
            Err(AiRefusal::Sensitive)
        );
    }
}
