//! Content-free diagnostic field types.

use std::fmt;

use vbuff_types::{Clip, ClipId, ContentKind};

/// A wrapper whose formatting can never reveal the wrapped value.
pub struct Sensitive<T>(pub T);

impl<T> fmt::Debug for Sensitive<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}

impl<T> fmt::Display for Sensitive<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}

/// The complete allow-list of clip fields permitted in structured tracing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RedactedClipFields<'a> {
    pub clip_id: ClipId,
    pub byte_size: u64,
    pub kind: ContentKind,
    pub source_app: Option<&'a str>,
    pub sensitive: bool,
}

impl<'a> From<&'a Clip> for RedactedClipFields<'a> {
    fn from(clip: &'a Clip) -> Self {
        Self {
            clip_id: clip.id,
            byte_size: clip.meta.byte_size,
            kind: clip.meta.kind,
            source_app: (!clip.meta.sensitive)
                .then_some(clip.meta.source_app.as_deref())
                .flatten(),
            sensitive: clip.meta.sensitive,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vbuff_types::{ClipMeta, Flavor};

    #[test]
    fn sensitive_formatting_never_contains_inner_value() {
        let secret = Sensitive("token-should-never-appear");
        assert_eq!(format!("{secret:?}"), "[redacted]");
        assert_eq!(format!("{secret}"), "[redacted]");
    }

    #[test]
    fn sensitive_clip_omits_source_metadata() {
        let mut meta = ClipMeta::now(ContentKind::Text, 6, Some("secret.app".into()));
        meta.sensitive = true;
        let clip = Clip {
            id: ClipId::new(),
            flavors: vec![Flavor::inline("text/plain", b"secret".to_vec())],
            content_hash: [0; 32],
            meta,
            pinned: false,
            favorite: false,
        };

        let fields = RedactedClipFields::from(&clip);
        assert_eq!(fields.source_app, None);
        assert!(!format!("{fields:?}").contains("secret"));
    }
}
