/// Returns `true` if `url`'s host equals `domain` or is a subdomain of it.
pub(super) fn host_matches_domain(url: &str, domain: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h == domain || h.ends_with(&format!(".{domain}")))
        })
        .unwrap_or(false)
}
