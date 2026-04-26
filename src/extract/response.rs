use super::selector::{Element, ElementList, re_matches};
use crate::error::KumoError;
use bytes::Bytes;
use reqwest::header::HeaderMap;
use std::time::Duration;

pub(crate) enum ResponseBody {
    Text(String),
    Bytes(Bytes),
}

/// Wraps an HTTP response and provides ergonomic extraction methods.
pub struct Response {
    url: String,
    status: u16,
    headers: HeaderMap,
    /// Wall-clock time from sending the request to reading the full body.
    elapsed: Duration,
    pub(crate) body: ResponseBody,
}

impl Response {
    /// Construct a text `Response` from raw parts. Primarily useful in tests and examples.
    pub fn from_parts(url: impl Into<String>, status: u16, body: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            status,
            headers: HeaderMap::new(),
            elapsed: Duration::ZERO,
            body: ResponseBody::Text(body.into()),
        }
    }

    /// Override the elapsed duration on an existing response — useful in tests.
    pub fn with_elapsed(mut self, elapsed: Duration) -> Self {
        self.elapsed = elapsed;
        self
    }

    /// Construct a `Response` from all fields — used internally by fetchers.
    pub(crate) fn new(
        url: String,
        status: u16,
        headers: HeaderMap,
        elapsed: Duration,
        body: ResponseBody,
    ) -> Self {
        Self {
            url,
            status,
            headers,
            elapsed,
            body,
        }
    }

    /// Load a `Response` from a local HTML file. Useful in spider unit tests.
    ///
    /// Returns `Err` if the file cannot be read.
    pub fn from_file(
        url: impl Into<String>,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Self, std::io::Error> {
        let body = std::fs::read_to_string(path)?;
        Ok(Self::from_parts(url, 200, body))
    }

    /// Construct a binary `Response` from raw bytes.
    pub fn from_bytes(url: impl Into<String>, status: u16, bytes: Bytes) -> Self {
        Self {
            url: url.into(),
            status,
            headers: HeaderMap::new(),
            elapsed: Duration::ZERO,
            body: ResponseBody::Bytes(bytes),
        }
    }

    /// The URL of the fetched page.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// HTTP status code (e.g. 200, 404).
    pub fn status(&self) -> u16 {
        self.status
    }

    /// Response headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Wall-clock time from sending the request to reading the full body.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Returns the body as a UTF-8 string slice, or `None` if the body is binary.
    pub fn text(&self) -> Option<&str> {
        match &self.body {
            ResponseBody::Text(s) => Some(s.as_str()),
            ResponseBody::Bytes(_) => None,
        }
    }

    /// Returns the raw body bytes regardless of content type.
    pub fn bytes(&self) -> &[u8] {
        match &self.body {
            ResponseBody::Text(s) => s.as_bytes(),
            ResponseBody::Bytes(b) => b.as_ref(),
        }
    }

    /// Select elements in this page via a CSS selector.
    /// Returns an empty list if the body is binary.
    pub fn css(&self, selector: &str) -> ElementList {
        let Some(text) = self.text() else {
            return ElementList { elements: vec![] };
        };
        let Some(sel) = crate::extract::selector::get_selector(selector) else {
            return ElementList { elements: vec![] };
        };
        let document = scraper::Html::parse_document(text);
        let elements = document
            .select(&sel)
            .map(|el| Element {
                outer_html: el.html(),
            })
            .collect();
        ElementList { elements }
    }

    /// Deserialize the response body as JSON. Works for both text and binary bodies.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, KumoError> {
        serde_json::from_slice(self.bytes())
            .map_err(|e| KumoError::parse("json deserialization", e))
    }

    /// Apply a regex pattern to the raw response body and return all matches.
    /// Returns an empty Vec if the body is binary.
    pub fn re(&self, pattern: &str) -> Vec<String> {
        match self.text() {
            Some(t) => re_matches(t, pattern),
            None => vec![],
        }
    }

    /// Return the first regex match in the response body, or `None`.
    pub fn re_first(&self, pattern: &str) -> Option<String> {
        self.re(pattern).into_iter().next()
    }

    /// Evaluate a JSONPath expression against the response body parsed as JSON.
    #[cfg(feature = "jsonpath")]
    pub fn jsonpath(&self, expr: &str) -> Result<Vec<serde_json::Value>, KumoError> {
        use jsonpath_rust::JsonPath;

        let value: serde_json::Value =
            serde_json::from_slice(self.bytes()).map_err(|e| KumoError::parse("json body", e))?;
        let results = value
            .query(expr)
            .map_err(|e| KumoError::parse(format!("jsonpath '{expr}'"), e))?;
        Ok(results.into_iter().cloned().collect())
    }

    /// Evaluate an XPath 1.0 expression against the response body.
    ///
    /// Returns string values of all matched nodes:
    /// - **Element nodes** → outer HTML serialization
    /// - **Text nodes** → the text content (use `text()` axis in your expression)
    /// - **Attribute nodes** → the attribute value (use `@attr` in your expression)
    ///
    /// Returns an empty `Vec` on invalid expressions, no matches, or binary bodies.
    ///
    /// # Note
    ///
    /// The underlying HTML parser auto-inserts `<tbody>` inside `<table>` elements.
    /// Use `//table/tbody/tr/td` instead of `//table/tr/td`.
    ///
    /// Requires the `xpath` feature flag.
    ///
    /// # Examples
    /// ```rust,ignore
    /// res.xpath("//h1/text()")                         // all h1 text
    /// res.xpath("//a/@href")                           // all href values
    /// res.xpath(r#"//div[@class="price"]/text()"#)     // filtered elements
    /// res.xpath("//item/title/text()")                 // RSS feed titles
    /// ```
    #[cfg(feature = "xpath")]
    pub fn xpath(&self, expr: &str) -> Vec<String> {
        let Some(text) = self.text() else {
            return vec![];
        };

        let package = sxd_html::parse_html(text);
        let document = package.as_document();

        let value = match sxd_xpath::evaluate_xpath(&document, expr) {
            Ok(v) => v,
            Err(_) => return vec![],
        };

        match value {
            sxd_xpath::Value::Nodeset(nodeset) => nodeset
                .document_order()
                .into_iter()
                .filter_map(xpath_node_to_string)
                .collect(),
            sxd_xpath::Value::String(s) => vec![s],
            sxd_xpath::Value::Number(n) => vec![n.to_string()],
            sxd_xpath::Value::Boolean(b) => vec![b.to_string()],
        }
    }

    /// Return the first XPath match as a string, or `None`.
    /// Requires the `xpath` feature flag.
    #[cfg(feature = "xpath")]
    pub fn xpath_first(&self, expr: &str) -> Option<String> {
        self.xpath(expr).into_iter().next()
    }

    /// Resolve a relative URL against this response's URL.
    pub fn urljoin(&self, path: &str) -> String {
        use url::Url;
        Url::parse(&self.url)
            .and_then(|base| base.join(path))
            .map(|u| u.to_string())
            .unwrap_or_else(|_| path.to_string())
    }
}

// ── XPath helpers (only compiled with the "xpath" feature) ────────────────────

#[cfg(feature = "xpath")]
fn xpath_node_to_string(node: sxd_xpath::nodeset::Node<'_>) -> Option<String> {
    use sxd_xpath::nodeset::Node;
    match node {
        Node::Text(t) => Some(t.text().to_string()),
        Node::Attribute(a) => Some(a.value().to_string()),
        Node::Element(e) => Some(xpath_element_to_html(e)),
        Node::Root(_) | Node::Comment(_) | Node::ProcessingInstruction(_) | Node::Namespace(_) => {
            None
        }
    }
}

#[cfg(feature = "xpath")]
fn xpath_element_to_html(el: sxd_document::dom::Element<'_>) -> String {
    let name = el.name().local_part();
    let attrs: String = el
        .attributes()
        .iter()
        .map(|a| format!(r#" {}="{}""#, a.name().local_part(), a.value()))
        .collect();
    let children: String = el
        .children()
        .iter()
        .filter_map(xpath_child_to_html)
        .collect();
    format!("<{name}{attrs}>{children}</{name}>")
}

#[cfg(feature = "xpath")]
fn xpath_child_to_html(child: &sxd_document::dom::ChildOfElement<'_>) -> Option<String> {
    use sxd_document::dom::ChildOfElement;
    match child {
        ChildOfElement::Element(e) => Some(xpath_element_to_html(*e)),
        ChildOfElement::Text(t) => Some(t.text().to_string()),
        ChildOfElement::Comment(_) | ChildOfElement::ProcessingInstruction(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_response(body: &str) -> Response {
        Response::from_parts("https://example.com", 200, body)
    }

    #[test]
    fn text_body_is_accessible() {
        let res = make_response("hello");
        assert_eq!(res.text(), Some("hello"));
        assert_eq!(res.bytes(), b"hello");
    }

    #[test]
    fn binary_body_text_returns_none() {
        let res = Response::from_bytes(
            "https://example.com",
            200,
            Bytes::from_static(b"\x89PNG\r\n"),
        );
        assert!(res.text().is_none());
        assert_eq!(&res.bytes()[..4], b"\x89PNG");
    }

    #[test]
    fn response_re_returns_matches() {
        let res = make_response("items: 5, total: 100");
        assert_eq!(res.re(r"\d+"), vec!["5", "100"]);
    }

    #[test]
    fn response_re_returns_capture_group_one() {
        let res = make_response("price: $42");
        assert_eq!(res.re(r"\$(\d+)"), vec!["42"]);
    }

    #[test]
    fn response_re_first_returns_first() {
        let res = make_response("1 and 2");
        assert_eq!(res.re_first(r"\d+"), Some("1".to_string()));
    }

    #[test]
    fn response_re_first_returns_none_when_no_match() {
        let res = make_response("no digits");
        assert_eq!(res.re_first(r"\d+"), None);
    }

    #[test]
    fn binary_body_re_returns_empty() {
        let res = Response::from_bytes("https://example.com", 200, Bytes::from_static(b"\xff\xfe"));
        assert!(res.re(r"\d+").is_empty());
    }

    #[cfg(feature = "jsonpath")]
    #[test]
    fn response_jsonpath_returns_values() {
        let res = make_response(r#"{"books":[{"title":"A"},{"title":"B"}]}"#);
        let vals = res.jsonpath("$.books[*].title").unwrap();
        let titles: Vec<&str> = vals.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(titles, vec!["A", "B"]);
    }

    #[cfg(feature = "jsonpath")]
    #[test]
    fn response_jsonpath_invalid_json_returns_error() {
        let res = make_response("not json");
        assert!(res.jsonpath("$.foo").is_err());
    }

    #[cfg(feature = "jsonpath")]
    #[test]
    fn response_jsonpath_invalid_path_returns_error() {
        let res = make_response(r#"{"a":1}"#);
        assert!(res.jsonpath("!!!bad").is_err());
    }

    #[cfg(feature = "xpath")]
    mod xpath_tests {
        use super::*;

        fn page(html: &str) -> Response {
            Response::from_parts("https://example.com", 200, html)
        }

        #[test]
        fn xpath_selects_element_text() {
            let res = page("<html><body><h1>Hello</h1></body></html>");
            let results = res.xpath("//h1");
            assert_eq!(results.len(), 1);
            assert!(results[0].contains("Hello"), "got: {:?}", results[0]);
        }

        #[test]
        fn xpath_extracts_attribute_value() {
            let res = page(r#"<html><body><a href="/next">Next</a></body></html>"#);
            let results = res.xpath("//a/@href");
            assert_eq!(results, vec!["/next"]);
        }

        #[test]
        fn xpath_extracts_text_node() {
            let res = page("<html><body><p>Hello world</p></body></html>");
            let results = res.xpath("//p/text()");
            assert_eq!(results, vec!["Hello world"]);
        }

        #[test]
        fn xpath_returns_multiple_matches() {
            let res = page("<html><body><ul><li>a</li><li>b</li><li>c</li></ul></body></html>");
            let results = res.xpath("//li/text()");
            assert_eq!(results, vec!["a", "b", "c"]);
        }

        #[test]
        fn xpath_returns_empty_on_no_match() {
            let res = page("<html><body><p>no span here</p></body></html>");
            assert!(res.xpath("//span").is_empty());
        }

        #[test]
        fn xpath_returns_empty_on_invalid_expr() {
            let res = page("<html><body></body></html>");
            assert!(res.xpath("!!!bad xpath").is_empty());
        }

        #[test]
        fn xpath_returns_empty_for_binary_body() {
            let res = Response::from_bytes(
                "https://example.com",
                200,
                bytes::Bytes::from_static(b"\xff\xfe"),
            );
            assert!(res.xpath("//p").is_empty());
        }

        #[test]
        fn xpath_first_returns_first_match() {
            let res = page("<html><body><p>one</p><p>two</p></body></html>");
            assert_eq!(res.xpath_first("//p/text()"), Some("one".to_string()));
        }

        #[test]
        fn xpath_first_returns_none_on_no_match() {
            let res = page("<html><body></body></html>");
            assert_eq!(res.xpath_first("//span"), None);
        }

        #[test]
        fn xpath_filtered_by_attribute() {
            let res = page(
                r#"<html><body>
                    <div class="price">$10</div>
                    <div class="title">Book</div>
                </body></html>"#,
            );
            let results = res.xpath(r#"//div[@class="price"]/text()"#);
            assert_eq!(results, vec!["$10"]);
        }
    }
}
