use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use crate::{
    error::KumoError, fetch::Fetcher, frontier::Frontier, middleware::Middleware,
    pipeline::Pipeline, spider::Spider, store::ItemStore,
};

use super::erased::{ErasedSpider, SpiderErased};

#[cfg(feature = "browser")]
use crate::fetch::BrowserConfig;

type FrontierOverride = Option<Arc<dyn Frontier>>;

/// Statistics returned by `CrawlEngine::run` after the crawl finishes.
#[derive(Debug, Default, Clone)]
pub struct CrawlStats {
    pub pages_crawled: u64,
    pub items_scraped: u64,
    pub errors: u64,
    pub duration: Duration,
    pub bytes_downloaded: u64,
    /// `true` when the crawl was stopped early by Ctrl+C.
    pub interrupted: bool,
}

/// Fluent builder for configuring and launching a crawl.
///
/// # Example
/// ```rust,ignore
/// let stats = CrawlEngine::builder()
///     .concurrency(8)
///     .middleware(DefaultHeaders::new().user_agent("kumo/0.1"))
///     .store(JsonlStore::new("items.jsonl"))
///     .run(MySpider)
///     .await?;
/// ```
pub struct CrawlEngine {
    pub(super) concurrency: usize,
    pub(super) middleware: Vec<Arc<dyn Middleware>>,
    pub(super) pipelines: Vec<Arc<dyn Pipeline>>,
    pub(super) store: Option<Arc<dyn ItemStore>>,
    pub(super) respect_robots: bool,
    pub(super) retry_policy: crate::retry::RetryPolicy,
    pub(super) politeness_policy: crate::scheduler::PolitenessPolicy,
    pub(super) frontier: FrontierOverride,
    /// Expected unique URL count for Bloom filter sizing (default: 1_000_000).
    pub(super) max_urls: usize,
    pub(super) robots_ttl: Duration,
    pub(super) metrics_interval: Option<Duration>,
    pub(super) stream_buffer: usize,
    pub(super) request_timeout: Option<Duration>,
    pub(super) spiders: Vec<Arc<dyn ErasedSpider>>,
    pub(super) fetcher_override: Option<Arc<dyn Fetcher>>,
    pub(super) cache_dir: Option<std::path::PathBuf>,
    pub(super) cache_ttl: Option<Duration>,
    pub(super) http_client_builder:
        Option<Box<dyn FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder + Send>>,
    pub(super) stream_cancelled: Option<Arc<AtomicBool>>,
    #[cfg(feature = "browser")]
    pub(super) browser: Option<BrowserConfig>,
    #[cfg(feature = "stealth")]
    pub(super) stealth_profile: Option<crate::fetch::StealthProfile>,
}

impl Default for CrawlEngine {
    fn default() -> Self {
        Self {
            concurrency: 8,
            middleware: Vec::new(),
            store: None,
            respect_robots: true,
            pipelines: Vec::new(),
            frontier: None,
            retry_policy: crate::retry::RetryPolicy::new(0),
            politeness_policy: crate::scheduler::PolitenessPolicy::default(),
            max_urls: 1_000_000,
            robots_ttl: Duration::from_secs(24 * 60 * 60),
            metrics_interval: None,
            stream_buffer: 100,
            request_timeout: None,
            spiders: Vec::new(),
            fetcher_override: None,
            cache_dir: None,
            cache_ttl: None,
            http_client_builder: None,
            stream_cancelled: None,
            #[cfg(feature = "browser")]
            browser: None,
            #[cfg(feature = "stealth")]
            stealth_profile: None,
        }
    }
}

impl CrawlEngine {
    /// Begin building a new engine. Defaults: concurrency=8, StdoutStore, no delay.
    pub fn builder() -> Self {
        Self::default()
    }

    pub fn concurrency(mut self, n: usize) -> Self {
        self.concurrency = n;
        self
    }

    /// Register a middleware (applied in registration order).
    pub fn middleware(mut self, mw: impl Middleware + 'static) -> Self {
        self.middleware.push(Arc::new(mw));
        self
    }

    /// Register an item pipeline stage (applied in registration order before the store).
    pub fn pipeline(mut self, p: impl Pipeline + 'static) -> Self {
        self.pipelines.push(Arc::new(p));
        self
    }

    /// Use a custom frontier (e.g. `FileFrontier`) instead of the default in-memory frontier.
    pub fn frontier(mut self, f: impl Frontier + 'static) -> Self {
        self.frontier = Some(Arc::new(f));
        self
    }

    /// Set the output store. Defaults to `StdoutStore` if not called.
    pub fn store(mut self, store: impl ItemStore + 'static) -> Self {
        self.store = Some(Arc::new(store));
        self
    }

    /// Insert a fixed delay between requests (applied per task, not globally).
    pub fn crawl_delay(mut self, delay: Duration) -> Self {
        self.politeness_policy = self.politeness_policy.per_domain_delay(delay);
        self
    }

    /// Configure production crawl politeness such as per-domain concurrency and delay.
    pub fn politeness(mut self, policy: crate::scheduler::PolitenessPolicy) -> Self {
        self.politeness_policy = policy;
        self
    }

    /// Set a retry policy with full control over attempts, delay, jitter, and status filtering.
    ///
    /// # Example
    /// ```rust,ignore
    /// .retry_policy(
    ///     RetryPolicy::new(3)
    ///         .base_delay(Duration::from_millis(200))
    ///         .max_delay(Duration::from_secs(30))
    ///         .jitter(true)
    ///         .on_status(429)
    ///         .on_status(503),
    /// )
    /// ```
    pub fn retry_policy(mut self, policy: crate::retry::RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    /// Convenience wrapper: retry up to `max_attempts` times with exponential backoff
    /// starting at `base_delay`. Retries on any transient HTTP or network error.
    pub fn retry(mut self, max_attempts: u32, base_delay: Duration) -> Self {
        self.retry_policy = crate::retry::RetryPolicy::new(max_attempts).base_delay(base_delay);
        self
    }

    /// Whether to respect robots.txt (default: true).
    pub fn respect_robots_txt(mut self, v: bool) -> Self {
        self.respect_robots = v;
        self
    }

    /// Emit a `tracing::info!` stats snapshot every `interval` during the crawl.
    /// Useful for monitoring long-running crawls without an external metrics system.
    pub fn metrics_interval(mut self, interval: Duration) -> Self {
        self.metrics_interval = Some(interval);
        self
    }

    /// TTL for cached robots.txt entries (default: 24 hours).
    pub fn robots_ttl(mut self, ttl: Duration) -> Self {
        self.robots_ttl = ttl;
        self
    }

    /// Expected number of unique URLs this crawl will visit (default: 1_000_000).
    /// Used to size the Bloom filter for URL deduplication â€” set lower for small
    /// crawls to save memory, higher for large crawls to reduce false-positive skips.
    pub fn max_urls(mut self, n: usize) -> Self {
        self.max_urls = n;
        self
    }

    /// Use a headless/headed browser to fetch pages instead of plain HTTP.
    /// Enables JavaScript rendering for SPAs (React, Vue, etc.).
    ///
    /// Requires the `browser` feature flag.
    #[cfg(feature = "browser")]
    pub fn browser(mut self, cfg: BrowserConfig) -> Self {
        self.browser = Some(cfg);
        self
    }

    /// Register a spider for multi-spider execution via [`run_all`](Self::run_all).
    /// Each registered spider gets its own URL frontier.
    pub fn add_spider<S: Spider + 'static>(mut self, spider: S) -> Self {
        self.spiders.push(Arc::new(SpiderErased(spider)));
        self
    }

    /// Use a custom fetcher instead of the default `HttpFetcher`.
    ///
    /// Primarily useful for testing â€” inject a [`MockFetcher`](crate::fetch::MockFetcher)
    /// to run spiders against predefined HTML without any network access.
    pub fn fetcher(mut self, f: impl Fetcher + 'static) -> Self {
        self.fetcher_override = Some(Arc::new(f));
        self
    }

    /// Cache HTTP responses to disk in `dir`.
    ///
    /// On subsequent runs, cached responses are served directly â€” no network requests.
    /// Ideal during development to speed up spider iteration.
    /// Use `.cache_ttl()` to set an expiry duration.
    pub fn http_cache(mut self, dir: impl Into<std::path::PathBuf>) -> Result<Self, KumoError> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir).map_err(|e| KumoError::store("http cache", e))?;
        self.cache_dir = Some(dir);
        Ok(self)
    }

    /// Expire cached HTTP responses older than `ttl` (used with `.http_cache()`).
    /// Default: entries never expire.
    pub fn cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = Some(ttl);
        self
    }

    /// Customize the underlying `reqwest::Client` before it is built.
    ///
    /// Use this to set custom timeouts, DNS resolvers, TLS configuration, or
    /// any other reqwest option not exposed by the engine builder.
    ///
    /// # Example
    /// ```rust,ignore
    /// CrawlEngine::builder()
    ///     .http_client_builder(|b| b.timeout(Duration::from_secs(10)))
    ///     .run(MySpider)
    ///     .await?;
    /// ```
    pub fn http_client_builder(
        mut self,
        f: impl FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder + Send + 'static,
    ) -> Self {
        self.http_client_builder = Some(Box::new(f));
        self
    }

    /// Use a stealth HTTP fetcher with TLS + HTTP/2 fingerprint spoofing.
    ///
    /// Requires the `stealth` feature flag (and cmake/NASM build tools for BoringSSL).
    /// Replaces the default `HttpFetcher` with one backed by `rquest` that reproduces
    /// the exact TLS client hello of a real browser, defeating JA3/JA4 fingerprinting.
    #[cfg(feature = "stealth")]
    pub fn stealth(mut self, profile: crate::fetch::StealthProfile) -> Self {
        self.stealth_profile = Some(profile);
        self
    }

    /// Set the internal channel buffer size for [`CrawlEngine::stream`].
    ///
    /// When the buffer is full the crawl pauses until the consumer reads more
    /// items â€” this is the natural backpressure mechanism. Default: `100`.
    pub fn stream_buffer(mut self, n: usize) -> Self {
        self.stream_buffer = n;
        self
    }

    /// Set a per-request timeout. Requests exceeding this duration return `KumoError::Fetch`.
    /// Default: no timeout.
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }
}
