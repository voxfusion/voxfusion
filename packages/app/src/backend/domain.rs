//! Parsing, normalization, and validation of browser hostnames for per-site
//! dictionaries and styles.

use std::net::{Ipv4Addr, Ipv6Addr};

use idna::uts46::{AsciiDenyList, DnsLength, Hyphens, Uts46};
use url::{Host, Url};

/// Normalizes a user-entered domain/URL into the canonical hostname key stored
/// for per-site dictionaries and styles.
///
/// Uses the WHATWG URL parser (`url`) so full URLs and browser host rules match
/// real browsers, then enforces strict DNS label syntax via UTS #46 and
/// normalizes idempotently (`normalize_domain(normalize_domain(x)) ==
/// normalize_domain(x)`).
///
/// Accepts:
/// - Multi-label DNS domains (ASCII or Unicode IDNs, normalized to lowercase
///   ASCII/Punycode), with optional single trailing root dot
/// - `localhost` (and subdomains such as `app.localhost`)
/// - Single-label intranet hostnames when given as a full `http://`/`https://`
///   URL (such as `http://intranet/`)
/// - IPv4 and IPv6 addresses (IPv6 is canonicalized in bracketed form, e.g.
///   `[::1]`)
/// - Full `http://` and `https://` URLs (stripping scheme, credentials, port,
///   path, query, fragment, and leading `www.` labels while at least two labels
///   remain)
///
/// Returns `None` for empty strings, control characters, whitespace in the
/// authority, unsupported schemes, invalid ports, malformed labels, or bare
/// single-label hosts other than `localhost`.
pub fn normalize_domain(input: &str) -> Option<String> {
    let (has_scheme, host) = parse_browser_host(input)?;
    if host.starts_with('[') || host.parse::<Ipv4Addr>().is_ok() {
        return Some(host);
    }

    if host == "localhost" {
        return Some(host);
    }

    if !host.contains('.') {
        // Allow valid single-label intranet hostnames in explicit URLs (such as
        // `http://intranet/`), while rejecting bare single-label text typos
        // such as `foo` or `www.`.
        return (has_scheme && host != "www").then_some(host);
    }

    let mut normalized = host.as_str();
    while let Some(rest) = normalized.strip_prefix("www.") {
        if rest.contains('.') {
            normalized = rest;
        } else {
            break;
        }
    }

    Some(normalized.to_string())
}

/// Normalizes a domain key for backend storage (`add_site_dictionary_word`,
/// `set_site_style`). Accepts everything [`normalize_domain`] accepts, plus an
/// already-normalized single-label intranet hostname (such as `intranet`,
/// emitted by the Add Site form from `http://intranet/`).
pub fn normalize_storage_domain(input: &str) -> Option<String> {
    if let Some(normalized) = normalize_domain(input) {
        return Some(normalized);
    }

    // Accept an already-canonicalized single-label DNS host (e.g. `intranet`
    // produced from `http://intranet/`), without accepting bare numbers,
    // `www.`, or malformed strings containing punctuation or whitespace.
    let trimmed = input.trim();
    if trimmed == input
        && !trimmed.contains(['.', ':', '/', '?', '#', '@', '\\'])
        && let Some(normalized) = normalize_domain(&format!("http://{trimmed}/"))
        && normalized == trimmed
    {
        return Some(normalized);
    }

    None
}

/// Extracts the canonicalized hostname from a browser tab URL for runtime
/// lookup without stripping `www.` and without rejecting single-label intranet
/// hostnames (such as `http://intranet/`), so [`domain_lookup_groups`] can
/// match both current canonical keys and legacy stored keys.
pub fn browser_domain(input: &str) -> Option<String> {
    parse_browser_host(input).map(|(_, host)| host)
}

/// Returns ordered candidate key groups for matching a browser hostname.
///
/// Each inner `Vec` represents the equivalent spellings of one site key (for
/// example canonical Punycode plus its legacy Unicode IDN spelling). Groups are
/// checked in order so more specific legacy keys (such as `www.example.com` for
/// `https://www.www.example.com/`) take precedence over broader fallbacks
/// (`example.com`) without mixing words or styles from distinct site entries.
pub fn domain_lookup_groups(domain: &str) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut push_group = |candidate: String| {
        if candidate.is_empty() {
            return;
        }
        let mut group = vec![candidate.clone()];
        if candidate.contains("xn--") {
            let (unicode, result) = idna::domain_to_unicode(&candidate);
            if result.is_ok() && !unicode.is_empty() && unicode != candidate {
                group.push(unicode);
            }
        }
        if !groups.iter().any(|existing| existing[0] == group[0]) {
            groups.push(group);
        }
    };

    let host = browser_domain(domain);
    let canonical = normalize_domain(domain);

    if let Some(ref host) = host {
        let one_prefix = host.strip_prefix("www.").unwrap_or(host);
        // When the browser visited `www.www.example.com`, prefer the more
        // specific one-prefix key (`www.example.com`) over `example.com`; when
        // it visited `www.localhost`, prefer `www.localhost` over legacy
        // `localhost`.
        if canonical.as_deref().is_some_and(|c| one_prefix.len() > c.len()) {
            push_group(one_prefix.to_string());
            if let Some(canonical) = canonical {
                push_group(canonical);
            }
        } else {
            if let Some(canonical) = canonical {
                push_group(canonical);
            }
            if !one_prefix.is_empty() {
                push_group(one_prefix.to_string());
            }
        }
    } else if let Some(canonical) = canonical {
        push_group(canonical);
    } else if !domain.is_empty() {
        push_group(domain.to_string());
    }

    groups
}

fn parse_browser_host(input: &str) -> Option<(bool, String)> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return None;
    }

    // Bare IPv6 address with no scheme, brackets, port, or path.
    if let Ok(ipv6) = trimmed.parse::<Ipv6Addr>() {
        return Some((false, format!("[{ipv6}]")));
    }

    let (has_scheme, url, schemeless_host) = parse_web_url(trimmed)?;

    let host = match url.host()? {
        Host::Ipv6(ipv6) => format!("[{ipv6}]"),
        Host::Ipv4(ipv4) => {
            if !has_scheme {
                // Without an explicit scheme, require standard dotted-quad IPv4
                // (with optional single trailing root dot) so bare numbers such
                // as `12345` or `1.2` are not silently turned into IP addresses.
                let raw_host = schemeless_host?;
                let candidate = raw_host.strip_suffix('.').unwrap_or(raw_host);
                candidate.parse::<Ipv4Addr>().ok()?;
            }
            ipv4.to_string()
        }
        Host::Domain(raw_domain) => normalize_ascii_domain(raw_domain)?,
    };

    Some((has_scheme, host))
}

fn normalize_ascii_domain(raw_domain: &str) -> Option<String> {
    let ascii = Uts46::new()
        .to_ascii(
            raw_domain.as_bytes(),
            AsciiDenyList::STD3,
            Hyphens::CheckFirstLast,
            DnsLength::VerifyAllowRootDot,
        )
        .ok()?;
    let domain = ascii.strip_suffix('.').unwrap_or(&ascii);
    (!domain.is_empty()).then(|| domain.to_string())
}

/// Parses `trimmed` as either an explicit `http://`/`https://` URL or a
/// schemeless web host/URL, returning `(has_scheme, parsed_url, schemeless_host)`.
fn parse_web_url(trimmed: &str) -> Option<(bool, Url, Option<&str>)> {
    if let Some(idx) = trimmed.find("://")
        && !trimmed[..idx].contains(['/', '?', '#'])
    {
        let scheme = &trimmed[..idx];
        if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
            return None;
        }
        let remainder = &trimmed[idx + 3..];
        let authority = remainder.split(['/', '?', '#']).next().unwrap_or("");
        if remainder.starts_with('/') || authority.ends_with(':') {
            return None;
        }
        let url = Url::parse(trimmed).ok()?;
        return Some((true, url, None));
    }

    if trimmed.starts_with(['/', '?', '#', ':']) {
        return None;
    }

    let authority = trimmed.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty()
        || authority.chars().any(char::is_whitespace)
        || authority.contains(['@', '\\'])
    {
        return None;
    }

    let schemeless_host = if let Some(after_open) = authority.strip_prefix('[') {
        let (_, after_close) = after_open.split_once(']')?;
        if !after_close.is_empty() {
            let port = after_close.strip_prefix(':')?;
            if !is_valid_port(port) {
                return None;
            }
        }
        authority
    } else if let Some((host, port)) = authority.split_once(':') {
        if host.is_empty() || !is_valid_port(port) {
            return None;
        }
        host
    } else {
        authority
    };

    let url = Url::parse(&format!("https://{trimmed}")).ok()?;
    Some((false, url, Some(schemeless_host)))
}

fn is_valid_port(port: &str) -> bool {
    !port.is_empty()
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u16>().is_ok()
}
