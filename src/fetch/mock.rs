use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::header::HeaderMap;

use super::Fetcher;
use crate::{
    error::KumoError,
    extract::{Response, response::ResponseBody},
    middleware::Request,
};

/// A test-only fetcher that returns predefined responses by URL.
///
/// Use with [`CrawlEngine::fetcher`](crate::engine::CrawlEngine::fetcher) to run
/// spiders against fixed HTML without any network access.
///
/// # Example
/// ```rust,ignore
/// let mock = MockFetcher::new()
///     .with_response("https://example.com", 200, "<h1>Hello</h1>")
///     .with_response("https://example.com/page/2", 200, "<h1>Page 2</h1>");
///
/// let stats = CrawlEngine::builder()
///     .fetcher(mock)
///     .run(MySpider)
///     .await?;
/// ```
pub struct MockFetcher {
    responses: HashMap<String, (u16, String)>,
    default: Option<(u16, String)>,
}

impl MockFetcher {
    /// Create an empty mock fetcher. Unregistered URLs return 404 with an empty body.
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
            default: None,
        }
    }

    /// Register a response for a specific URL.
    pub fn with_response(mut self, url: &str, status: u16, body: impl Into<String>) -> Self {
        self.responses
            .insert(url.to_string(), (status, body.into()));
        self
    }

    /// Load a response body from a local HTML file for a specific URL.
    /// Panics if the file cannot be read.
    pub fn with_html_file(mut self, url: &str, path: impl AsRef<std::path::Path>) -> Self {
        let body = std::fs::read_to_string(path.as_ref()).unwrap_or_else(|e| {
            panic!(
                "MockFetcher: failed to read {}: {e}",
                path.as_ref().display()
            )
        });
        self.responses.insert(url.to_string(), (200, body));
        self
    }

    /// Respond with this status + body for any URL not explicitly registered.
    pub fn with_default(mut self, status: u16, body: impl Into<String>) -> Self {
        self.default = Some((status, body.into()));
        self
    }
}

impl Default for MockFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Fetcher for MockFetcher {
    async fn fetch(&self, request: &Request) -> Result<Response, KumoError> {
        let (status, body) = self
            .responses
            .get(request.url())
            .or(self.default.as_ref())
            .cloned()
            .unwrap_or((404, String::new()));

        Ok(Response::new(
            request.url().to_string(),
            status,
            HeaderMap::new(),
            std::time::Duration::ZERO,
            ResponseBody::Text(body),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(url: &str) -> Request {
        Request::new(url, 0)
    }

    #[tokio::test]
    async fn returns_registered_response() {
        let mock = MockFetcher::new().with_response("https://example.com", 200, "<h1>Hello</h1>");
        let res = mock.fetch(&req("https://example.com")).await.unwrap();
        assert_eq!(res.status(), 200);
        assert_eq!(res.text(), Some("<h1>Hello</h1>"));
        assert_eq!(res.url(), "https://example.com");
    }

    #[tokio::test]
    async fn returns_404_for_unregistered_url() {
        let mock = MockFetcher::new();
        let res = mock
            .fetch(&req("https://example.com/unknown"))
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    }

    #[tokio::test]
    async fn default_response_used_for_unregistered_url() {
        let mock = MockFetcher::new().with_default(200, "<p>default</p>");
        let res = mock.fetch(&req("https://anything.com")).await.unwrap();
        assert_eq!(res.status(), 200);
        assert_eq!(res.text(), Some("<p>default</p>"));
    }

    #[tokio::test]
    async fn specific_response_beats_default() {
        let mock = MockFetcher::new()
            .with_response("https://example.com/specific", 200, "specific")
            .with_default(200, "default");
        let res = mock
            .fetch(&req("https://example.com/specific"))
            .await
            .unwrap();
        assert_eq!(res.text(), Some("specific"));
    }

    #[tokio::test]
    async fn with_html_file_loads_from_disk() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), "<h1>From file</h1>").unwrap();
        let mock = MockFetcher::new().with_html_file("https://example.com", tmp.path());
        let res = mock.fetch(&req("https://example.com")).await.unwrap();
        assert_eq!(res.text(), Some("<h1>From file</h1>"));
    }
}
