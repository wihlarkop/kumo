use std::{sync::Arc, time::Duration};
use tokio::task::JoinSet;
use tracing::{error, info};

use crate::{
    error::{ErrorPolicy, KumoError},
    extract::Response,
    fetch::{Fetcher, http::HttpFetcher},
    frontier::{Frontier, memory::MemoryFrontier},
    middleware::{Middleware, Request},
    pipeline::Pipeline,
    robots::RobotsCache,
    spider::{Output, Spider},
    store::ItemStore,
};

// ── Type erasure ──────────────────────────────────────────────────────────────
//
// `Spider::Item` is an associated type, which makes `dyn Spider` non-object-safe.
// `ErasedSpider` is an internal object-safe twin that serializes items to
// `serde_json::Value` inside `parse_erased`, allowing the engine to use
// `Arc<dyn ErasedSpider>` for its task contexts.

struct ErasedOutput {
    items: Vec<serde_json::Value>,
    follow: Vec<String>,
}

#[async_trait::async_trait]
trait ErasedSpider: Send + Sync {
    fn name(&self) -> &str;
    fn start_urls(&self) -> Vec<String>;
    async fn parse_erased(&self, response: &Response) -> Result<ErasedOutput, KumoError>;
    fn on_error(&self, url: &str, err: &KumoError) -> ErrorPolicy;
    fn max_depth(&self) -> Option<usize>;
    fn allowed_domains(&self) -> Vec<&str>;
    async fn open(&self) -> Result<(), KumoError>;
    async fn close(&self, stats: &CrawlStats) -> Result<(), KumoError>;
}

struct SpiderErased<S>(S);

#[async_trait::async_trait]
impl<S: Spider + 'static> ErasedSpider for SpiderErased<S> {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn start_urls(&self) -> Vec<String> {
        self.0.start_urls()
    }

    async fn parse_erased(&self, response: &Response) -> Result<ErasedOutput, KumoError> {
        let output: Output<S::Item> = self.0.parse(response).await?;
        let items = output
            .items
            .into_iter()
            .map(|item| {
                serde_json::to_value(item).map_err(|e| KumoError::parse("item serialization", e))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ErasedOutput {
            items,
            follow: output.follow,
        })
    }

    fn on_error(&self, url: &str, err: &KumoError) -> ErrorPolicy {
        self.0.on_error(url, err)
    }
    fn max_depth(&self) -> Option<usize> {
        self.0.max_depth()
    }
    fn allowed_domains(&self) -> Vec<&str> {
        self.0.allowed_domains()
    }
    async fn open(&self) -> Result<(), KumoError> {
        self.0.open().await
    }
    async fn close(&self, stats: &CrawlStats) -> Result<(), KumoError> {
        self.0.close(stats).await
    }
}

type FrontierOverride = Option<Arc<dyn Frontier>>;

#[cfg(feature = "browser")]
use crate::fetch::{BrowserConfig, BrowserFetcher};

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
    concurrency: usize,
    middleware: Vec<Arc<dyn Middleware>>,
    pipelines: Vec<Arc<dyn Pipeline>>,
    store: Option<Arc<dyn ItemStore>>,
    crawl_delay: Option<Duration>,
    respect_robots: bool,
    retry_policy: crate::retry::RetryPolicy,
    frontier: FrontierOverride,
    /// Expected unique URL count for Bloom filter sizing (default: 1_000_000).
    max_urls: usize,
    robots_ttl: Duration,
    metrics_interval: Option<Duration>,
    stream_buffer: usize,
    spiders: Vec<Arc<dyn ErasedSpider>>,
    fetcher_override: Option<Arc<dyn Fetcher>>,
    cache_dir: Option<std::path::PathBuf>,
    cache_ttl: Option<Duration>,
    http_client_builder:
        Option<Box<dyn FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder + Send>>,
    #[cfg(feature = "browser")]
    browser: Option<BrowserConfig>,
    #[cfg(feature = "stealth")]
    stealth_profile: Option<crate::fetch::StealthProfile>,
}

impl Default for CrawlEngine {
    fn default() -> Self {
        Self {
            concurrency: 8,
            middleware: Vec::new(),
            store: None,
            crawl_delay: None,
            respect_robots: true,
            pipelines: Vec::new(),
            frontier: None,
            retry_policy: crate::retry::RetryPolicy::new(0),
            max_urls: 1_000_000,
            robots_ttl: Duration::from_secs(24 * 60 * 60),
            metrics_interval: None,
            stream_buffer: 100,
            spiders: Vec::new(),
            fetcher_override: None,
            cache_dir: None,
            cache_ttl: None,
            http_client_builder: None,
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
        self.crawl_delay = Some(delay);
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
    /// Used to size the Bloom filter for URL deduplication — set lower for small
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
    /// Primarily useful for testing — inject a [`MockFetcher`](crate::fetch::MockFetcher)
    /// to run spiders against predefined HTML without any network access.
    pub fn fetcher(mut self, f: impl Fetcher + 'static) -> Self {
        self.fetcher_override = Some(Arc::new(f));
        self
    }

    /// Cache HTTP responses to disk in `dir`.
    ///
    /// On subsequent runs, cached responses are served directly — no network requests.
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
    /// items — this is the natural backpressure mechanism. Default: `100`.
    pub fn stream_buffer(mut self, n: usize) -> Self {
        self.stream_buffer = n;
        self
    }

    /// Run the crawl in the background and stream items as they are scraped.
    ///
    /// Unlike [`run`](Self::run), this returns immediately with an [`ItemStream`].
    /// The crawl engine runs in a spawned Tokio task; dropping the stream stops
    /// the crawl gracefully.
    ///
    /// # Example
    /// ```rust,ignore
    /// use tokio_stream::StreamExt;
    ///
    /// let mut stream = CrawlEngine::builder()
    ///     .concurrency(4)
    ///     .stream(MySpider)
    ///     .await?;
    ///
    /// while let Some(item) = stream.next().await {
    ///     println!("{}", item);
    /// }
    /// ```
    pub async fn stream<S>(self, spider: S) -> Result<ItemStream, KumoError>
    where
        S: Spider + 'static,
    {
        let buffer = self.stream_buffer;
        let (tx, rx) = tokio::sync::mpsc::channel(buffer);
        let engine = self.store(ChannelStore { tx });
        tokio::spawn(async move {
            if let Err(e) = engine.run(spider).await {
                tracing::error!(error = %e, "stream crawl error");
            }
        });
        Ok(ItemStream {
            inner: tokio_stream::wrappers::ReceiverStream::new(rx),
        })
    }

    /// Consume the engine, run the spider, and return crawl statistics.
    pub async fn run<S>(self, spider: S) -> Result<CrawlStats, KumoError>
    where
        S: Spider + 'static,
    {
        let start = std::time::Instant::now();
        let metrics_interval = self.metrics_interval;
        let spider: Arc<dyn ErasedSpider> = Arc::new(SpiderErased(spider));
        let frontier: Arc<dyn Frontier> = self
            .frontier
            .unwrap_or_else(|| Arc::new(MemoryFrontier::new(self.max_urls)));
        let store = self
            .store
            .unwrap_or_else(|| Arc::new(crate::store::stdout::StdoutStore));
        let middleware: Arc<Vec<Arc<dyn Middleware>>> = Arc::new(self.middleware);
        let pipelines: Arc<Vec<Arc<dyn Pipeline>>> = Arc::new(self.pipelines);

        // Warn if both AutoThrottle and RateLimiter are registered — they compound delays.
        {
            let has_throttle = middleware
                .iter()
                .any(|mw| std::any::type_name_of_val(mw.as_ref()).contains("AutoThrottle"));
            let has_limiter = middleware
                .iter()
                .any(|mw| std::any::type_name_of_val(mw.as_ref()).contains("RateLimiter"));
            if has_throttle && has_limiter {
                tracing::warn!(
                    "Both AutoThrottle and RateLimiter are registered. \
                     They apply delays independently and will compound. \
                     Consider using only one."
                );
            }
        }
        let crawl_delay = self.crawl_delay;
        let concurrency = self.concurrency;
        let retry_policy = self.retry_policy;
        let robots_cache = if self.respect_robots {
            Some(Arc::new(RobotsCache::with_ttl(
                concat!("kumo/", env!("CARGO_PKG_VERSION")),
                self.robots_ttl,
            )))
        } else {
            None
        };

        // Single shared reqwest client — used for robots.txt and plain HTTP fetching.
        let client = {
            let mut builder = reqwest::Client::builder()
                .cookie_store(true)
                .user_agent(concat!("kumo/", env!("CARGO_PKG_VERSION")));
            if let Some(customize) = self.http_client_builder {
                builder = customize(builder);
            }
            builder.build().map_err(KumoError::Fetch)?
        };

        // Build the fetcher: use override if provided, else stealth/browser/plain HTTP.
        let fetcher: Arc<dyn Fetcher> = if let Some(f) = self.fetcher_override {
            f
        } else {
            #[cfg(feature = "stealth")]
            if let Some(profile) = self.stealth_profile {
                Arc::new(crate::fetch::StealthHttpFetcher::new(profile)?)
            } else {
                #[cfg(not(feature = "browser"))]
                {
                    Arc::new(HttpFetcher::new(
                        client.clone(),
                        concat!("kumo/", env!("CARGO_PKG_VERSION")),
                    ))
                }
                #[cfg(feature = "browser")]
                {
                    match self.browser {
                        Some(cfg) => Arc::new(BrowserFetcher::launch(cfg, concurrency).await?),
                        None => Arc::new(HttpFetcher::new(
                            client.clone(),
                            concat!("kumo/", env!("CARGO_PKG_VERSION")),
                        )),
                    }
                }
            }
            #[cfg(not(feature = "stealth"))]
            {
                #[cfg(not(feature = "browser"))]
                {
                    Arc::new(HttpFetcher::new(
                        client.clone(),
                        concat!("kumo/", env!("CARGO_PKG_VERSION")),
                    ))
                }
                #[cfg(feature = "browser")]
                {
                    match self.browser {
                        Some(cfg) => Arc::new(BrowserFetcher::launch(cfg, concurrency).await?),
                        None => Arc::new(HttpFetcher::new(
                            client.clone(),
                            concat!("kumo/", env!("CARGO_PKG_VERSION")),
                        )),
                    }
                }
            }
        };

        // Wrap with disk cache if configured.
        let fetcher: Arc<dyn Fetcher> = if let Some(dir) = self.cache_dir {
            let mut cf = crate::fetch::CachingFetcher::new(ArcFetcher(fetcher), dir)?;
            if let Some(ttl) = self.cache_ttl {
                cf = cf.ttl(ttl);
            }
            Arc::new(cf)
        } else {
            fetcher
        };

        spider.open().await?;

        let start_urls = spider.start_urls();
        info!(
            spider = spider.name(),
            start_urls = start_urls.len(),
            "spider.open"
        );
        for url in start_urls {
            frontier.push(url, 0).await;
        }

        type TaskResult = (
            String,
            usize,
            u32,
            Result<(u64, u64, Vec<(String, usize)>), KumoError>,
        );
        let mut join_set: JoinSet<TaskResult> = JoinSet::new();
        let mut stats = CrawlStats::default();

        // Spawn periodic metrics logger if configured.
        let live_stats = Arc::new(tokio::sync::Mutex::new(CrawlStats::default()));
        let _metrics_task = metrics_interval.map(|interval| {
            let live = live_stats.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(interval).await;
                    let s = live.lock().await;
                    tracing::info!(
                        pages = s.pages_crawled,
                        items = s.items_scraped,
                        errors = s.errors,
                        bytes = s.bytes_downloaded,
                        elapsed_secs = s.duration.as_secs_f64(),
                        "[kumo metrics]"
                    );
                }
            })
        });

        let shutdown = async {
            #[cfg(not(target_arch = "wasm32"))]
            {
                tokio::signal::ctrl_c().await.ok();
                tracing::info!("ctrl-c received — finishing in-flight tasks then exiting");
            }
            #[cfg(target_arch = "wasm32")]
            std::future::pending::<()>().await
        };
        tokio::pin!(shutdown);
        let mut shutting_down = false;

        loop {
            if !shutting_down {
                // Fill up to the concurrency limit.
                while join_set.len() < concurrency {
                    match frontier.pop().await {
                        Some((url, depth, retry_count)) => {
                            // Check robots.txt before dispatching.
                            if let Some(ref cache) = robots_cache
                                && !cache.is_allowed(&client, &url).await
                            {
                                tracing::debug!(url = %url, "blocked by robots.txt, skipping");
                                continue;
                            }

                            let ctx = TaskContext {
                                spider: spider.clone(),
                                store: store.clone(),
                                middleware: middleware.clone(),
                                pipelines: pipelines.clone(),
                                fetcher: fetcher.clone(),
                                crawl_delay,
                                retry_policy: retry_policy.clone(),
                            };

                            join_set.spawn(async move {
                                let result = process_url_with_retry(url.clone(), depth, ctx).await;
                                (url, depth, retry_count, result)
                            });
                        }
                        // Frontier currently empty — tasks may still add URLs.
                        None => break,
                    }
                }
            }

            // Both the queue is empty and no tasks are running → crawl complete.
            if join_set.is_empty() {
                break;
            }

            tokio::select! {
                _ = &mut shutdown, if !shutting_down => {
                    shutting_down = true;
                    stats.interrupted = true;
                }
                result = join_set.join_next() => {
                    match result {
                        Some(Ok((_url, _depth, _retry_count, Ok((item_count, bytes, follows))))) => {
                            stats.pages_crawled += 1;
                            stats.items_scraped += item_count;
                            stats.bytes_downloaded += bytes;
                            // Keep live snapshot up to date for the metrics task.
                            if metrics_interval.is_some() {
                                let mut snap = live_stats.lock().await;
                                snap.pages_crawled = stats.pages_crawled;
                                snap.items_scraped = stats.items_scraped;
                                snap.errors = stats.errors;
                                snap.bytes_downloaded = stats.bytes_downloaded;
                                snap.duration = start.elapsed();
                            }

                            if !shutting_down {
                                for (follow_url, follow_depth) in follows {
                                    // Respect max_depth.
                                    if let Some(max) = spider.max_depth()
                                        && follow_depth > max
                                    {
                                        continue;
                                    }

                                    // Respect allowed_domains (empty list = allow all).
                                    let allowed = spider.allowed_domains();
                                    if !allowed.is_empty() {
                                        let domain_ok = url::Url::parse(&follow_url)
                                            .ok()
                                            .and_then(|u| u.host_str().map(String::from))
                                            .map(|host| allowed.iter().any(|d| host.ends_with(*d)))
                                            .unwrap_or(false);
                                        if !domain_ok {
                                            continue;
                                        }
                                    }

                                    frontier.push(follow_url, follow_depth).await;
                                }
                            }
                        }
                        Some(Ok((url, depth, retry_count, Err(e)))) => {
                            stats.errors += 1;
                            // Notify all middleware of the permanent failure.
                            for mw in middleware.iter() {
                                mw.on_error(&url, &e).await;
                            }
                            match spider.on_error(&url, &e) {
                                ErrorPolicy::Abort => {
                                    error!(url = %url, error = %e, "aborting crawl");
                                    return Err(e);
                                }
                                ErrorPolicy::Retry(max) if retry_count < max => {
                                    tracing::warn!(
                                        spider = spider.name(),
                                        url = %url,
                                        attempt = retry_count + 1,
                                        max,
                                        error = %e,
                                        "re-queuing failed URL"
                                    );
                                    if !shutting_down {
                                        frontier.push_force(url, depth, retry_count + 1).await;
                                    }
                                }
                                ErrorPolicy::Retry(_) => {
                                    tracing::warn!(spider = spider.name(), url = %url, error = %e, "fetch.skip.retry_exhausted");
                                }
                                ErrorPolicy::Skip => {
                                    tracing::warn!(spider = spider.name(), url = %url, error = %e, "fetch.skip");
                                }
                            }
                        }
                        Some(Err(join_err)) => {
                            stats.errors += 1;
                            error!(spider = spider.name(), error = %join_err, "crawl task panicked");
                        }
                        None => break,
                    }

                    if shutting_down && join_set.is_empty() {
                        break;
                    }
                }
            }
        }

        store.flush().await?;
        stats.duration = start.elapsed();

        // close() errors are intentionally not propagated — the crawl and store
        // flush completed successfully. Cleanup failures are logged only.
        if let Err(e) = spider.close(&stats).await {
            tracing::error!(error = %e, "spider::close failed");
        }

        let rps = if stats.duration.as_secs_f64() > 0.0 {
            stats.pages_crawled as f64 / stats.duration.as_secs_f64()
        } else {
            0.0
        };
        info!(
            pages = stats.pages_crawled,
            items = stats.items_scraped,
            errors = stats.errors,
            bytes = stats.bytes_downloaded,
            duration_secs = stats.duration.as_secs_f64(),
            pages_per_sec = format!("{rps:.1}"),
            interrupted = stats.interrupted,
            "crawl complete"
        );

        Ok(stats)
    }

    /// Run all spiders registered via [`add_spider`](Self::add_spider) concurrently
    /// within the same engine (shared middleware, store, and concurrency limit).
    ///
    /// Returns one [`CrawlStats`] per spider, in registration order.
    pub async fn run_all(self) -> Result<Vec<CrawlStats>, KumoError> {
        if self.spiders.is_empty() {
            return Ok(Vec::new());
        }

        let start = std::time::Instant::now();
        let n = self.spiders.len();

        let spider_entries: Vec<(Arc<dyn ErasedSpider>, Arc<dyn Frontier>)> = self
            .spiders
            .into_iter()
            .map(|sp| {
                let frontier: Arc<dyn Frontier> = Arc::new(MemoryFrontier::new(self.max_urls));
                (sp, frontier)
            })
            .collect();

        let store = self
            .store
            .unwrap_or_else(|| Arc::new(crate::store::stdout::StdoutStore));
        let middleware: Arc<Vec<Arc<dyn Middleware>>> = Arc::new(self.middleware);
        let pipelines: Arc<Vec<Arc<dyn Pipeline>>> = Arc::new(self.pipelines);
        let crawl_delay = self.crawl_delay;
        let concurrency = self.concurrency;
        let retry_policy = self.retry_policy;

        let client = {
            let mut builder = reqwest::Client::builder()
                .cookie_store(true)
                .user_agent(concat!("kumo/", env!("CARGO_PKG_VERSION")));
            if let Some(customize) = self.http_client_builder {
                builder = customize(builder);
            }
            builder.build().map_err(KumoError::Fetch)?
        };

        let fetcher: Arc<dyn Fetcher> = if let Some(f) = self.fetcher_override {
            f
        } else {
            #[cfg(feature = "stealth")]
            if let Some(profile) = self.stealth_profile {
                Arc::new(crate::fetch::StealthHttpFetcher::new(profile)?)
            } else {
                #[cfg(not(feature = "browser"))]
                {
                    Arc::new(HttpFetcher::new(
                        client.clone(),
                        concat!("kumo/", env!("CARGO_PKG_VERSION")),
                    ))
                }
                #[cfg(feature = "browser")]
                {
                    match self.browser {
                        Some(cfg) => Arc::new(BrowserFetcher::launch(cfg, concurrency).await?),
                        None => Arc::new(HttpFetcher::new(
                            client.clone(),
                            concat!("kumo/", env!("CARGO_PKG_VERSION")),
                        )),
                    }
                }
            }
            #[cfg(not(feature = "stealth"))]
            {
                #[cfg(not(feature = "browser"))]
                {
                    Arc::new(HttpFetcher::new(
                        client.clone(),
                        concat!("kumo/", env!("CARGO_PKG_VERSION")),
                    ))
                }
                #[cfg(feature = "browser")]
                {
                    match self.browser {
                        Some(cfg) => Arc::new(BrowserFetcher::launch(cfg, concurrency).await?),
                        None => Arc::new(HttpFetcher::new(
                            client.clone(),
                            concat!("kumo/", env!("CARGO_PKG_VERSION")),
                        )),
                    }
                }
            }
        };

        let fetcher: Arc<dyn Fetcher> = if let Some(dir) = self.cache_dir {
            let mut cf = crate::fetch::CachingFetcher::new(ArcFetcher(fetcher), dir)?;
            if let Some(ttl) = self.cache_ttl {
                cf = cf.ttl(ttl);
            }
            Arc::new(cf)
        } else {
            fetcher
        };

        let robots_cache = if self.respect_robots {
            Some(Arc::new(RobotsCache::with_ttl(
                concat!("kumo/", env!("CARGO_PKG_VERSION")),
                self.robots_ttl,
            )))
        } else {
            None
        };

        for (spider, _) in &spider_entries {
            spider.open().await?;
        }

        for (spider, frontier) in &spider_entries {
            info!(spider = spider.name(), "registering spider for multi-crawl");
            for url in spider.start_urls() {
                frontier.push(url, 0).await;
            }
        }

        type MultiTaskResult = (
            usize,
            String,
            usize,
            u32,
            Result<(u64, u64, Vec<(String, usize)>), KumoError>,
        );
        let mut join_set: JoinSet<MultiTaskResult> = JoinSet::new();
        let mut stats_vec: Vec<CrawlStats> = (0..n).map(|_| CrawlStats::default()).collect();

        let shutdown = async {
            #[cfg(not(target_arch = "wasm32"))]
            {
                tokio::signal::ctrl_c().await.ok();
                tracing::info!("ctrl-c received — finishing in-flight tasks then exiting");
            }
            #[cfg(target_arch = "wasm32")]
            std::future::pending::<()>().await
        };
        tokio::pin!(shutdown);
        let mut shutting_down = false;
        let mut fill_cursor = 0usize;

        loop {
            if !shutting_down {
                'fill: while join_set.len() < concurrency {
                    let mut any_popped = false;
                    for attempt in 0..n {
                        let idx = (fill_cursor + attempt) % n;
                        let (spider, frontier) = &spider_entries[idx];
                        if let Some((url, depth, retry_count)) = frontier.pop().await {
                            if let Some(ref cache) = robots_cache
                                && !cache.is_allowed(&client, &url).await
                            {
                                tracing::debug!(url = %url, "blocked by robots.txt, skipping");
                                continue;
                            }
                            let ctx = TaskContext {
                                spider: spider.clone(),
                                store: store.clone(),
                                middleware: middleware.clone(),
                                pipelines: pipelines.clone(),
                                fetcher: fetcher.clone(),
                                crawl_delay,
                                retry_policy: retry_policy.clone(),
                            };
                            join_set.spawn(async move {
                                let result = process_url_with_retry(url.clone(), depth, ctx).await;
                                (idx, url, depth, retry_count, result)
                            });
                            fill_cursor = idx + 1;
                            any_popped = true;
                            break;
                        }
                    }
                    if !any_popped {
                        break 'fill;
                    }
                }
            }

            if join_set.is_empty() {
                break;
            }

            tokio::select! {
                _ = &mut shutdown, if !shutting_down => {
                    shutting_down = true;
                    for s in &mut stats_vec { s.interrupted = true; }
                }
                result = join_set.join_next() => {
                    match result {
                        Some(Ok((spider_idx, _url, _depth, _retry_count, Ok((item_count, bytes, follows))))) => {
                            let stats = &mut stats_vec[spider_idx];
                            stats.pages_crawled += 1;
                            stats.items_scraped += item_count;
                            stats.bytes_downloaded += bytes;
                            if !shutting_down {
                                let (spider, frontier) = &spider_entries[spider_idx];
                                for (follow_url, follow_depth) in follows {
                                    if let Some(max) = spider.max_depth()
                                        && follow_depth > max
                                    {
                                        continue;
                                    }
                                    let allowed = spider.allowed_domains();
                                    if !allowed.is_empty() {
                                        let ok = url::Url::parse(&follow_url)
                                            .ok()
                                            .and_then(|u| u.host_str().map(String::from))
                                            .map(|host| allowed.iter().any(|d| host.ends_with(*d)))
                                            .unwrap_or(false);
                                        if !ok {
                                            continue;
                                        }
                                    }
                                    frontier.push(follow_url, follow_depth).await;
                                }
                            }
                        }
                        Some(Ok((spider_idx, url, depth, retry_count, Err(e)))) => {
                            stats_vec[spider_idx].errors += 1;
                            for mw in middleware.iter() {
                                mw.on_error(&url, &e).await;
                            }
                            let (spider, frontier) = &spider_entries[spider_idx];
                            match spider.on_error(&url, &e) {
                                ErrorPolicy::Abort => {
                                    error!(url = %url, error = %e, "aborting crawl");
                                    return Err(e);
                                }
                                ErrorPolicy::Retry(max) if retry_count < max => {
                                    if !shutting_down {
                                        frontier.push_force(url, depth, retry_count + 1).await;
                                    }
                                }
                                _ => {
                                    tracing::warn!(spider = spider.name(), url = %url, error = %e, "fetch.skip");
                                }
                            }
                        }
                        Some(Err(join_err)) => {
                            error!(error = %join_err, "crawl task panicked");
                        }
                        None => break,
                    }
                    if shutting_down && join_set.is_empty() {
                        break;
                    }
                }
            }
        }

        store.flush().await?;
        let elapsed = start.elapsed();

        for (i, (spider, _)) in spider_entries.iter().enumerate() {
            stats_vec[i].duration = elapsed;
            if let Err(e) = spider.close(&stats_vec[i]).await {
                tracing::error!(spider = spider.name(), error = %e, "spider::close failed");
            }
            let rps = if elapsed.as_secs_f64() > 0.0 {
                stats_vec[i].pages_crawled as f64 / elapsed.as_secs_f64()
            } else {
                0.0
            };
            info!(
                spider = spider.name(),
                pages = stats_vec[i].pages_crawled,
                items = stats_vec[i].items_scraped,
                errors = stats_vec[i].errors,
                bytes = stats_vec[i].bytes_downloaded,
                pages_per_sec = format!("{rps:.1}"),
                "spider complete"
            );
        }

        Ok(stats_vec)
    }
}

/// Shared context cloned into each spawned task.
struct TaskContext {
    spider: Arc<dyn ErasedSpider>,
    store: Arc<dyn ItemStore>,
    middleware: Arc<Vec<Arc<dyn Middleware>>>,
    pipelines: Arc<Vec<Arc<dyn Pipeline>>>,
    fetcher: Arc<dyn Fetcher>,
    crawl_delay: Option<Duration>,
    retry_policy: crate::retry::RetryPolicy,
}

/// Fetch, run middleware, parse, and store items for a single URL.
/// Returns (items_stored, bytes_downloaded, follow_urls_with_depth).
async fn process_url(
    url: &str,
    depth: usize,
    ctx: &TaskContext,
) -> Result<(u64, u64, Vec<(String, usize)>), KumoError> {
    if let Some(delay) = ctx.crawl_delay {
        tokio::time::sleep(delay).await;
    }

    let mut request = Request::new(url, depth);
    for mw in ctx.middleware.iter() {
        mw.before_request(&mut request).await?;
    }

    let mut response = ctx.fetcher.fetch(&request).await?;
    let bytes_downloaded = response.bytes().len() as u64;

    for mw in ctx.middleware.iter() {
        mw.after_response(&mut response).await?;
    }

    let output = ctx.spider.parse_erased(&response).await?;

    let mut item_count = 0u64;
    'items: for item in output.items {
        let mut current = item;
        for pipeline in ctx.pipelines.iter() {
            match pipeline.process(current).await {
                Ok(Some(v)) => current = v,
                Ok(None) => {
                    tracing::debug!(spider = ctx.spider.name(), url, "item.drop");
                    continue 'items;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "pipeline dropped item due to error");
                    continue 'items;
                }
            }
        }
        ctx.store.store(&current).await?;
        item_count += 1;
    }

    tracing::debug!(
        spider = ctx.spider.name(),
        url,
        status = response.status(),
        bytes = bytes_downloaded,
        depth,
        items = item_count,
        "fetch.ok"
    );

    let follows = output.follow.into_iter().map(|u| (u, depth + 1)).collect();

    Ok((item_count, bytes_downloaded, follows))
}

/// Wraps `process_url` with exponential-backoff retry driven by `RetryPolicy`.
async fn process_url_with_retry(
    url: String,
    depth: usize,
    ctx: TaskContext,
) -> Result<(u64, u64, Vec<(String, usize)>), KumoError> {
    let mut attempt = 0u32;
    loop {
        match process_url(&url, depth, &ctx).await {
            Ok(result) => return Ok(result),
            Err(e)
                if attempt < ctx.retry_policy.max_attempts && ctx.retry_policy.is_retriable(&e) =>
            {
                for mw in ctx.middleware.iter() {
                    mw.on_error(&url, &e).await;
                }
                let delay = ctx.retry_policy.delay_for(attempt);
                tracing::warn!(
                    url = %url,
                    attempt = attempt + 1,
                    max = ctx.retry_policy.max_attempts,
                    retry_in_ms = delay.as_millis(),
                    error = %e,
                    "retrying URL"
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Newtype so `Arc<dyn Fetcher>` can be passed where `impl Fetcher + 'static` is required.
struct ArcFetcher(Arc<dyn Fetcher>);

#[async_trait::async_trait]
impl Fetcher for ArcFetcher {
    async fn fetch(&self, request: &crate::middleware::Request) -> Result<Response, KumoError> {
        self.0.fetch(request).await
    }
}

// ── Item Stream API ───────────────────────────────────────────────────────────

/// Internal `ItemStore` that forwards items into an mpsc channel.
/// Used by `CrawlEngine::stream()` — not part of the public API.
struct ChannelStore {
    tx: tokio::sync::mpsc::Sender<serde_json::Value>,
}

#[async_trait::async_trait]
impl crate::store::ItemStore for ChannelStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        // If the receiver was dropped (consumer cancelled), ignore the send error.
        self.tx.send(item.clone()).await.ok();
        Ok(())
    }
}

/// An async stream of scraped items returned by [`CrawlEngine::stream`].
///
/// Implements [`tokio_stream::Stream`]`<Item = serde_json::Value>`.
/// Use [`tokio_stream::StreamExt::next`] to consume items one by one.
///
/// Dropping this stream closes the channel, which causes the background
/// crawl engine to stop gracefully on its next attempted send.
pub struct ItemStream {
    inner: tokio_stream::wrappers::ReceiverStream<serde_json::Value>,
}

impl tokio_stream::Stream for ItemStream {
    type Item = serde_json::Value;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.inner).poll_next(cx)
    }
}
