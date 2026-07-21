//! One reviewed clean-link implementation shared by UI and plugin surfaces.

use thiserror::Error;
use url::Url;

const MAX_URL_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CleanLinkError {
    #[error("URL input is empty or exceeds the byte limit")]
    InvalidSize,
    #[error("clean links require an HTTP(S) URL")]
    UnsupportedUrl,
}

pub fn clean_link(input: &str) -> Result<String, CleanLinkError> {
    let input = input.trim();
    if input.is_empty() || input.len() > MAX_URL_BYTES {
        return Err(CleanLinkError::InvalidSize);
    }
    let mut url = Url::parse(input).map_err(|_| CleanLinkError::UnsupportedUrl)?;
    if !is_supported(&url) {
        return Err(CleanLinkError::UnsupportedUrl);
    }
    if let Some(unwrapped) = known_redirect_target(&url)? {
        url = unwrapped;
    }
    let retained = url
        .query_pairs()
        .filter(|(key, _)| !is_tracking_parameter(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    if !retained.is_empty() {
        url.query_pairs_mut().extend_pairs(retained);
    }
    url.set_fragment(None);
    Ok(url.to_string())
}

fn known_redirect_target(url: &Url) -> Result<Option<Url>, CleanLinkError> {
    let Some(host) = url.host_str() else {
        return Ok(None);
    };
    let host = host.to_ascii_lowercase();
    let parameter = match (host.as_str(), url.path()) {
        ("www.google.com" | "google.com", "/url") => "q",
        ("l.facebook.com", "/l.php") => "u",
        ("out.reddit.com", _) => "url",
        _ => return Ok(None),
    };
    let target = url
        .query_pairs()
        .find_map(|(key, value)| (key == parameter).then(|| value.into_owned()))
        .ok_or(CleanLinkError::UnsupportedUrl)?;
    let target = Url::parse(&target).map_err(|_| CleanLinkError::UnsupportedUrl)?;
    if !is_supported(&target) {
        return Err(CleanLinkError::UnsupportedUrl);
    }
    Ok(Some(target))
}

fn is_supported(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
}

fn is_tracking_parameter(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("utm_")
        || key.starts_with("mc_")
        || matches!(
            key.as_str(),
            "fbclid" | "gclid" | "dclid" | "msclkid" | "igshid" | "ref_src" | "ref_url"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_link_unwraps_known_redirects_without_fetching() {
        assert_eq!(
            clean_link(
                "https://www.google.com/url?q=https%3A%2F%2Fexample.test%2Fdoc%3Fa%3D1%26utm_source%3Dx&fbclid=y"
            )
            .unwrap(),
            "https://example.test/doc?a=1"
        );
        assert_eq!(
            clean_link("https://example.test/path?keep=1&utm_campaign=x#private").unwrap(),
            "https://example.test/path?keep=1"
        );
        assert_eq!(
            clean_link("file:///tmp/private"),
            Err(CleanLinkError::UnsupportedUrl)
        );
        assert_eq!(
            clean_link("https://user:password@example.test/private"),
            Err(CleanLinkError::UnsupportedUrl)
        );
        assert_eq!(
            clean_link("https://www.google.com/url?q=https%3A%2F%2Fuser%3Apassword%40example.test"),
            Err(CleanLinkError::UnsupportedUrl)
        );
    }
}
