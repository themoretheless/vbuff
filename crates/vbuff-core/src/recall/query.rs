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

/// Query-facing kind parser: UI synonyms ([`natural_kind`], e.g. "link",
/// "pictures", "snippets") layered over the canonical case-insensitive
/// [`ContentKind`] slug vocabulary (`FromStr` in `vbuff-types`).
fn parse_kind(value: &str) -> Option<ContentKind> {
    let lower = value.to_ascii_lowercase();
    natural_kind(&lower).or_else(|| lower.parse::<ContentKind>().ok())
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

/// Local domain of the query fingerprint ("vbuff-natural-query-v1"), not a
/// storage format: this numbering feeds only the blake3 hash above and is
/// never persisted. Do not unify it with
/// `ContentKind::stored_discriminant` without bumping the fingerprint
/// domain string, since renumbering silently changes every fingerprint.
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

/// Characterization suite for the recall query grammar.
///
/// These tests deliberately pin *current* behavior, including the parts that
/// look wrong, so that the planned unification behind a `FacetSpec` registry
/// (theme T1 in `docs/solid-dry-review-2026-07-26.md`) cannot change the
/// grammar silently. Every place where the recorded behavior is a quirk
/// rather than a designed rule is called out in a comment.
#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    /// Fixed clock for the whole suite: 2026-07-21 18:00:00 UTC.
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 21, 18, 0, 0).unwrap()
    }

    fn utc(year: i32, month: u32, day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).unwrap()
    }

    fn parse(raw: &str) -> NaturalQuery {
        parse_natural_query(raw, now()).expect("query should parse")
    }

    fn reject(raw: &str) -> QueryParseError {
        parse_natural_query(raw, now()).expect_err("query should be rejected")
    }

    #[test]
    fn every_supported_facet_populates_exactly_one_field() {
        // The closed facet vocabulary of this grammar. Anything outside this
        // list is not a facet here, whatever other vbuff parsers accept.
        let app = parse("app:Chrome");
        assert_eq!(app.app.as_deref(), Some("chrome"));
        assert!(app.text.is_empty());
        assert!(app.has_filters());

        assert_eq!(parse("kind:url").kind, Some(ContentKind::Url));
        assert_eq!(parse("tag:Work").tag.as_deref(), Some("work"));
        assert_eq!(parse("device:Laptop").device.as_deref(), Some("laptop"));
        assert_eq!(parse("before:2026-07-20").before, Some(utc(2026, 7, 20, 0)));
        assert_eq!(parse("after:2026-07-01").after, Some(utc(2026, 7, 1, 0)));

        // Facets are orthogonal: each one leaves the others untouched.
        let all = parse(
            "kind:url app:chrome tag:work device:laptop after:2026-07-01 before:2026-07-20 release",
        );
        assert_eq!(all.kind, Some(ContentKind::Url));
        assert_eq!(all.app.as_deref(), Some("chrome"));
        assert_eq!(all.tag.as_deref(), Some("work"));
        assert_eq!(all.device.as_deref(), Some("laptop"));
        assert_eq!(all.after, Some(utc(2026, 7, 1, 0)));
        assert_eq!(all.before, Some(utc(2026, 7, 20, 0)));
        assert_eq!(all.text, "release");
    }

    #[test]
    fn facet_keys_are_case_insensitive_and_values_are_ascii_lowercased() {
        assert_eq!(parse("APP:Foo").app.as_deref(), Some("foo"));
        assert_eq!(parse("KiNd:URL").kind, Some(ContentKind::Url));
        assert_eq!(parse("Tag:UrGeNt").tag.as_deref(), Some("urgent"));
        assert_eq!(parse("DEVICE:MacBook").device.as_deref(), Some("macbook"));
        assert_eq!(parse("BEFORE:2026-07-20").before, Some(utc(2026, 7, 20, 0)));
        assert_eq!(parse("After:today").after, Some(utc(2026, 7, 21, 0)));
    }

    #[test]
    fn kind_facet_accepts_canonical_slugs_and_ui_synonyms() {
        for (raw, expected) in [
            ("kind:text", ContentKind::Text),
            ("kind:url", ContentKind::Url),
            ("kind:color", ContentKind::Color),
            ("kind:code", ContentKind::Code),
            ("kind:image", ContentKind::Image),
            ("kind:file", ContentKind::File),
            // Rtf/Html/Other are parseable even though completion hides them.
            ("kind:rtf", ContentKind::Rtf),
            ("kind:html", ContentKind::Html),
            ("kind:other", ContentKind::Other),
            // UI synonyms layered on top of the canonical slugs.
            ("kind:urls", ContentKind::Url),
            ("kind:link", ContentKind::Url),
            ("kind:links", ContentKind::Url),
            ("kind:images", ContentKind::Image),
            ("kind:picture", ContentKind::Image),
            ("kind:pictures", ContentKind::Image),
            ("kind:snippet", ContentKind::Code),
            ("kind:snippets", ContentKind::Code),
            ("kind:files", ContentKind::File),
            ("kind:colors", ContentKind::Color),
        ] {
            assert_eq!(parse(raw).kind, Some(expected), "kind facet {raw}");
        }
        assert_eq!(reject("kind:unknown"), QueryParseError::InvalidFilter);
        // No plural/synonym for Text: "texts" is not in the synonym table and
        // is not a canonical slug either.
        assert_eq!(reject("kind:texts"), QueryParseError::InvalidFilter);
    }

    #[test]
    fn quotes_are_stripped_and_keep_spaces_inside_facet_values() {
        assert_eq!(
            parse("app:\"Sublime Text\"").app.as_deref(),
            Some("sublime text")
        );
        assert_eq!(
            parse("tag:\"release notes\"").tag.as_deref(),
            Some("release notes")
        );
        // Quoting the whole token does NOT escape the facet: the quote
        // characters vanish before facet splitting, so this is still a facet.
        assert_eq!(
            parse("\"app:Sublime Text\"").app.as_deref(),
            Some("sublime text")
        );
        // Quotes toggle mid-token, so they can glue two words into one token.
        assert_eq!(parse("he\"llo wor\"ld").text, "hello world");
        // Inside quotes the original spacing survives; outside it collapses.
        assert_eq!(parse("\"a  b\"").text, "a  b");
        assert_eq!(parse("a  b").text, "a b");
        assert_eq!(reject("\"unterminated"), QueryParseError::InvalidSyntax);
    }

    #[test]
    fn foreign_store_grammar_facets_are_swallowed_as_free_text() {
        // `host|color|lang|iso_date` belong to the store-side grammar
        // (`vbuff-store/src/search.rs::parse_query`). This parser does not
        // know them, so they are neither honored nor reported: they end up
        // verbatim in the text term and silently widen the text match.
        let parsed = parse("host:example.com lang:rust iso_date:2026-07-20 color:#ff8800");
        assert_eq!(
            parsed.text,
            "host:example.com lang:rust iso_date:2026-07-20 color:#ff8800"
        );
        assert!(!parsed.has_filters());
        // Unknown keys keep their original case, unlike facet values.
        assert_eq!(parse("host:Example.COM").text, "host:Example.COM");
        // A key that merely starts with a known facet name is not a facet.
        assert_eq!(parse("xapp:foo").text, "xapp:foo");
        assert_eq!(parse("apps:foo").text, "apps:foo");
        // An empty key is not a facet either.
        assert_eq!(parse(":value").text, ":value");
        // A bare facet name without a colon is plain text.
        assert_eq!(parse("app").text, "app");
    }

    #[test]
    fn empty_or_blank_facet_values_are_rejected_rather_than_treated_as_text() {
        for raw in ["app:", "kind:", "tag:", "device:", "before:", "after:"] {
            assert_eq!(
                parse_natural_query(raw, now()),
                Err(QueryParseError::InvalidFilter),
                "bare facet {raw}"
            );
        }
        // Quoted whitespace passes the is_empty gate but fails the trim gate.
        assert_eq!(reject("app:\"   \""), QueryParseError::InvalidFilter);
        // One bad facet poisons the whole query; there is no partial parse.
        assert_eq!(reject("release app: notes"), QueryParseError::InvalidFilter);
    }

    #[test]
    fn repeating_a_facet_is_rejected_across_both_sub_grammars() {
        for raw in [
            "app:a app:b",
            "app:a from b",
            "from a app:b",
            "kind:url kind:code",
            "kind:url links",
            "urls kind:url",
            "urls links",
            "tag:a tag:b",
            "device:a device:b",
            "before:2026-07-20 before:2026-07-19",
            "after:today after:yesterday",
            "today today",
            "yesterday yesterday",
            // "yesterday" already sets both bounds, so a second bound clashes.
            "yesterday before:2026-07-19",
            "today before lunch",
        ] {
            assert_eq!(
                parse_natural_query(raw, now()),
                Err(QueryParseError::InvalidFilter),
                "duplicate filter {raw}"
            );
        }
    }

    #[test]
    fn non_ascii_facet_values_are_left_unchanged_by_ascii_lowercasing() {
        // `to_ascii_lowercase` only touches A-Z, so a Cyrillic capital is
        // stored as typed. See the search-side tests for why that matters:
        // `recall/search.rs` lowercases the clip side with the Unicode-aware
        // `to_lowercase`, so the two normalizations disagree.
        assert_eq!(parse("app:Продукт").app.as_deref(), Some("Продукт"));
        assert_eq!(parse("app:продукт").app.as_deref(), Some("продукт"));
        // Mixed scripts: the ASCII half is lowered, the Cyrillic half is not.
        assert_eq!(
            parse("app:Chrome-Продукт").app.as_deref(),
            Some("chrome-Продукт")
        );
        assert_eq!(parse("device:Ноутбук").device.as_deref(), Some("Ноутбук"));
        assert_eq!(parse("tag:Работа").tag.as_deref(), Some("Работа"));
        // Non-ASCII free text is preserved verbatim, including case.
        assert_eq!(parse("Заметки о релизе").text, "Заметки о релизе");
    }

    #[test]
    fn date_facets_accept_iso_dates_and_relative_keywords() {
        assert_eq!(parse("before:2026-07-20").before, Some(utc(2026, 7, 20, 0)));
        assert_eq!(parse("after:today").after, Some(utc(2026, 7, 21, 0)));
        assert_eq!(parse("before:yesterday").before, Some(utc(2026, 7, 20, 0)));
        assert_eq!(
            parse("after:last-2h").after,
            Some(now() - Duration::hours(2))
        );
        assert_eq!(
            parse("after:last-30m").after,
            Some(now() - Duration::minutes(30))
        );
        assert_eq!(
            parse("after:last-3d").after,
            Some(now() - Duration::days(3))
        );
        assert_eq!(
            parse("after:last-1w").after,
            Some(now() - Duration::weeks(1))
        );
        assert_eq!(
            parse("after:last-hour").after,
            Some(now() - Duration::hours(1))
        );
        assert_eq!(
            parse("after:last-day").after,
            Some(now() - Duration::days(1))
        );
        assert_eq!(
            parse("after:last-week").after,
            Some(now() - Duration::days(7))
        );
        assert_eq!(
            parse("after:last-365d").after,
            Some(now() - Duration::days(365))
        );

        for raw in [
            "after:tomorrow",
            "after:2026-13-01",
            "after:20260720",
            "after:2026-07-20T10:00:00Z",
            "after:last-",
            "after:last-5",
            "after:last-0d",
            "after:last-366d",
            "after:last-5s",
            "after:last-1mo",
        ] {
            assert_eq!(
                parse_natural_query(raw, now()),
                Err(QueryParseError::InvalidFilter),
                "date facet {raw}"
            );
        }
    }

    #[test]
    fn relative_date_prefix_is_case_sensitive_although_the_keywords_are_not() {
        // Quirk: `parse_date_or_relative` matches "today"/"yesterday" on the
        // lowercased value but tests the "last-" prefix on the raw value, so
        // capitalizing only that form breaks it.
        assert_eq!(parse("after:TODAY").after, Some(utc(2026, 7, 21, 0)));
        assert_eq!(parse("before:YESTERDAY").before, Some(utc(2026, 7, 20, 0)));
        assert_eq!(
            parse("after:last-2H").after,
            Some(now() - Duration::hours(2))
        );
        assert_eq!(reject("after:LAST-2h"), QueryParseError::InvalidFilter);
    }

    #[test]
    fn bare_kind_words_are_consumed_out_of_the_free_text() {
        for (raw, expected) in [
            ("url", ContentKind::Url),
            ("urls", ContentKind::Url),
            ("link", ContentKind::Url),
            ("links", ContentKind::Url),
            ("image", ContentKind::Image),
            ("images", ContentKind::Image),
            ("picture", ContentKind::Image),
            ("pictures", ContentKind::Image),
            ("code", ContentKind::Code),
            ("snippet", ContentKind::Code),
            ("snippets", ContentKind::Code),
            ("file", ContentKind::File),
            ("files", ContentKind::File),
            ("color", ContentKind::Color),
            ("colors", ContentKind::Color),
        ] {
            let parsed = parse(raw);
            assert_eq!(parsed.kind, Some(expected), "bare kind word {raw}");
            assert!(parsed.text.is_empty(), "bare kind word {raw} left text");
        }
        assert_eq!(parse("FILES").kind, Some(ContentKind::File));
        // Consequence: an ordinary noun disappears from the text term.
        let parsed = parse("meeting notes files");
        assert_eq!(parsed.kind, Some(ContentKind::File));
        assert_eq!(parsed.text, "meeting notes");
        // "text" is not a synonym, so it stays in the text term.
        let plain = parse("text");
        assert_eq!(plain.kind, None);
        assert_eq!(plain.text, "text");
    }

    #[test]
    fn from_keyword_consumes_the_next_token_verbatim() {
        let parsed = parse("notes from Chrome");
        assert_eq!(parsed.app.as_deref(), Some("chrome"));
        assert_eq!(parsed.text, "notes");
        assert_eq!(parse("notes FROM Chrome").app.as_deref(), Some("chrome"));
        // Quirk: the token after "from" is taken as-is, so a facet that
        // follows "from" is eaten as an app name instead of being parsed.
        let swallowed = parse("notes from kind:url");
        assert_eq!(swallowed.app.as_deref(), Some("kind:url"));
        assert_eq!(swallowed.kind, None);
        assert_eq!(swallowed.text, "notes");
        // A trailing "from" has nothing to consume and stays as text.
        let trailing = parse("notes from");
        assert_eq!(trailing.app, None);
        assert_eq!(trailing.text, "notes from");
    }

    #[test]
    fn today_yesterday_and_last_window_semantics() {
        let today = parse("today");
        assert_eq!(today.after, Some(utc(2026, 7, 21, 0)));
        assert_eq!(today.before, None);

        let yesterday = parse("yesterday");
        assert_eq!(yesterday.after, Some(utc(2026, 7, 20, 0)));
        assert_eq!(yesterday.before, Some(utc(2026, 7, 21, 0)));

        assert_eq!(parse("last week").after, Some(now() - Duration::days(7)));
        assert_eq!(parse("last hour").after, Some(now() - Duration::hours(1)));
        assert_eq!(parse("last 3d").after, Some(now() - Duration::days(3)));
        assert_eq!(parse("last 2w").after, Some(now() - Duration::weeks(2)));

        // Quirk: "last" plus an unparseable unit fails the whole query instead
        // of degrading to text, so a natural phrase like "last night" is a
        // hard parse error.
        assert_eq!(reject("last night"), QueryParseError::InvalidFilter);
        assert_eq!(reject("notes last 5s"), QueryParseError::InvalidFilter);
        // A trailing "last" has nothing to consume and stays as text.
        assert_eq!(parse("notes last").text, "notes last");
    }

    #[test]
    fn before_lunch_is_a_two_token_special_case() {
        let parsed = parse("before lunch");
        assert_eq!(parsed.after, Some(utc(2026, 7, 21, 0)));
        assert_eq!(parsed.before, Some(utc(2026, 7, 21, 12)));
        assert_eq!(parse("before LUNCH").before, Some(utc(2026, 7, 21, 12)));
        // Only "lunch" is special; every other follower degrades to text.
        let dinner = parse("before dinner");
        assert!(!dinner.has_filters());
        assert_eq!(dinner.text, "before dinner");
        assert_eq!(parse("before").text, "before");
    }

    #[test]
    fn contradictory_time_windows_are_rejected_including_the_empty_one() {
        // The window is rejected when before <= after, so a zero-width window
        // is an error rather than an empty result.
        assert_eq!(
            reject("after:2026-07-20 before:2026-07-20"),
            QueryParseError::InvalidFilter
        );
        assert_eq!(
            reject("after:2026-07-21 before:2026-07-20"),
            QueryParseError::InvalidFilter
        );
        assert!(parse_natural_query("after:2026-07-19 before:2026-07-20", now()).is_ok());
    }

    #[test]
    fn free_text_is_joined_by_single_spaces_and_keeps_its_case() {
        assert_eq!(parse("Release Notes").text, "Release Notes");
        assert_eq!(parse("  spaced   out  ").text, "spaced out");
        let empty = parse("");
        assert_eq!(empty.text, "");
        assert!(!empty.has_filters());
        assert_eq!(parse("   ").text, "");
        assert_eq!(parse("\"\"").text, "");
        // URLs survive: the scheme prefix is not a known facet key.
        assert_eq!(parse("https://example.test").text, "https://example.test");
    }

    #[test]
    fn facet_values_may_contain_further_colons() {
        // split_once splits on the FIRST colon only.
        assert_eq!(parse("app:a:b").app.as_deref(), Some("a:b"));
        assert_eq!(parse("tag:v1:2").tag.as_deref(), Some("v1:2"));
    }

    #[test]
    fn size_limits_fail_closed() {
        assert_eq!(
            reject(&"a".repeat(MAX_QUERY_BYTES + 1)),
            QueryParseError::TooLarge
        );
        assert!(parse_natural_query(&"a".repeat(MAX_QUERY_BYTES), now()).is_ok());

        let tokens = vec!["z"; MAX_QUERY_TOKENS].join(" ");
        assert!(parse_natural_query(&tokens, now()).is_ok());
        let too_many = vec!["z"; MAX_QUERY_TOKENS + 1].join(" ");
        assert_eq!(
            parse_natural_query(&too_many, now()),
            Err(QueryParseError::TooLarge)
        );

        assert_eq!(reject("hello\0world"), QueryParseError::TooLarge);

        let long_value = format!("app:{}", "a".repeat(MAX_FILTER_VALUE_BYTES));
        assert!(parse_natural_query(&long_value, now()).is_ok());
        let over_value = format!("app:{}", "a".repeat(MAX_FILTER_VALUE_BYTES + 1));
        assert_eq!(
            parse_natural_query(&over_value, now()),
            Err(QueryParseError::InvalidFilter)
        );
    }

    #[test]
    fn control_characters_are_rejected_in_facets_but_tolerated_in_text() {
        assert_eq!(reject("app:a\u{7}b"), QueryParseError::InvalidFilter);
        // A quoted tab reaches the facet value and trips the control check.
        assert_eq!(reject("app:\"a\tb\""), QueryParseError::InvalidFilter);
        // Free text has no such gate: only NUL is refused, at the size check.
        assert_eq!(parse("a\u{7}b").text, "a\u{7}b");
    }

    #[test]
    fn fingerprint_is_computed_over_normalized_fields_only() {
        // Case folding of facet values makes these the same cache key.
        assert_eq!(
            parse("app:Chrome").fingerprint(),
            parse("app:chrome").fingerprint()
        );
        // The two sub-grammars are fingerprint-equivalent when they agree.
        assert_eq!(
            parse("urls from chrome").fingerprint(),
            parse("kind:url app:chrome").fingerprint()
        );
        // Text is compared verbatim, so word order and case matter.
        assert_ne!(parse("a b").fingerprint(), parse("b a").fingerprint());
        assert_ne!(parse("Note").fingerprint(), parse("note").fingerprint());
        // The same value under a different facet is a different key.
        assert_ne!(
            parse("app:chrome").fingerprint(),
            parse("device:chrome").fingerprint()
        );
        // Relative windows are resolved against `now`, so the fingerprint of
        // the same raw query drifts with the clock.
        let later = now() + Duration::hours(1);
        assert_ne!(
            parse("last hour").fingerprint(),
            parse_natural_query("last hour", later)
                .unwrap()
                .fingerprint()
        );
    }

    #[test]
    fn has_filters_tracks_facets_and_ignores_the_text_term() {
        assert!(!parse("release notes").has_filters());
        assert!(parse("app:chrome").has_filters());
        assert!(parse("today").has_filters());
        assert!(parse("urls").has_filters());
    }

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
