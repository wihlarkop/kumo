use crate::extract::Response;
use regex::Regex;

/// Collects, filters, and deduplicates hyperlinks from a [`Response`].
///
/// Eliminates the boilerplate `res.css("a").iter().filter_map(|el| el.attr("href"))…`
/// pattern that every spider writes manually.
///
/// # Example
/// ```rust,ignore
/// let links = LinkExtractor::new()
///     .allow(r"/product/\d+")   // only product pages
///     .deny(r"\.pdf$")           // skip PDF links
///     .extract(&response);
///
/// Output::new().follow_many(links)
/// ```
pub struct LinkExtractor {
    allow: Vec<Regex>,
    deny: Vec<Regex>,
    restrict_css: Option<String>,
}

impl LinkExtractor {
    pub fn new() -> Self {
        Self {
            allow: vec![],
            deny: vec![],
            restrict_css: None,
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

    /// Extract all links from `response`, resolve relative URLs, apply filters,
    /// and deduplicate. Returns absolute URLs in document order.
    pub fn extract(&self, response: &Response) -> Vec<String> {
        let Some(html) = response.text() else {
            return vec![];
        };
        let document = scraper::Html::parse_document(html);
        let href_sel = scraper::Selector::parse("a[href]").unwrap();

        // Optionally scope to a sub-element.
        let scope_html: Option<String> = self.restrict_css.as_deref().and_then(|css| {
            scraper::Selector::parse(css)
                .ok()
                .and_then(|sel| document.select(&sel).next().map(|el| el.html()))
        });

        let hrefs: Vec<String> = if let Some(ref scoped) = scope_html {
            let frag = scraper::Html::parse_fragment(scoped);
            frag.select(&href_sel)
                .filter_map(|el| el.value().attr("href"))
                .map(|h| response.urljoin(h))
                .collect()
        } else {
            document
                .select(&href_sel)
                .filter_map(|el| el.value().attr("href"))
                .map(|h| response.urljoin(h))
                .collect()
        };

        let mut seen = std::collections::HashSet::new();
        hrefs
            .into_iter()
            .filter(|url| {
                if !self.allow.is_empty() && !self.allow.iter().any(|r| r.is_match(url)) {
                    return false;
                }
                if self.deny.iter().any(|r| r.is_match(url)) {
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
        // ../2 from /page/1 goes up to root, giving /2
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
}
