use reqwest::header::HeaderMap;
use crate::error::KumoError;
use super::selector::{Element, ElementList};

/// Wraps an HTTP response and provides ergonomic extraction methods.
pub struct Response {
    pub url: String,
    pub status: u16,
    pub headers: HeaderMap,
    pub(crate) body: String,
}

impl Response {
    /// Select elements in this page via a CSS selector.
    pub fn css(&self, selector: &str) -> ElementList {
        let document = scraper::Html::parse_document(&self.body);
        let Ok(sel) = scraper::Selector::parse(selector) else {
            return ElementList { elements: vec![] };
        };
        let elements = document
            .select(&sel)
            .map(|el| Element { outer_html: el.html() })
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
