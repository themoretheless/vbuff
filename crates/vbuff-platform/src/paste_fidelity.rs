//! End-to-end paste trace contract used by native sink harnesses.

use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PasteTrace {
    pub source_hash: [u8; 32],
    pub sink_hash: Option<[u8; 32]>,
    pub clipboard_write_succeeded: bool,
    pub target_confirmed: bool,
    pub injection_attempted: bool,
    pub pressed_modifiers: BTreeSet<String>,
    pub released_modifiers: BTreeSet<String>,
    pub leaked_modifiers: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PasteConformanceIssue {
    ClipboardWriteFailed,
    TargetNotConfirmed,
    InjectionMissing,
    SinkDidNotReceive,
    ContentMismatch,
    ModifierLeak,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteConformanceReport {
    pub issues: BTreeSet<PasteConformanceIssue>,
}

impl PasteTrace {
    pub fn verify(&self) -> PasteConformanceReport {
        let mut issues = BTreeSet::new();
        if !self.clipboard_write_succeeded {
            issues.insert(PasteConformanceIssue::ClipboardWriteFailed);
        }
        if !self.target_confirmed {
            issues.insert(PasteConformanceIssue::TargetNotConfirmed);
        }
        if !self.injection_attempted {
            issues.insert(PasteConformanceIssue::InjectionMissing);
        }
        match self.sink_hash {
            None => {
                issues.insert(PasteConformanceIssue::SinkDidNotReceive);
            }
            Some(hash) if hash != self.source_hash => {
                issues.insert(PasteConformanceIssue::ContentMismatch);
            }
            Some(_) => {}
        }
        if self.pressed_modifiers != self.released_modifiers || !self.leaked_modifiers.is_empty() {
            issues.insert(PasteConformanceIssue::ModifierLeak);
        }
        PasteConformanceReport { issues }
    }
}

impl PasteConformanceReport {
    pub fn is_lossless(&self) -> bool {
        self.issues.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sink_contract_detects_literal_or_modifier_corruption() {
        let hash = *blake3::hash(b"exact text").as_bytes();
        let clean = PasteTrace {
            source_hash: hash,
            sink_hash: Some(hash),
            clipboard_write_succeeded: true,
            target_confirmed: true,
            injection_attempted: true,
            pressed_modifiers: BTreeSet::from(["ctrl".into()]),
            released_modifiers: BTreeSet::from(["ctrl".into()]),
            leaked_modifiers: BTreeSet::new(),
        };
        assert!(clean.verify().is_lossless());

        let mut bad = clean;
        bad.sink_hash = Some(*blake3::hash(b"v").as_bytes());
        bad.released_modifiers.clear();
        bad.leaked_modifiers.insert("shift".into());
        let report = bad.verify();
        assert!(
            report
                .issues
                .contains(&PasteConformanceIssue::ContentMismatch)
        );
        assert!(report.issues.contains(&PasteConformanceIssue::ModifierLeak));
    }
}
