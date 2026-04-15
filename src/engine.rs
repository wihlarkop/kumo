use std::{sync::Arc, time::Duration};
use tokio::task::JoinSet;
use tracing::{error, info};

use crate::{
    error::{ErrorPolicy, KumoError},
    fetch::{http::HttpFetcher, Fetcher},
    frontier::{memory::MemoryFrontier, Frontier},
    middleware::{Middleware, Request},
    spider::Spider,
    store::ItemStore,
};

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
/// let stats = CrawlEngine::new()
///     .concurrency(8)
///     .middleware(DefaultHeaders::new().user_agent("kumo/0.1"))
///     .store(JsonlStore::new("items.jsonl"))
///     .run(MySpider)
///     .await?;
/// ```
pub struct CrawlEngine {
    concurrency: usize,
    middleware: Vec<Arc<dyn Middleware>>,
    store: Arc<dyn ItemStore>,
    crawl_delay: Option<Duration>,
}

impl CrawlEngine {
    /// Begin building a new engine. Defaults: concurrency=8, StdoutStore, no delay.
    pub fn new() -> CrawlEngineBuilder {
        CrawlEngineBuilder::default()
    }
}

impl Default for CrawlEngine {
    fn default() -> Self {
        unimplemented!("use CrawlEngine::new() to build")
    }
}

pub struct CrawlEngineBuilder {
    concurrency: usize,
    middleware: Vec<Arc<dyn Middleware>>,
    store: Option<Arc<dyn ItemStore>>,
    crawl_delay: Option<Duration>,
    #[allow(dead_code)]
    respect_robots: bool,
}

impl Default for CrawlEngineBuilder {
    fn default() -> Self {
        Self {
            concurrency: 8,
            middleware: Vec::new(),
            store: None,
            crawl_delay: None,
            respect_robots: true,
        }
    }
}

impl CrawlEngineBuilder {
    pub fn concurrency(mut self, n: usize) -> Self {
        self.concurrency = n;
        self
    }

    /// Register a middleware (applied in registration order).
    pub fn middleware(mut self, mw: impl Middleware + 'static) -> Self {
        self.middleware.push(Arc::new(mw));
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

    /// Whether to respect robots.txt (stored for future use; not yet implemented).
    pub fn respect_robots_txt(self, _v: bool) -> Self {
        // TODO: implement robots.txt parsing in a follow-up task
        self
    }

    /// Consume the builder, run the spider, and return crawl statistics.
    pub async fn run<S>(self, spider: S) -> Result<CrawlStats, KumoError>
    where
        S: Spider + 'static,
    {
        let store = self
            .store
            .unwrap_or_else(|| Arc::new(crate::store::stdout::StdoutStore));

        let engine = CrawlEngine {
            concurrency: self.concurrency,
            middleware: self.middleware,
            store,
            crawl_delay: self.crawl_delay,
        };

        engine.execute(spider).await
    }
}

impl CrawlEngine {
    async fn execute<S: Spider + 'static>(self, spider: S) -> Result<CrawlStats, KumoError> {
        let start = std::time::Instant::now();
        let spider: Arc<dyn Spider> = Arc::new(spider);
        let frontier = Arc::new(MemoryFrontier::new());
        let store = self.store;
        let middleware: Arc<Vec<Arc<dyn Middleware>>> = Arc::new(self.middleware);
        let crawl_delay = self.crawl_delay;
        let concurrency = self.concurrency;

        // Single shared reqwest client — handles cookie jar + connection pooling.
        let client = reqwest::Client::builder()
            .cookie_store(true)
            .user_agent(concat!("kumo/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(KumoError::Fetch)?;

        info!(spider = spider.name(), "starting crawl");
        for url in spider.start_urls() {
            frontier.push(url, 0).await;
        }

        type TaskResult = (String, Result<(u64, Vec<(String, usize)>), KumoError>);
        let mut join_set: JoinSet<TaskResult> = JoinSet::new();
        let mut stats = CrawlStats::default();

        loop {
            // Fill up to the concurrency limit.
            while join_set.len() < concurrency {
                match frontier.pop().await {
                    Some((url, depth)) => {
                        let spider = spider.clone();
                        let store = store.clone();
                        let middleware = middleware.clone();
                        let client = client.clone();

                        join_set.spawn(async move {
                            let result =
                                process_url(url.clone(), depth, spider, store, middleware, client, crawl_delay)
                                    .await;
                            (url, result)
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
                Some(Ok((_url, Ok((item_count, follows))))) => {
                    stats.pages_crawled += 1;
                    stats.items_scraped += item_count;

                    for (follow_url, follow_depth) in follows {
                        // Respect max_depth.
                        if let Some(max) = spider.max_depth() {
                            if follow_depth > max {
                                continue;
                            }
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
                Some(Ok((url, Err(e)))) => {
                    stats.errors += 1;
                    match spider.on_error(&url, &e) {
                        ErrorPolicy::Abort => {
                            error!(url = %url, error = %e, "aborting crawl");
                            return Err(e);
                        }
                        ErrorPolicy::Retry(_) => {
                            // TODO: implement proper retry with backoff countdown
                            error!(url = %url, error = %e, "retry not yet implemented, skipping");
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

/// Fetch, run middleware, parse, and store items for a single URL.
/// Returns (items_stored, follow_urls_with_depth).
async fn process_url(
    url: String,
    depth: usize,
    spider: Arc<dyn Spider>,
    store: Arc<dyn ItemStore>,
    middleware: Arc<Vec<Arc<dyn Middleware>>>,
    client: reqwest::Client,
    crawl_delay: Option<Duration>,
) -> Result<(u64, Vec<(String, usize)>), KumoError> {
    if let Some(delay) = crawl_delay {
        tokio::time::sleep(delay).await;
    }

    let mut request = Request::new(&url, depth);
    for mw in middleware.iter() {
        mw.before_request(&mut request).await?;
    }

    let fetcher = HttpFetcher::new(client);
    let mut response = fetcher.fetch(&request).await?;

    for mw in middleware.iter() {
        mw.after_response(&mut response).await?;
    }

    let output = spider.parse(response).await?;

    let mut item_count = 0u64;
    for item in &output.items {
        store.store(item).await?;
        item_count += 1;
    }

    let follows = output
        .follow
        .into_iter()
        .map(|u| (u, depth + 1))
        .collect();

    Ok((item_count, follows))
}
