use bytes::Bytes;
use reqwest::header::HeaderMap;
use std::time::Duration;

pub(crate) enum ResponseBody {
    Text(String),
    Bytes(Bytes),
}

/// Wraps an HTTP response and provides ergonomic extraction methods.
pub struct Response {
    pub(super) url: String,
    pub(super) status: u16,
    pub(super) headers: HeaderMap,
    /// Wall-clock time from sending the request to reading the full body.
    pub(super) elapsed: Duration,
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

    /// Override the elapsed duration on an existing response - useful in tests.
    pub fn with_elapsed(mut self, elapsed: Duration) -> Self {
        self.elapsed = elapsed;
        self
    }

    /// Construct a `Response` from all fields - used internally by fetchers.
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
}
