//! Privacy-conscious structured facets extracted from canonical text.

use std::sync::OnceLock;

use regex::Regex;
use url::Url;
use vbuff_types::ContentKind;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Facet {
    pub key: String,
    pub value: String,
}

pub fn extract_facets(text: &str, kind: ContentKind, sensitive: bool) -> Vec<Facet> {
    let trimmed = text.trim();
    let mut facets = Vec::new();
    if let Ok(url) = Url::parse(trimmed)
        && let Some(host) = url.host_str()
    {
        facets.push(facet("host", host.to_lowercase()));
    }
    if matches!(kind, ContentKind::Color) {
        facets.push(facet("color", trimmed.to_lowercase()));
    }
    if matches!(kind, ContentKind::Code)
        && let Some(language) = detect_language(trimmed)
    {
        facets.push(facet("lang", language));
    }
    if iso_date_regex().is_match(trimmed) {
        facets.push(facet("iso_date", trimmed));
    }
    if !sensitive && has_payment_number(trimmed) {
        // Store only a boolean, never the number or BIN.
        facets.push(facet("has_payment_number", "true"));
    }
    facets.sort();
    facets.dedup();
    facets
}

fn facet(key: impl Into<String>, value: impl Into<String>) -> Facet {
    Facet {
        key: key.into(),
        value: value.into(),
    }
}

fn detect_language(text: &str) -> Option<&'static str> {
    if text.contains("fn ") && (text.contains("let ") || text.contains("impl ")) {
        Some("rust")
    } else if text.contains("def ") && text.contains(':') {
        Some("python")
    } else if text.contains("function ") || text.contains("=>") {
        Some("javascript")
    } else if text.contains("SELECT ") || text.contains("CREATE TABLE ") {
        Some("sql")
    } else {
        None
    }
}

fn iso_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^\d{4}-\d{2}-\d{2}(?:[T ][0-9:.+\-Z]+)?$").unwrap())
}

fn has_payment_number(text: &str) -> bool {
    let digits = text
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();
    (13..=19).contains(&digits.len()) && luhn_valid(&digits)
}

fn luhn_valid(digits: &str) -> bool {
    digits
        .bytes()
        .rev()
        .enumerate()
        .map(|(index, byte)| {
            let mut digit = u32::from(byte - b'0');
            if index % 2 == 1 {
                digit *= 2;
                if digit > 9 {
                    digit -= 9;
                }
            }
            digit
        })
        .sum::<u32>()
        % 10
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_indexable_facets_without_secret_values() {
        assert_eq!(
            extract_facets("https://docs.rs/rusqlite", ContentKind::Url, false),
            vec![facet("host", "docs.rs")]
        );
        assert_eq!(
            extract_facets("fn main() { let x = 1; }", ContentKind::Code, false),
            vec![facet("lang", "rust")]
        );
        let payment = extract_facets("4111 1111 1111 1111", ContentKind::Text, false);
        assert_eq!(payment, vec![facet("has_payment_number", "true")]);
        assert!(extract_facets("4111 1111 1111 1111", ContentKind::Text, true).is_empty());
    }
}
