//! Pure rendering helpers for the popup (no eframe app state).

use chrono::{DateTime, Utc};

/// Format a timestamp as a compact relative time like `3m`, `2h`, `5d`.
pub fn relative_time(ts: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let delta = now.signed_duration_since(ts);
    let secs = delta.num_seconds();
    if secs < 0 {
        return "now".to_string();
    }
    if secs < 60 {
        return "now".to_string();
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{days}d");
    }
    let weeks = days / 7;
    if weeks < 5 {
        return format!("{weeks}w");
    }
    let months = days / 30;
    if months < 12 {
        return format!("{months}mo");
    }
    let years = days / 365;
    format!("{years}y")
}

/// Extract the short display name from a source-app identifier.
///
/// Turns `/Applications/Foo.app` or `com.example.foo` into `Foo`/`foo`.
pub fn short_app_name(source: &str) -> String {
    // Bundle id style: take the last dotted component.
    if source.contains('.')
        && !source.contains('/')
        && let Some(last) = source.rsplit('.').next()
    {
        return last.to_string();
    }
    // Path style: take the file stem, dropping a trailing `.app`.
    let last = source.rsplit(['/', '\\']).next().unwrap_or(source);
    last.strip_suffix(".app").unwrap_or(last).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn relative_time_buckets() {
        let now = Utc::now();
        assert_eq!(relative_time(now, now), "now");
        assert_eq!(relative_time(now - Duration::minutes(5), now), "5m");
        assert_eq!(relative_time(now - Duration::hours(3), now), "3h");
        assert_eq!(relative_time(now - Duration::days(2), now), "2d");
        assert_eq!(relative_time(now - Duration::days(10), now), "1w");
    }

    #[test]
    fn app_name_shortening() {
        assert_eq!(short_app_name("/Applications/Safari.app"), "Safari");
        assert_eq!(short_app_name("com.apple.Safari"), "Safari");
        assert_eq!(short_app_name("firefox"), "firefox");
    }
}
