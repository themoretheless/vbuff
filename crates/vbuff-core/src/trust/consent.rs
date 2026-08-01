use chrono::{DateTime, Utc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EphemeralCountdown {
    Expired,
    Remaining { seconds: u64 },
}

impl EphemeralCountdown {
    pub fn between(now: DateTime<Utc>, expires_at: DateTime<Utc>) -> Self {
        let remaining_ms = expires_at.signed_duration_since(now).num_milliseconds();
        if remaining_ms <= 0 {
            Self::Expired
        } else {
            Self::Remaining {
                seconds: (remaining_ms as u64).saturating_add(999) / 1_000,
            }
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Expired => "expired".into(),
            Self::Remaining { seconds } if seconds < 60 => format!("{seconds}s"),
            Self::Remaining { seconds } => {
                let minutes = seconds.saturating_add(59) / 60;
                format!("{minutes}m")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    #[test]
    fn countdown_rounds_up_and_expires_at_the_boundary() {
        let now = Utc.timestamp_opt(1_000, 0).unwrap();
        assert_eq!(
            EphemeralCountdown::between(now, now + chrono::Duration::milliseconds(1)).label(),
            "1s"
        );
        assert_eq!(EphemeralCountdown::between(now, now).label(), "expired");
        assert_eq!(
            EphemeralCountdown::between(now, now + chrono::Duration::seconds(61)).label(),
            "2m"
        );
    }
}
