use chrono::{DateTime, Duration, NaiveDate, TimeZone as _, Utc};
use thiserror::Error;
use vbuff_types::ContentKind;

const MAX_QUERY_BYTES: usize = 4 * 1_024;
const MAX_QUERY_TOKENS: usize = 64;
const MAX_FILTER_VALUE_BYTES: usize = 512;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NaturalQuery {
    pub text: String,
    pub app: Option<String>,
    pub kind: Option<ContentKind>,
    pub tag: Option<String>,
    pub device: Option<String>,
    pub before: Option<DateTime<Utc>>,
    pub after: Option<DateTime<Utc>>,
    fingerprint: [u8; 32],
}

impl NaturalQuery {
    pub const fn fingerprint(&self) -> [u8; 32] {
        self.fingerprint
    }

    pub fn has_filters(&self) -> bool {
        self.app.is_some()
            || self.kind.is_some()
            || self.tag.is_some()
            || self.device.is_some()
            || self.before.is_some()
            || self.after.is_some()
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum QueryParseError {
    #[error("query is too large")]
    TooLarge,
    #[error("query syntax is invalid")]
    InvalidSyntax,
    #[error("query filter is invalid")]
    InvalidFilter,
}

pub fn parse_natural_query(raw: &str, now: DateTime<Utc>) -> Result<NaturalQuery, QueryParseError> {
    if raw.len() > MAX_QUERY_BYTES || raw.chars().any(|ch| ch == '\0') {
        return Err(QueryParseError::TooLarge);
    }
    let tokens = tokenize(raw)?;
    if tokens.len() > MAX_QUERY_TOKENS {
        return Err(QueryParseError::TooLarge);
    }
    let mut query = NaturalQuery::default();
    let mut text = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        if let Some((key, value)) = token.split_once(':')
            && matches!(
                key.to_ascii_lowercase().as_str(),
                "app" | "kind" | "tag" | "device" | "before" | "after"
            )
        {
            apply_facet(&mut query, key, value, now)?;
            index += 1;
            continue;
        }
        let lower = token.to_ascii_lowercase();
        if lower == "from" && index + 1 < tokens.len() {
            set_once(&mut query.app, &tokens[index + 1])?;
            index += 2;
            continue;
        }
        if let Some(kind) = natural_kind(&lower) {
            if query.kind.replace(kind).is_some() {
                return Err(QueryParseError::InvalidFilter);
            }
            index += 1;
            continue;
        }
        if lower == "today" {
            set_after(&mut query, start_of_day(now))?;
            index += 1;
            continue;
        }
        if lower == "yesterday" {
            let today = start_of_day(now);
            set_after(&mut query, today - Duration::days(1))?;
            set_before(&mut query, today)?;
            index += 1;
            continue;
        }
        if lower == "last" && index + 1 < tokens.len() {
            let duration = parse_relative_duration(&tokens[index + 1])?;
            set_after(&mut query, now - duration)?;
            index += 2;
            continue;
        }
        if lower == "before"
            && index + 1 < tokens.len()
            && tokens[index + 1].eq_ignore_ascii_case("lunch")
        {
            let date = now.date_naive();
            let lunch = date
                .and_hms_opt(12, 0, 0)
                .ok_or(QueryParseError::InvalidFilter)?;
            set_after(&mut query, start_of_day(now))?;
            set_before(&mut query, Utc.from_utc_datetime(&lunch))?;
            index += 2;
            continue;
        }
        text.push(token.clone());
        index += 1;
    }
    query.text = text.join(" ");
    if query
        .before
        .zip(query.after)
        .is_some_and(|(before, after)| before <= after)
    {
        return Err(QueryParseError::InvalidFilter);
    }
    query.fingerprint = fingerprint(&query);
    Ok(query)
}

fn tokenize(raw: &str) -> Result<Vec<String>, QueryParseError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for ch in raw.chars() {
        match ch {
            '"' => quoted = !quoted,
            ch if ch.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if quoted {
        return Err(QueryParseError::InvalidSyntax);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

fn apply_facet(
    query: &mut NaturalQuery,
    key: &str,
    value: &str,
    now: DateTime<Utc>,
) -> Result<(), QueryParseError> {
    if value.is_empty()
        || value.len() > MAX_FILTER_VALUE_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(QueryParseError::InvalidFilter);
    }
    match key.to_ascii_lowercase().as_str() {
        "app" => set_once(&mut query.app, value),
        "kind" => {
            let kind = parse_kind(value).ok_or(QueryParseError::InvalidFilter)?;
            if query.kind.replace(kind).is_some() {
                Err(QueryParseError::InvalidFilter)
            } else {
                Ok(())
            }
        }
        "tag" => set_once(&mut query.tag, value),
        "device" => set_once(&mut query.device, value),
        "before" => set_before(query, parse_date_or_relative(value, now)?),
        "after" => set_after(query, parse_date_or_relative(value, now)?),
        _ => Err(QueryParseError::InvalidFilter),
    }
}

fn set_once(slot: &mut Option<String>, value: &str) -> Result<(), QueryParseError> {
    if slot.is_some()
        || value.trim().is_empty()
        || value.len() > MAX_FILTER_VALUE_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(QueryParseError::InvalidFilter);
    }
    *slot = Some(value.to_ascii_lowercase());
    Ok(())
}

fn set_before(slot: &mut NaturalQuery, value: DateTime<Utc>) -> Result<(), QueryParseError> {
    if slot.before.replace(value).is_some() {
        return Err(QueryParseError::InvalidFilter);
    }
    Ok(())
}

fn set_after(slot: &mut NaturalQuery, value: DateTime<Utc>) -> Result<(), QueryParseError> {
    if slot.after.replace(value).is_some() {
        return Err(QueryParseError::InvalidFilter);
    }
    Ok(())
}

fn parse_date_or_relative(
    value: &str,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, QueryParseError> {
    match value.to_ascii_lowercase().as_str() {
        "today" => Ok(start_of_day(now)),
        "yesterday" => Ok(start_of_day(now) - Duration::days(1)),
        _ if value.starts_with("last-") => Ok(now - parse_relative_duration(&value[5..])?),
        _ => {
            let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| QueryParseError::InvalidFilter)?;
            let midnight = date
                .and_hms_opt(0, 0, 0)
                .ok_or(QueryParseError::InvalidFilter)?;
            Ok(Utc.from_utc_datetime(&midnight))
        }
    }
}

fn parse_relative_duration(value: &str) -> Result<Duration, QueryParseError> {
    let lower = value.to_ascii_lowercase();
    match lower.as_str() {
        "hour" => return Ok(Duration::hours(1)),
        "day" => return Ok(Duration::days(1)),
        "week" => return Ok(Duration::days(7)),
        _ => {}
    }
    let split = lower
        .find(|ch: char| !ch.is_ascii_digit())
        .ok_or(QueryParseError::InvalidFilter)?;
    let amount = lower[..split]
        .parse::<i64>()
        .map_err(|_| QueryParseError::InvalidFilter)?;
    if !(1..=365).contains(&amount) {
        return Err(QueryParseError::InvalidFilter);
    }
    match &lower[split..] {
        "m" | "min" => Ok(Duration::minutes(amount)),
        "h" | "hr" => Ok(Duration::hours(amount)),
        "d" | "day" | "days" => Ok(Duration::days(amount)),
        "w" | "week" | "weeks" => Ok(Duration::weeks(amount)),
        _ => Err(QueryParseError::InvalidFilter),
    }
}

fn natural_kind(value: &str) -> Option<ContentKind> {
    match value {
        "url" | "urls" | "link" | "links" => Some(ContentKind::Url),
        "image" | "images" | "picture" | "pictures" => Some(ContentKind::Image),
        "code" | "snippet" | "snippets" => Some(ContentKind::Code),
        "file" | "files" => Some(ContentKind::File),
        "color" | "colors" => Some(ContentKind::Color),
        _ => None,
    }
}

fn parse_kind(value: &str) -> Option<ContentKind> {
    natural_kind(&value.to_ascii_lowercase()).or_else(|| {
        match value.to_ascii_lowercase().as_str() {
            "text" => Some(ContentKind::Text),
            "html" => Some(ContentKind::Html),
            "rtf" => Some(ContentKind::Rtf),
            "other" => Some(ContentKind::Other),
            _ => None,
        }
    })
}

fn start_of_day(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.from_utc_datetime(
        &now.date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("valid UTC midnight"),
    )
}

fn fingerprint(query: &NaturalQuery) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vbuff-natural-query-v1");
    for value in [
        Some(query.text.as_str()),
        query.app.as_deref(),
        query.tag.as_deref(),
        query.device.as_deref(),
    ] {
        let value = value.unwrap_or_default().as_bytes();
        hasher.update(&(value.len() as u32).to_be_bytes());
        hasher.update(value);
    }
    hasher.update(&[query.kind.map_or(255, kind_code)]);
    hasher.update(
        &query
            .before
            .map_or(i64::MAX, |value| value.timestamp_millis())
            .to_be_bytes(),
    );
    hasher.update(
        &query
            .after
            .map_or(i64::MIN, |value| value.timestamp_millis())
            .to_be_bytes(),
    );
    *hasher.finalize().as_bytes()
}

const fn kind_code(kind: ContentKind) -> u8 {
    match kind {
        ContentKind::Text => 0,
        ContentKind::Url => 1,
        ContentKind::Color => 2,
        ContentKind::Code => 3,
        ContentKind::Image => 4,
        ContentKind::File => 5,
        ContentKind::Rtf => 6,
        ContentKind::Html => 7,
        ContentKind::Other => 8,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    #[test]
    fn natural_query_extracts_kind_app_and_relative_time() {
        let now = Utc.with_ymd_and_hms(2026, 7, 21, 18, 0, 0).unwrap();
        let parsed = parse_natural_query("urls from Chrome last week", now).unwrap();
        assert_eq!(parsed.kind, Some(ContentKind::Url));
        assert_eq!(parsed.app.as_deref(), Some("chrome"));
        assert_eq!(parsed.after, Some(now - Duration::days(7)));
        assert!(parsed.text.is_empty());
    }

    #[test]
    fn facets_quotes_and_before_lunch_are_deterministic() {
        let now = Utc.with_ymd_and_hms(2026, 7, 21, 18, 0, 0).unwrap();
        let parsed = parse_natural_query("\"release note\" app:Editor before lunch", now).unwrap();
        assert_eq!(parsed.text, "release note");
        assert_eq!(parsed.app.as_deref(), Some("editor"));
        assert_eq!(
            parsed.after,
            Some(Utc.with_ymd_and_hms(2026, 7, 21, 0, 0, 0).unwrap())
        );
        assert_eq!(
            parsed.before,
            Some(Utc.with_ymd_and_hms(2026, 7, 21, 12, 0, 0).unwrap())
        );
    }

    #[test]
    fn duplicate_and_unknown_filters_are_rejected() {
        let now = Utc.timestamp_opt(1_000, 0).unwrap();
        assert!(parse_natural_query("app:a app:b", now).is_err());
        assert_eq!(
            parse_natural_query("https://example.test", now)
                .unwrap()
                .text,
            "https://example.test"
        );
        assert!(parse_natural_query("kind:unknown", now).is_err());
        assert!(parse_natural_query("\"unterminated", now).is_err());
    }
}
