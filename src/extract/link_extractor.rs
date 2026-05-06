mod builder;
mod domain;
mod extract;

pub use builder::LinkExtractor;

#[cfg(test)]
mod tests {
    use crate::extract::Response;

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
