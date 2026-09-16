pub mod ojs;
pub mod nasati;
pub mod biomedical;
pub mod cs_ai;
pub mod open_repos;
pub mod keyed;
pub mod ai_search;
pub mod public_apis;

/// Error for a keyed source that answered with an authentication failure. The key
/// exists but the provider rejected it, so the remedy is the same as a missing
/// key — fix it in Settings — and it must not be reported as an unresponsive site.
pub fn keyed_http_error(label: &str, status: reqwest::StatusCode) -> String {
    if matches!(status.as_u16(), 401 | 403) {
        return format!(
            "{}{}: the provider rejected the saved credential (HTTP {})",
            NEEDS_SETUP,
            label,
            status.as_u16()
        );
    }
    format!("{}: HTTP {}", label, status)
}

/// Turns a reqwest transport error into something a reader can act on. The raw
/// Display value is just "error sending request for url (…)", which hides whether
/// the site timed out, refused the connection or failed its TLS handshake.
pub fn transport_reason(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return "the site did not respond in time".to_string();
    }
    if error.is_connect() {
        let cause = std::error::Error::source(error)
            .map(|source| source.to_string())
            .unwrap_or_default();
        let lowered = cause.to_lowercase();
        if lowered.contains("ssl") || lowered.contains("tls") || lowered.contains("certificate") {
            return "the site's HTTPS handshake failed".to_string();
        }
        if lowered.contains("dns") || lowered.contains("resolve") {
            return "the site's address could not be resolved".to_string();
        }
        return "the site refused the connection".to_string();
    }
    if error.is_body() || error.is_decode() {
        return "the response was cut short".to_string();
    }
    std::error::Error::source(error)
        .map(|source| source.to_string())
        .unwrap_or_else(|| error.to_string())
}

/// Prefix marking an error that means "this source needs credentials or a URL",
/// as opposed to a source that was reachable but failed. The engine strips it and
/// reports the source as needing setup so the UI does not call it unresponsive.
pub const NEEDS_SETUP: &str = "needs-setup: ";

/// Strips HTML tags and decodes common HTML entities
pub fn clean_html_text(raw: &str) -> String {
    let unescaped = raw
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&#160;", " ")
        .replace("&#8211;", "-")
        .replace("&#8212;", "-")
        .replace("&#8216;", "'")
        .replace("&#8217;", "'")
        .replace("&#8220;", "\"")
        .replace("&#8221;", "\"");

    // Remove tags
    let mut in_tag = false;
    let mut out = String::with_capacity(unescaped.len());
    for ch in unescaped.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(ch);
        }
    }

    // Collapse extra whitespaces
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse_author_list(raw: &str) -> Vec<String> {
    let cleaned = clean_html_text(raw);
    if cleaned.is_empty() {
        return Vec::new();
    }

    cleaned
        .split([',', ';'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}


#[cfg(test)]
mod tests {
    use super::*;

    // A saved-but-rejected key is a settings problem, not a dead site. Reporting it
    // as a failure told the user to wait for a source that will never recover on
    // its own.
    #[test]
    fn rejected_credentials_are_reported_as_needing_setup() {
        for status in [401u16, 403] {
            let message = keyed_http_error("scopus", reqwest::StatusCode::from_u16(status).unwrap());
            assert!(message.starts_with(NEEDS_SETUP), "{message}");
            assert!(message.contains("rejected the saved credential"), "{message}");
            assert!(message.contains(&status.to_string()), "{message}");
        }
    }

    #[test]
    fn other_http_failures_stay_plain_failures() {
        for status in [429u16, 500, 503] {
            let message = keyed_http_error("ieee", reqwest::StatusCode::from_u16(status).unwrap());
            assert!(!message.starts_with(NEEDS_SETUP), "{message}");
            assert!(message.starts_with("ieee: HTTP"), "{message}");
        }
    }
}
