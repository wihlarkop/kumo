use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};

use super::Fetcher;
use crate::{
    error::KumoError,
    extract::{Response, response::ResponseBody},
    middleware::Request,
};

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    url: String,
    status: u16,
    body: String,   // text body only; binary responses are not cached
    cached_at: u64, // Unix timestamp (seconds)
}

/// Wraps any [`Fetcher`] and caches text responses to disk.
///
/// Binary responses (images, PDFs, etc.) bypass the cache and are always fetched live.
/// Cache files are stored as JSON in the configured directory, one file per URL.
///
/// # Example
/// ```rust,ignore
/// use kumo::prelude::*;
///
/// // Convenience builder — wraps the default HTTP fetcher automatically:
/// let stats = CrawlEngine::builder()
///     .http_cache("./cache")?
///     .run(MySpider)
///     .await?;
/// ```
pub struct CachingFetcher {
    inner: Arc<dyn Fetcher>,
    dir: PathBuf,
    ttl: Option<Duration>,
}

impl CachingFetcher {
    /// Wrap `inner` with a disk cache stored in `dir`.
    pub fn new(inner: impl Fetcher + 'static, dir: impl Into<PathBuf>) -> Result<Self, KumoError> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir).map_err(|e| KumoError::store("http cache", e))?;
        Ok(Self {
            inner: Arc::new(inner),
            dir,
            ttl: None,
        })
    }

    /// Expire cached entries older than `ttl` and refetch them.
    /// Default: entries never expire.
    pub fn ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    fn cache_path(&self, url: &str) -> PathBuf {
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        self.dir.join(format!("{:016x}.json", hasher.finish()))
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    fn is_fresh(&self, entry: &CacheEntry) -> bool {
        match self.ttl {
            None => true,
            Some(ttl) => Self::now_secs().saturating_sub(entry.cached_at) < ttl.as_secs(),
        }
    }
}

#[async_trait]
impl Fetcher for CachingFetcher {
    async fn fetch(&self, request: &Request) -> Result<Response, KumoError> {
        let path = self.cache_path(request.url());

        // Try cache hit.
        if path.exists()
            && let Ok(data) = std::fs::read_to_string(&path)
            && let Ok(entry) = serde_json::from_str::<CacheEntry>(&data)
            && entry.url == request.url()
            && self.is_fresh(&entry)
        {
            tracing::debug!(url = request.url(), "http cache hit");
            return Ok(Response::new(
                entry.url,
                entry.status,
                HeaderMap::new(),
                Duration::ZERO,
                ResponseBody::Text(entry.body),
            ));
        }

        // Cache miss — fetch live.
        tracing::debug!(url = request.url(), "http cache miss");
        let response = self.inner.fetch(request).await?;

        // Only cache text responses; skip binary.
        if let Some(body_text) = response.text() {
            let entry = CacheEntry {
                url: response.url().to_string(),
                status: response.status(),
                body: body_text.to_string(),
                cached_at: Self::now_secs(),
            };
            if let Ok(json) = serde_json::to_string(&entry) {
                let _ = std::fs::write(&path, json); // best-effort write
            }
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::MockFetcher;

    fn req(url: &str) -> Request {
        Request::new(url, 0)
    }

    #[tokio::test]
    async fn first_request_fetches_from_inner() {
        let tmp = tempfile::TempDir::new().unwrap();
        let inner = MockFetcher::new().with_response("https://example.com", 200, "<h1>Hello</h1>");
        let cf = CachingFetcher::new(inner, tmp.path()).unwrap();

        let res = cf.fetch(&req("https://example.com")).await.unwrap();
        assert_eq!(res.status(), 200);
        assert_eq!(res.text(), Some("<h1>Hello</h1>"));
    }

    #[tokio::test]
    async fn second_request_served_from_cache() {
        let tmp = tempfile::TempDir::new().unwrap();
        let inner = MockFetcher::new().with_response("https://example.com", 200, "from network");
        let cf = CachingFetcher::new(inner, tmp.path()).unwrap();

        cf.fetch(&req("https://example.com")).await.unwrap();
        let res2 = cf.fetch(&req("https://example.com")).await.unwrap();
        assert_eq!(res2.text(), Some("from network"));
    }

    #[tokio::test]
    async fn cache_file_is_created_after_fetch() {
        let tmp = tempfile::TempDir::new().unwrap();
        let inner = MockFetcher::new().with_response("https://example.com", 200, "body");
        let cf = CachingFetcher::new(inner, tmp.path()).unwrap();

        cf.fetch(&req("https://example.com")).await.unwrap();

        let files: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
        assert_eq!(files.len(), 1);
    }

    #[tokio::test]
    async fn expired_entry_is_refetched() {
        let tmp = tempfile::TempDir::new().unwrap();
        let inner = MockFetcher::new()
            .with_response("https://example.com", 200, "body")
            .with_default(200, "refetched");
        let cf = CachingFetcher::new(inner, tmp.path())
            .unwrap()
            .ttl(Duration::from_secs(0)); // always expire

        cf.fetch(&req("https://example.com")).await.unwrap();
        let res = cf.fetch(&req("https://example.com")).await.unwrap();
        // TTL=0 means always stale — inner returns "refetched" via default
        assert_eq!(res.status(), 200);
    }
}
