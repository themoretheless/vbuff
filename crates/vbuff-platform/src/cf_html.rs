//! Bounded parser for the Windows CF_HTML clipboard format.

use std::collections::BTreeSet;

const MAX_CF_HTML_BYTES: usize = 32 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_SOURCE_URL_BYTES: usize = 2 * 1024;
const START_MARKERS: [&[u8]; 2] = [b"<!--StartFragment-->", b"<!--StartFragment -->"];
const END_MARKERS: [&[u8]; 2] = [b"<!--EndFragment-->", b"<!--EndFragment -->"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CfHtmlError {
    TooLarge,
    MissingHeader,
    DuplicateField,
    InvalidOffset,
    InvalidRange,
    SourceUrlTooLarge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CfHtml<'a> {
    pub html: &'a [u8],
    pub fragment: &'a [u8],
    pub source_url: Option<&'a str>,
}

pub fn parse_cf_html(input: &[u8]) -> Result<CfHtml<'_>, CfHtmlError> {
    if input.len() > MAX_CF_HTML_BYTES {
        return Err(CfHtmlError::TooLarge);
    }
    let declared_start = declared_html_start(input)?;
    let header_end = match declared_start {
        Some(Some(start)) if start > 0 && start <= input.len() && start <= MAX_HEADER_BYTES => {
            start
        }
        Some(Some(_)) => return Err(CfHtmlError::InvalidOffset),
        Some(None) | None => fallback_header_end(input)?,
    };
    let header =
        std::str::from_utf8(&input[..header_end]).map_err(|_| CfHtmlError::MissingHeader)?;

    let mut seen = BTreeSet::new();
    let mut version_seen = false;
    let mut start_html = None;
    let mut end_html = None;
    let mut start_fragment = None;
    let mut end_fragment = None;
    let mut source_url = None;
    for line in header.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        let key = key.trim();
        let recognized = matches!(
            key,
            "Version" | "StartHTML" | "EndHTML" | "StartFragment" | "EndFragment" | "SourceURL"
        );
        if recognized && !seen.insert(key) {
            return Err(CfHtmlError::DuplicateField);
        }
        match key {
            "Version" => {
                if value.is_empty()
                    || value.len() > 16
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || byte == b'.')
                {
                    return Err(CfHtmlError::MissingHeader);
                }
                version_seen = true;
            }
            "StartHTML" => start_html = parse_offset(value)?,
            "EndHTML" => end_html = parse_offset(value)?,
            "StartFragment" => start_fragment = parse_offset(value)?,
            "EndFragment" => end_fragment = parse_offset(value)?,
            "SourceURL" => {
                if value.len() > MAX_SOURCE_URL_BYTES {
                    return Err(CfHtmlError::SourceUrlTooLarge);
                }
                source_url = Some(value);
            }
            _ => {}
        }
    }
    if !version_seen || (declared_start.is_some() && !seen.contains("StartHTML")) {
        return Err(CfHtmlError::MissingHeader);
    }

    let html_start = start_html.unwrap_or(header_end);
    let html_end = end_html.unwrap_or(input.len());
    let (fragment_start, fragment_end) = match (start_fragment, end_fragment) {
        (Some(start), Some(end)) => (start, end),
        (None, None) => fragment_offsets_from_markers(input, html_start, html_end)?,
        _ => return Err(CfHtmlError::InvalidOffset),
    };
    if html_start > fragment_start
        || fragment_start > fragment_end
        || fragment_end > html_end
        || html_end > input.len()
    {
        return Err(CfHtmlError::InvalidRange);
    }

    Ok(CfHtml {
        html: &input[html_start..html_end],
        fragment: &input[fragment_start..fragment_end],
        source_url,
    })
}

fn parse_offset(value: &str) -> Result<Option<usize>, CfHtmlError> {
    if value == "-1" {
        return Ok(None);
    }
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(CfHtmlError::InvalidOffset);
    }
    value
        .parse::<usize>()
        .map(Some)
        .map_err(|_| CfHtmlError::InvalidOffset)
}

fn declared_html_start(input: &[u8]) -> Result<Option<Option<usize>>, CfHtmlError> {
    let prefix = &input[..input.len().min(MAX_HEADER_BYTES)];
    let mut cursor = 0;
    while cursor < prefix.len() {
        let relative_end = prefix[cursor..]
            .iter()
            .position(|byte| matches!(*byte, b'\r' | b'\n'))
            .unwrap_or(prefix.len() - cursor);
        let line_end = cursor + relative_end;
        let line = &prefix[cursor..line_end];
        if let Some(value) = line.strip_prefix(b"StartHTML:") {
            let value = std::str::from_utf8(value).map_err(|_| CfHtmlError::InvalidOffset)?;
            return parse_offset(value.trim()).map(Some);
        }
        if line.starts_with(b"<") || line_end == prefix.len() {
            break;
        }
        cursor = line_end;
        while cursor < prefix.len() && matches!(prefix[cursor], b'\r' | b'\n') {
            cursor += 1;
        }
    }
    Ok(None)
}

fn fallback_header_end(input: &[u8]) -> Result<usize, CfHtmlError> {
    let prefix = &input[..input.len().min(MAX_HEADER_BYTES)];
    let crlf = prefix
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|start| (start, start + 4));
    let lf = prefix
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|start| (start, start + 2));
    let html = prefix
        .iter()
        .position(|byte| *byte == b'<')
        .map(|start| (start, start));
    [crlf, lf, html]
        .into_iter()
        .flatten()
        .min_by_key(|(start, _)| *start)
        .map(|(_, end)| end)
        .filter(|end| *end > 0)
        .ok_or(CfHtmlError::MissingHeader)
}

fn fragment_offsets_from_markers(
    input: &[u8],
    html_start: usize,
    html_end: usize,
) -> Result<(usize, usize), CfHtmlError> {
    if html_start > html_end || html_end > input.len() {
        return Err(CfHtmlError::InvalidRange);
    }
    let html = &input[html_start..html_end];
    let (start_offset, start_marker_len) =
        find_marker(html, &START_MARKERS).ok_or(CfHtmlError::InvalidOffset)?;
    let start = html_start + start_offset + start_marker_len;
    let end = find_marker(&input[start..html_end], &END_MARKERS)
        .map(|(offset, _)| start + offset)
        .ok_or(CfHtmlError::InvalidOffset)?;
    Ok((start, end))
}

fn find_marker(haystack: &[u8], markers: &[&[u8]]) -> Option<(usize, usize)> {
    markers
        .iter()
        .filter_map(|marker| {
            haystack
                .windows(marker.len())
                .position(|window| window == *marker)
                .map(|offset| (offset, marker.len()))
        })
        .min_by_key(|(offset, _)| *offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let html = b"<html><body><!--StartFragment--><b>hello</b><!--EndFragment--></body></html>";
        let placeholder = "Version:1.0\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\nSourceURL:https://example.test/\r\n\r\n";
        let start = placeholder.len();
        let fragment_relative = html
            .windows(START_MARKERS[0].len())
            .position(|window| window == START_MARKERS[0])
            .unwrap()
            + START_MARKERS[0].len();
        let fragment_end_relative = html
            .windows(END_MARKERS[0].len())
            .position(|window| window == END_MARKERS[0])
            .unwrap();
        let header = format!(
            "Version:1.0\r\nStartHTML:{start:010}\r\nEndHTML:{:010}\r\nStartFragment:{:010}\r\nEndFragment:{:010}\r\nSourceURL:https://example.test/\r\n\r\n",
            start + html.len(),
            start + fragment_relative,
            start + fragment_end_relative,
        );
        [header.as_bytes(), html].concat()
    }

    fn fixture_without_blank_separator() -> Vec<u8> {
        let html = b"<html><body><!--StartFragment --><i>spec</i><!--EndFragment --></body></html>";
        let placeholder = "Version:1.0\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:-1\r\nEndFragment:-1\r\n";
        let start = placeholder.len();
        let header = format!(
            "Version:1.0\r\nStartHTML:{start:010}\r\nEndHTML:{:010}\r\nStartFragment:-1\r\nEndFragment:-1\r\n",
            start + html.len(),
        );
        [header.as_bytes(), html].concat()
    }

    #[test]
    fn parses_byte_offsets_and_preserves_fragment_bytes() {
        let bytes = fixture();
        let parsed = parse_cf_html(&bytes).unwrap();
        assert_eq!(parsed.fragment, b"<b>hello</b>");
        assert_eq!(parsed.source_url, Some("https://example.test/"));
        assert!(parsed.html.starts_with(b"<html>"));
    }

    #[test]
    fn rejects_out_of_bounds_and_half_present_fragment_offsets() {
        let broken = b"Version:1.0\r\nStartHTML:0000000053\r\nEndHTML:9999999999\r\nStartFragment:-1\r\nEndFragment:0000000042\r\n\r\n<html/>";
        assert!(matches!(
            parse_cf_html(broken),
            Err(CfHtmlError::InvalidOffset | CfHtmlError::InvalidRange)
        ));
    }

    #[test]
    fn accepts_spec_header_without_blank_separator_and_spaced_markers() {
        let bytes = fixture_without_blank_separator();
        let parsed = parse_cf_html(&bytes).unwrap();
        assert_eq!(parsed.fragment, b"<i>spec</i>");
        assert!(parsed.html.starts_with(b"<html>"));
    }

    #[test]
    fn rejects_duplicate_offset_fields() {
        let bytes = b"Version:1.0\r\nStartHTML:-1\r\nStartHTML:-1\r\n\r\n<html><body><!--StartFragment-->x<!--EndFragment--></body></html>";
        assert_eq!(parse_cf_html(bytes), Err(CfHtmlError::DuplicateField));
    }
}
