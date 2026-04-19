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
    pub url: String,
    pub status: u16,
    pub headers: HeaderMap,
    /// Wall-clock time from sending the request to reading the full body.
    pub elapsed: Duration,
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
        let document = scraper::Html::parse_document(text);
        let Ok(sel) = scraper::Selector::parse(selector) else {
            return ElementList { elements: vec![] };
        };
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
        serde_json::from_slice(self.bytes()).map_err(|e| KumoError::Parse(e.to_string()))
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

        let value: serde_json::Value = serde_json::from_slice(self.bytes())
            .map_err(|e| KumoError::Parse(format!("invalid JSON body: {e}")))?;
        let results = value
            .query(expr)
            .map_err(|e| KumoError::Parse(format!("invalid JSONPath '{expr}': {e}")))?;
        Ok(results.into_iter().cloned().collect())
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
}
