use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PrivacyPostureInput {
    pub encryption_at_rest: bool,
    pub strict_local_only: bool,
    pub sensitive_memory_only: bool,
    pub telemetry_enabled: bool,
    pub sync_enabled: bool,
    pub denied_source_count: u32,
    pub retention_days: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyScoreLevel {
    Strong,
    Balanced,
    NeedsAttention,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PrivacyScoreFactor {
    pub key: &'static str,
    pub points: i8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PrivacyScore {
    pub value: u8,
    pub level: PrivacyScoreLevel,
    pub factors: Vec<PrivacyScoreFactor>,
}

impl PrivacyScore {
    pub fn calculate(input: PrivacyPostureInput) -> Self {
        let mut factors = Vec::with_capacity(7);
        factor(
            &mut factors,
            "encryption_at_rest",
            if input.encryption_at_rest { 24 } else { -24 },
        );
        factor(
            &mut factors,
            "strict_local_only",
            if input.strict_local_only { 18 } else { 0 },
        );
        factor(
            &mut factors,
            "sensitive_memory_only",
            if input.sensitive_memory_only { 16 } else { -8 },
        );
        factor(
            &mut factors,
            "telemetry",
            if input.telemetry_enabled { -10 } else { 8 },
        );
        factor(
            &mut factors,
            "sync",
            if input.sync_enabled { -8 } else { 8 },
        );
        factor(
            &mut factors,
            "denied_sources",
            if input.denied_source_count > 0 { 8 } else { 0 },
        );
        let retention_points = match input.retention_days {
            Some(0..=7) => 12,
            Some(8..=30) => 6,
            Some(31..=90) => 0,
            Some(_) => -8,
            None => -12,
        };
        factor(&mut factors, "retention", retention_points);

        let mut value = (50_i16
            + factors
                .iter()
                .map(|factor| i16::from(factor.points))
                .sum::<i16>())
        .clamp(0, 100) as u8;
        if (!input.encryption_at_rest || !input.sensitive_memory_only) && value > 54 {
            let adjustment = 54_i16 - i16::from(value);
            factor(&mut factors, "minimum_protection_gate", adjustment as i8);
            value = 54;
        }
        let level = match value {
            80..=100 => PrivacyScoreLevel::Strong,
            55..=79 => PrivacyScoreLevel::Balanced,
            _ => PrivacyScoreLevel::NeedsAttention,
        };
        Self {
            value,
            level,
            factors,
        }
    }
}

fn factor(output: &mut Vec<PrivacyScoreFactor>, key: &'static str, points: i8) {
    output.push(PrivacyScoreFactor { key, points });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_score_has_bounded_explainable_factors() {
        let posture = PrivacyPostureInput {
            encryption_at_rest: true,
            strict_local_only: true,
            sensitive_memory_only: true,
            telemetry_enabled: false,
            sync_enabled: false,
            denied_source_count: 2,
            retention_days: Some(7),
        };
        let score = PrivacyScore::calculate(posture);
        assert_eq!(score.value, 100);
        assert_eq!(score.level, PrivacyScoreLevel::Strong);
        assert_eq!(score.factors.len(), 7);
    }

    #[test]
    fn privacy_score_requires_encrypted_storage_for_strong_level() {
        let score = PrivacyScore::calculate(PrivacyPostureInput {
            encryption_at_rest: false,
            strict_local_only: true,
            sensitive_memory_only: true,
            telemetry_enabled: false,
            sync_enabled: false,
            denied_source_count: 2,
            retention_days: Some(7),
        });

        assert_eq!(score.value, 54);
        assert_eq!(score.level, PrivacyScoreLevel::NeedsAttention);
        assert_eq!(score.factors.last().unwrap().key, "minimum_protection_gate");
    }

    #[test]
    fn privacy_score_requires_sensitive_memory_only_for_strong_level() {
        let score = PrivacyScore::calculate(PrivacyPostureInput {
            encryption_at_rest: true,
            strict_local_only: true,
            sensitive_memory_only: false,
            telemetry_enabled: false,
            sync_enabled: false,
            denied_source_count: 2,
            retention_days: Some(7),
        });

        assert_eq!(score.value, 54);
        assert_eq!(score.level, PrivacyScoreLevel::NeedsAttention);
        assert_eq!(score.factors.last().unwrap().key, "minimum_protection_gate");
    }
}
