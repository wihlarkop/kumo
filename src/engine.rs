use std::{sync::Arc, time::Duration};
use tokio::task::JoinSet;
use tracing::{error, info};

use crate::{
    error::{ErrorPolicy, KumoError},
    fetch::{Fetcher, http::HttpFetcher},
    frontier::{Frontier, memory::MemoryFrontier},
    middleware::{Middleware, Request},
    pipeline::Pipeline,
    robots::RobotsCache,
    spider::Spider,
    store::ItemStore,
};

type FrontierOverride = Option<Arc<dyn Frontier>>;

#[cfg(feature = "browser")]
use crate::fetch::{BrowserConfig, BrowserFetcher};

/// Statistics returned by `CrawlEngine::run` after the crawl finishes.
#[derive(Debug, Default)]
pub struct CrawlStats {
    pub pages_crawled: u64,
    pub items_scraped: u64,
    pub errors: u64,
    pub duration: Duration,
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
    max_retries: u32,
    retry_base_delay: Duration,
    frontier: FrontierOverride,
    /// Expected unique URL count for Bloom filter sizing (default: 1_000_000).
    max_urls: usize,
    robots_ttl: Duration,
    #[cfg(feature = "browser")]
    browser: Option<BrowserConfig>,
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
            max_retries: 0,
            retry_base_delay: Duration::from_millis(500),
            max_urls: 1_000_000,
            robots_ttl: Duration::from_secs(24 * 60 * 60),
            #[cfg(feature = "browser")]
            browser: None,
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

    /// Retry failed fetches up to `max_attempts` times with exponential backoff.
    ///
    /// Delay between attempts: `base_delay * 2^attempt` (500ms, 1s, 2s, …).
    pub fn retry(mut self, max_attempts: u32, base_delay: Duration) -> Self {
        self.max_retries = max_attempts;
        self.retry_base_delay = base_delay;
        self
    }

    /// Whether to respect robots.txt (default: true).
    pub fn respect_robots_txt(mut self, v: bool) -> Self {
        self.respect_robots = v;
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

    /// Consume the engine, run the spider, and return crawl statistics.
    pub async fn run<S>(self, spider: S) -> Result<CrawlStats, KumoError>
    where
        S: Spider + 'static,
    {
        let start = std::time::Instant::now();
        let spider: Arc<dyn Spider> = Arc::new(spider);
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
        let max_retries = self.max_retries;
        let retry_base_delay = self.retry_base_delay;
        let robots_cache = if self.respect_robots {
            Some(Arc::new(RobotsCache::with_ttl(
                concat!("kumo/", env!("CARGO_PKG_VERSION")),
                self.robots_ttl,
            )))
        } else {
            None
        };

        // Single shared reqwest client — used for robots.txt and plain HTTP fetching.
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .user_agent(concat!("kumo/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(KumoError::Fetch)?;

        // Build the fetcher: browser (CDP) if configured, otherwise plain HTTP.
        #[cfg(not(feature = "browser"))]
        let fetcher: Arc<dyn Fetcher> = Arc::new(HttpFetcher::new(
            client.clone(),
            concat!("kumo/", env!("CARGO_PKG_VERSION")),
        ));

        #[cfg(feature = "browser")]
        let fetcher: Arc<dyn Fetcher> = match self.browser {
            Some(cfg) => Arc::new(BrowserFetcher::launch(cfg, concurrency).await?),
            None => Arc::new(HttpFetcher::new(
                client.clone(),
                concat!("kumo/", env!("CARGO_PKG_VERSION")),
            )),
        };

        info!(spider = spider.name(), "starting crawl");
        for url in spider.start_urls() {
            frontier.push(url, 0).await;
        }

        type TaskResult = (
            String,
            usize,
            u32,
            Result<(u64, Vec<(String, usize)>), KumoError>,
        );
        let mut join_set: JoinSet<TaskResult> = JoinSet::new();
        let mut stats = CrawlStats::default();

        loop {
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
                            max_retries,
                            retry_base_delay,
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

            // Both the queue is empty and no tasks are running → crawl complete.
            if join_set.is_empty() {
                break;
            }

            // Wait for the next task to finish, then process its output.
            match join_set.join_next().await {
                Some(Ok((_url, _depth, _retry_count, Ok((item_count, follows))))) => {
                    stats.pages_crawled += 1;
                    stats.items_scraped += item_count;

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
                                url = %url,
                                attempt = retry_count + 1,
                                max,
                                error = %e,
                                "re-queuing failed URL"
                            );
                            frontier.push_force(url, depth, retry_count + 1).await;
                        }
                        ErrorPolicy::Retry(_) => {
                            error!(url = %url, error = %e, "retry limit reached, skipping URL");
                        }
                        ErrorPolicy::Skip => {
                            error!(url = %url, error = %e, "skipping URL");
                        }
                    }
                }
                Some(Err(join_err)) => {
                    stats.errors += 1;
                    error!(error = %join_err, "crawl task panicked");
                }
                None => break,
            }
        }

        store.flush().await?;
        stats.duration = start.elapsed();
        info!(
            pages = stats.pages_crawled,
            items = stats.items_scraped,
            errors = stats.errors,
            duration_secs = stats.duration.as_secs_f64(),
            "crawl complete"
        );

        Ok(stats)
    }
}

/// Shared context cloned into each spawned task.
struct TaskContext {
    spider: Arc<dyn Spider>,
    store: Arc<dyn ItemStore>,
    middleware: Arc<Vec<Arc<dyn Middleware>>>,
    pipelines: Arc<Vec<Arc<dyn Pipeline>>>,
    fetcher: Arc<dyn Fetcher>,
    crawl_delay: Option<Duration>,
    max_retries: u32,
    retry_base_delay: Duration,
}

/// Fetch, run middleware, parse, and store items for a single URL.
/// Returns (items_stored, follow_urls_with_depth).
async fn process_url(
    url: &str,
    depth: usize,
    ctx: &TaskContext,
) -> Result<(u64, Vec<(String, usize)>), KumoError> {
    if let Some(delay) = ctx.crawl_delay {
        tokio::time::sleep(delay).await;
    }

    let mut request = Request::new(url, depth);
    for mw in ctx.middleware.iter() {
        mw.before_request(&mut request).await?;
    }

    let mut response = ctx.fetcher.fetch(&request).await?;

    for mw in ctx.middleware.iter() {
        mw.after_response(&mut response).await?;
    }

    let output = ctx.spider.parse(response).await?;

    let mut item_count = 0u64;
    'items: for item in output.items {
        let mut current = item;
        for pipeline in ctx.pipelines.iter() {
            match pipeline.process(current).await {
                Ok(Some(v)) => current = v,
                Ok(None) => continue 'items, // dropped by pipeline
                Err(e) => {
                    tracing::warn!(error = %e, "pipeline dropped item due to error");
                    continue 'items;
                }
            }
        }
        ctx.store.store(&current).await?;
        item_count += 1;
    }

    let follows = output.follow.into_iter().map(|u| (u, depth + 1)).collect();

    Ok((item_count, follows))
}

/// Wraps `process_url` with exponential-backoff retry.
async fn process_url_with_retry(
    url: String,
    depth: usize,
    ctx: TaskContext,
) -> Result<(u64, Vec<(String, usize)>), KumoError> {
    for attempt in 0..=ctx.max_retries {
        match process_url(&url, depth, &ctx).await {
            Ok(result) => return Ok(result),
            Err(e) if attempt < ctx.max_retries => {
                // Notify middleware of this attempt's failure before backing off.
                for mw in ctx.middleware.iter() {
                    mw.on_error(&url, &e).await;
                }
                let delay = ctx.retry_base_delay * 2_u32.pow(attempt);
                tracing::warn!(
                    url = %url,
                    attempt = attempt + 1,
                    max = ctx.max_retries,
                    retry_in_ms = delay.as_millis(),
                    error = %e,
                    "retrying URL"
                );
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
