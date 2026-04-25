use crate::extract::Response;
use regex::Regex;

/// Returns `true` if `url`'s host equals `domain` or is a subdomain of it.
fn host_matches_domain(url: &str, domain: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h == domain || h.ends_with(&format!(".{domain}")))
        })
        .unwrap_or(false)
}

/// Collects, filters, and deduplicates hyperlinks from a [`Response`].
///
/// Eliminates the boilerplate `res.css("a").iter().filter_map(|el| el.attr("href"))…`
/// pattern that every spider writes manually.
///
/// # Example
/// ```rust,ignore
/// let links = LinkExtractor::new()
///     .allow_domains(&["example.com"])   // stay on-site
///     .allow(r"/product/\d+")            // only product pages
///     .deny(r"\.pdf$")                   // skip PDF links
///     .canonicalize(true)                // collapse /page#s1 and /page#s2 → /page
///     .extract(&response);
///
/// Output::new().follow_many(links)
/// ```
pub struct LinkExtractor {
    allow: Vec<Regex>,
    deny: Vec<Regex>,
    restrict_css: Option<String>,
    canonicalize: bool,
    allow_domains: Vec<String>,
    deny_domains: Vec<String>,
    tags: Vec<String>,
    attrs: Vec<String>,
}

impl LinkExtractor {
    pub fn new() -> Self {
        Self {
            allow: vec![],
            deny: vec![],
            restrict_css: None,
            canonicalize: false,
            allow_domains: vec![],
            deny_domains: vec![],
            tags: vec!["a".into(), "area".into()],
            attrs: vec!["href".into()],
        }
    }

    /// Only keep URLs matching this regex. Multiple calls are OR-ed together.
    /// Panics if `pattern` is not a valid regex.
    pub fn allow(mut self, pattern: &str) -> Self {
        self.allow.push(
            Regex::new(pattern)
                .unwrap_or_else(|e| panic!("invalid allow pattern '{pattern}': {e}")),
        );
        self
    }

    /// Drop URLs matching this regex. Multiple calls are OR-ed together.
    /// Panics if `pattern` is not a valid regex.
    pub fn deny(mut self, pattern: &str) -> Self {
        self.deny.push(
            Regex::new(pattern).unwrap_or_else(|e| panic!("invalid deny pattern '{pattern}': {e}")),
        );
        self
    }

    /// Limit link extraction to elements inside the first element matching `selector`.
    pub fn restrict_css(mut self, selector: &str) -> Self {
        self.restrict_css = Some(selector.to_string());
        self
    }

    /// Strip URL fragments (`#section`) before deduplication so that
    /// `/page#s1` and `/page#s2` collapse to a single `/page` entry.
    /// Default: `false`.
    pub fn canonicalize(mut self, enabled: bool) -> Self {
        self.canonicalize = enabled;
        self
    }

    /// Only keep URLs whose host is `domain` or any subdomain of it.
    /// Multiple calls are OR-ed together (same as `allow`).
    ///
    /// Example: `allow_domains(&["example.com"])` accepts `example.com`
    /// and `www.example.com` but not `notexample.com`.
    pub fn allow_domains(mut self, domains: &[&str]) -> Self {
        self.allow_domains
            .extend(domains.iter().map(|d| d.to_string()));
        self
    }

    /// Drop URLs whose host is `domain` or any subdomain of it.
    /// Multiple calls are OR-ed together (same as `deny`).
    pub fn deny_domains(mut self, domains: &[&str]) -> Self {
        self.deny_domains
            .extend(domains.iter().map(|d| d.to_string()));
        self
    }

    /// Set which HTML tags to extract links from.
    /// Default: `["a", "area"]`.
    pub fn tags(mut self, tags: &[&str]) -> Self {
        self.tags = tags.iter().map(|t| t.to_string()).collect();
        self
    }

    /// Set which HTML attributes are treated as link sources.
    /// Default: `["href"]`.
    pub fn attrs(mut self, attrs: &[&str]) -> Self {
        self.attrs = attrs.iter().map(|a| a.to_string()).collect();
        self
    }

    /// Extract all links from `response`, resolve relative URLs, apply filters,
    /// and deduplicate. Returns absolute URLs in document order.
    pub fn extract(&self, response: &Response) -> Vec<String> {
        let Some(html) = response.text() else {
            return vec![];
        };
        let document = scraper::Html::parse_document(html);

        // Build selector like "a[href],area[href]" from configured tags + attrs.
        let selector_str = self
            .tags
            .iter()
            .flat_map(|tag| self.attrs.iter().map(move |attr| format!("{tag}[{attr}]")))
            .collect::<Vec<_>>()
            .join(",");
        let href_sel = scraper::Selector::parse(&selector_str).unwrap();

        // Optionally scope to a sub-element.
        let scope_html: Option<String> = self.restrict_css.as_deref().and_then(|css| {
            scraper::Selector::parse(css)
                .ok()
                .and_then(|sel| document.select(&sel).next().map(|el| el.html()))
        });

        let hrefs: Vec<String> = if let Some(ref scoped) = scope_html {
            let frag = scraper::Html::parse_fragment(scoped);
            frag.select(&href_sel)
                .filter_map(|el| {
                    self.attrs
                        .iter()
                        .find_map(|attr| el.value().attr(attr.as_str()))
                })
                .map(|h| response.urljoin(h))
                .collect()
        } else {
            document
                .select(&href_sel)
                .filter_map(|el| {
                    self.attrs
                        .iter()
                        .find_map(|attr| el.value().attr(attr.as_str()))
                })
                .map(|h| response.urljoin(h))
                .collect()
        };

        let mut seen = std::collections::HashSet::new();
        hrefs
            .into_iter()
            .map(|url| {
                if self.canonicalize {
                    url.find('#').map(|i| url[..i].to_string()).unwrap_or(url)
                } else {
                    url
                }
            })
            .filter(|url| {
                // allow_domains and allow regex are OR-ed: pass if EITHER matches.
                let allow_ok = self.allow.is_empty() && self.allow_domains.is_empty()
                    || self.allow.iter().any(|r| r.is_match(url))
                    || self
                        .allow_domains
                        .iter()
                        .any(|d| host_matches_domain(url, d));
                if !allow_ok {
                    return false;
                }
                // deny_domains and deny regex are OR-ed: drop if EITHER matches.
                if self.deny.iter().any(|r| r.is_match(url))
                    || self
                        .deny_domains
                        .iter()
                        .any(|d| host_matches_domain(url, d))
                {
                    return false;
                }
                seen.insert(url.clone())
            })
            .collect()
    }
}

impl Default for LinkExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_response(url: &str, html: &str) -> Response {
        Response::from_parts(url, 200, html)
    }

    #[test]
    fn extracts_all_links_by_default() {
        let res = make_response(
            "https://example.com/",
            r#"<a href="/a">A</a><a href="/b">B</a>"#,
        );
        let links = LinkExtractor::new().extract(&res);
        assert_eq!(
            links,
            vec!["https://example.com/a", "https://example.com/b"]
        );
    }

    #[test]
    fn resolves_relative_urls() {
        let res = make_response("https://example.com/page/1", r#"<a href="../2">next</a>"#);
        let links = LinkExtractor::new().extract(&res);
        assert_eq!(links, vec!["https://example.com/2"]);
    }

    #[test]
    fn allow_filter_keeps_matching_only() {
        let res = make_response(
            "https://example.com/",
            r#"<a href="/product/1">p</a><a href="/about">a</a>"#,
        );
        let links = LinkExtractor::new().allow(r"/product/").extract(&res);
        assert_eq!(links, vec!["https://example.com/product/1"]);
    }

    #[test]
    fn deny_filter_removes_matching() {
        let res = make_response(
            "https://example.com/",
            r#"<a href="/page">p</a><a href="/page.pdf">pdf</a>"#,
        );
        let links = LinkExtractor::new().deny(r"\.pdf$").extract(&res);
        assert_eq!(links, vec!["https://example.com/page"]);
    }

    #[test]
    fn deduplicates_links() {
        let res = make_response(
            "https://example.com/",
            r#"<a href="/a">1</a><a href="/a">2</a><a href="/b">3</a>"#,
        );
        let links = LinkExtractor::new().extract(&res);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0], "https://example.com/a");
        assert_eq!(links[1], "https://example.com/b");
    }

    #[test]
    fn restrict_css_scopes_search() {
        let res = make_response(
            "https://example.com/",
            r#"<nav><a href="/nav">nav</a></nav><footer><a href="/foot">foot</a></footer>"#,
        );
        let links = LinkExtractor::new().restrict_css("nav").extract(&res);
        assert_eq!(links, vec!["https://example.com/nav"]);
    }

    #[test]
    fn returns_empty_for_binary_response() {
        let res = Response::from_bytes(
            "https://example.com",
            200,
            bytes::Bytes::from_static(b"\xff\xfe"),
        );
        let links = LinkExtractor::new().extract(&res);
        assert!(links.is_empty());
    }

    #[test]
    fn allow_and_deny_combine() {
        let res = make_response(
            "https://example.com/",
            r#"<a href="/product/1">p1</a>
               <a href="/product/2.pdf">pdf</a>
               <a href="/about">about</a>"#,
        );
        let links = LinkExtractor::new()
            .allow(r"/product/")
            .deny(r"\.pdf$")
            .extract(&res);
        assert_eq!(links, vec!["https://example.com/product/1"]);
    }

    // ── canonicalize ─────────────────────────────────────────────────────────

    #[test]
    fn canonicalize_strips_fragments() {
        let res = make_response(
            "https://example.com/",
            r#"<a href="/page#s1">s1</a><a href="/page#s2">s2</a><a href="/page">p</a>"#,
        );
        let links = LinkExtractor::new().canonicalize(true).extract(&res);
        assert_eq!(links, vec!["https://example.com/page"]);
    }

    #[test]
    fn no_canonicalize_keeps_fragments_distinct() {
        let res = make_response(
            "https://example.com/",
            r#"<a href="/page#s1">s1</a><a href="/page#s2">s2</a>"#,
        );
        let links = LinkExtractor::new().canonicalize(false).extract(&res);
        assert_eq!(links.len(), 2);
    }

    // ── allow_domains / deny_domains ─────────────────────────────────────────

    #[test]
    fn allow_domains_keeps_matching_domain() {
        let res = make_response(
            "https://example.com/",
            r#"<a href="https://example.com/a">a</a>
               <a href="https://other.com/b">b</a>
               <a href="https://sub.example.com/c">c</a>"#,
        );
        let links = LinkExtractor::new()
            .allow_domains(&["example.com"])
            .extract(&res);
        assert_eq!(
            links,
            vec!["https://example.com/a", "https://sub.example.com/c"]
        );
    }

    #[test]
    fn deny_domains_removes_matching_domain() {
        let res = make_response(
            "https://example.com/",
            r#"<a href="https://example.com/a">a</a>
               <a href="https://ads.com/b">b</a>"#,
        );
        let links = LinkExtractor::new()
            .deny_domains(&["ads.com"])
            .extract(&res);
        assert_eq!(links, vec!["https://example.com/a"]);
    }

    #[test]
    fn allow_domains_and_allow_regex_are_or_ed() {
        let res = make_response(
            "https://example.com/",
            r#"<a href="https://example.com/page">page</a>
               <a href="https://cdn.other.com/img.png">img</a>
               <a href="https://third.com/x">x</a>"#,
        );
        let links = LinkExtractor::new()
            .allow_domains(&["example.com"])
            .allow(r"cdn\.other\.com")
            .extract(&res);
        assert_eq!(
            links,
            vec!["https://example.com/page", "https://cdn.other.com/img.png"]
        );
    }

    // ── tags / attrs ─────────────────────────────────────────────────────────

    #[test]
    fn extracts_from_area_tags_by_default() {
        let res = make_response(
            "https://example.com/",
            r#"<map><area href="/map-link"></map><a href="/a-link">a</a>"#,
        );
        let links = LinkExtractor::new().extract(&res);
        assert!(links.contains(&"https://example.com/map-link".to_string()));
        assert!(links.contains(&"https://example.com/a-link".to_string()));
    }

    #[test]
    fn tags_restricts_to_specified_tags_only() {
        let res = make_response(
            "https://example.com/",
            r#"<a href="/a-link">a</a><area href="/area-link">"#,
        );
        let links = LinkExtractor::new().tags(&["a"]).extract(&res);
        assert_eq!(links, vec!["https://example.com/a-link"]);
        assert!(!links.contains(&"https://example.com/area-link".to_string()));
    }

    #[test]
    fn attrs_extracts_from_custom_attribute() {
        let res = make_response(
            "https://example.com/",
            r#"<a data-href="/custom">x</a><a href="/normal">y</a>"#,
        );
        let links = LinkExtractor::new().attrs(&["data-href"]).extract(&res);
        assert_eq!(links, vec!["https://example.com/custom"]);
    }
}
