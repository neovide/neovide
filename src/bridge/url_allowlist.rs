/// Check if a URL/path matches a single wildcard pattern.
///
/// `*` matches any sequence of characters (including none).
/// All other characters are matched literally.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    // Split pattern on `*`, then verify each literal segment appears in order.
    let segments: Vec<&str> = pattern.split('*').collect();

    // If there are no wildcards, require exact match
    if segments.len() == 1 {
        return pattern == text;
    }

    let mut pos = 0;

    for (i, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            continue;
        }

        match text[pos..].find(segment) {
            Some(offset) => {
                // First segment must anchor to the start
                if i == 0 && offset != 0 {
                    return false;
                }
                pos += offset + segment.len();
            }
            None => return false,
        }
    }

    // The last segment is what comes after the final `*`.
    // If it's non-empty, the text must end with it (end anchor).
    // If it's empty (pattern ends with `*`), no end anchoring needed.
    let last = segments.last().expect("split always produces at least one segment");
    if !last.is_empty() {
        return text.ends_with(last);
    }
    true
}

/// Check if a URL is allowed by any of the patterns.
///
/// Returns `false` if patterns is `None` or empty (deny by default).
pub fn is_url_allowed(url: &str, patterns: &Option<Vec<String>>) -> bool {
    match patterns {
        None => false,
        Some(p) if p.is_empty() => false,
        Some(patterns) => patterns.iter().any(|p| wildcard_match(p, url)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert!(wildcard_match("hello", "hello"));
        assert!(!wildcard_match("hello", "world"));
    }

    #[test]
    fn test_wildcard_matches_all() {
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("*", ""));
    }

    #[test]
    fn test_wildcard_prefix() {
        assert!(wildcard_match("http://*", "http://example.com"));
        assert!(wildcard_match("http://*", "http://example.com/path/to/page"));
        assert!(!wildcard_match("http://*", "https://example.com"));
    }

    #[test]
    fn test_wildcard_suffix() {
        assert!(wildcard_match("*.txt", "file.txt"));
        assert!(wildcard_match("*.txt", "path/to/file.txt"));
        assert!(!wildcard_match("*.txt", "file.rs"));
    }

    #[test]
    fn test_wildcard_both_ends() {
        assert!(wildcard_match("http://*.com", "http://example.com"));
        assert!(wildcard_match("http://*.com", "http://www.example.com"));
        assert!(!wildcard_match("http://*.com", "http://example.org"));
    }

    #[test]
    fn test_multiple_wildcards() {
        assert!(wildcard_match("http://*/path/*", "http://example.com/path/to/page"));
        assert!(!wildcard_match("http://*/path/*", "http://example.com/other/page"));
    }

    #[test]
    fn test_anchoring() {
        // Pattern must match from start
        assert!(!wildcard_match("://*", "http://example.com"));
        // Pattern must match to end
        assert!(!wildcard_match("*.com/path", "http://example.com/path/extra"));
    }

    #[test]
    fn test_is_url_allowed() {
        let patterns = Some(vec!["https://*".to_string(), "http://*".to_string()]);
        assert!(is_url_allowed("https://example.com", &patterns));
        assert!(is_url_allowed("http://example.com/path", &patterns));
        assert!(!is_url_allowed("ftp://example.com", &patterns));

        // Deny by default
        assert!(!is_url_allowed("https://example.com", &None));
        assert!(!is_url_allowed("https://example.com", &Some(vec![])));
    }

    // --- Adversarial pattern examples ---
    //
    // These tests document some common but insecure patterns.

    #[test]
    fn adversarial_userinfo_spoof() {
        // Pattern intends: any github.com URL
        // Problem: `*` matches the `@`, so `github.com` appears as the userinfo
        // (username:password) prefix of a completely different host.
        assert!(wildcard_match(
            "https://github.com*",
            "https://github.com:foo@phishing-domain.com/",
        ));
        // Solution: always include the `/` after the domain name.
        assert!(!wildcard_match(
            "https://github.com/*",
            "https://github.com:foo@phishing-domain.com/",
        ));
    }

    #[test]
    fn adversarial_scheme() {
        // Pattern intends: any path on mydomain.com
        // Problem: the pattern matches any URL scheme
        assert!(wildcard_match(
            "*://mydomain.com/*",
            "mailto:hacker@phishing-domain.com?subject=://mydomain.com/&body=Get%20phished",
        ));
        // Solution: always start the pattern with the intended scheme
        assert!(!wildcard_match(
            "https://mydomain.com/*",
            "mailto:hacker@phishing-domain.com?subject=://mydomain.com/&body=Get%20phished",
        ));
    }

    #[test]
    fn adversarial_subdomains() {
        // Pattern intends: any path on *.mydomain.com
        // Problem: `*` matches across the `?` query boundary, so a phishing
        // domain can embed the literal substring in its query string.
        assert!(wildcard_match(
            "https://*.mydomain.com/*",
            "https://phishing-domain.com/?.mydomain.com/",
        ));
        // Solution: do not use wildcard subdomains.
        assert!(!wildcard_match(
            "https://allowed.mydomain.com/*",
            "https://phishing-domain.com/?.mydomain.com/",
        ));
    }
}
