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
    middleware::FetchRequest,
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

impl std::fmt::Debug for CachingFetcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachingFetcher")
            .field("dir", &self.dir)
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl Fetcher for CachingFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<Response, KumoError> {
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
