//! Small, opt-in local examples for an otherwise empty history.

use vbuff_gui::StarterPack;
use vbuff_types::{Clip, ClipId, ClipMeta, ContentKind, Flavor};

pub(crate) fn clips(pack: StarterPack) -> Vec<Clip> {
    let values: &[(&str, ContentKind)] = match pack {
        StarterPack::Developer => &[
            (
                "cargo test --workspace --all-features --locked",
                ContentKind::Code,
            ),
            (
                "cargo clippy --workspace --all-targets --all-features -- -D warnings",
                ContentKind::Code,
            ),
            ("cargo fmt --all -- --check", ContentKind::Code),
            ("vbuff doctor --json", ContentKind::Code),
        ],
        StarterPack::Writing => &[
            ("Summary\n\nDecision\n\nNext action", ContentKind::Text),
            (
                "Problem\n\nContext\n\nOptions\n\nRecommendation",
                ContentKind::Text,
            ),
            ("Added\n\nChanged\n\nFixed\n\nSecurity", ContentKind::Text),
        ],
    };
    values
        .iter()
        .map(|(text, kind)| text_clip(text, *kind))
        .collect()
}

fn text_clip(text: &str, kind: ContentKind) -> Clip {
    let flavors = vec![Flavor::inline("text/plain", text.as_bytes().to_vec())];
    Clip {
        id: ClipId::new(),
        content_hash: vbuff_core::content_hash_from_flavors(&flavors),
        flavors,
        meta: ClipMeta::now(kind, text.len() as u64, Some("vbuff.starter-pack".into())),
        pinned: false,
        favorite: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_are_small_local_plain_text_sets() {
        for pack in [StarterPack::Developer, StarterPack::Writing] {
            let clips = clips(pack);
            assert!(!clips.is_empty());
            assert!(clips.len() <= 4);
            assert!(clips.iter().all(|clip| clip.flavors.len() == 1));
            assert!(
                clips
                    .iter()
                    .all(|clip| clip.meta.source_app.as_deref() == Some("vbuff.starter-pack"))
            );
        }
    }
}
