use super::selector::{re_matches, Element, ElementList};
use crate::error::KumoError;
use reqwest::header::HeaderMap;
use std::time::Duration;

/// Wraps an HTTP response and provides ergonomic extraction methods.
pub struct Response {
    pub url: String,
    pub status: u16,
    pub headers: HeaderMap,
    /// Wall-clock time from sending the request to reading the full body.
    pub elapsed: Duration,
    pub(crate) body: String,
}

impl Response {
    /// Construct a `Response` from raw parts. Primarily useful in tests and examples.
    pub fn from_parts(url: impl Into<String>, status: u16, body: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            status,
            headers: HeaderMap::new(),
            elapsed: Duration::ZERO,
            body: body.into(),
        }
    }

    /// Select elements in this page via a CSS selector.
    pub fn css(&self, selector: &str) -> ElementList {
        let document = scraper::Html::parse_document(&self.body);
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

    /// Get the raw HTML body.
    pub fn text(&self) -> &str {
        &self.body
    }

    /// Deserialize the response body as JSON.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, KumoError> {
        serde_json::from_str(&self.body).map_err(|e| KumoError::Parse(e.to_string()))
    }

    /// Apply a regex pattern to the raw response body and return all matches.
    ///
    /// If the pattern contains capture group 1, returns group-1 matches.
    /// Otherwise returns the full match. Returns an empty Vec on invalid pattern.
    pub fn re(&self, pattern: &str) -> Vec<String> {
        re_matches(&self.body, pattern)
    }

    /// Return the first regex match in the response body, or `None`.
    pub fn re_first(&self, pattern: &str) -> Option<String> {
        self.re(pattern).into_iter().next()
    }

    /// Evaluate a JSONPath expression against the response body parsed as JSON.
    ///
    /// Returns matched values as `serde_json::Value`. Errors on invalid JSON or
    /// invalid JSONPath expression.
    #[cfg(feature = "jsonpath")]
    pub fn jsonpath(&self, expr: &str) -> Result<Vec<serde_json::Value>, KumoError> {
        use jsonpath_rust::JsonPath;

        let value: serde_json::Value = serde_json::from_str(&self.body)
            .map_err(|e| KumoError::Parse(format!("invalid JSON body: {e}")))?;
        let results = value
            .query(expr)
            .map_err(|e| KumoError::Parse(format!("invalid JSONPath '{expr}': {e}")))?;
        Ok(results.into_iter().cloned().collect())
    }

    /// Resolve a relative URL against this response's URL.
    ///
    /// Returns `path` unchanged if joining fails (e.g. `path` is already absolute).
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
    use std::time::Duration;

    fn make_response(body: &str) -> Response {
        Response {
            url: "https://example.com".to_string(),
            status: 200,
            headers: HeaderMap::new(),
            elapsed: Duration::ZERO,
            body: body.to_string(),
        }
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
