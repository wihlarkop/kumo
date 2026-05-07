use crate::{
    engine::CrawlStats,
    error::{ErrorPolicy, KumoError},
    extract::Response,
};

/// Carries extracted items and URLs to follow — returned by `Spider::parse`.
///
/// `T` is the item type declared by the spider via `type Item = MyItem`.
/// Items are stored as `T` and serialized to JSON exactly once when handed
/// to the item-pipeline / store, avoiding redundant allocations.
pub struct Output<T: serde::Serialize> {
    pub(crate) items: Vec<T>,
    /// URLs to enqueue for crawling.
    pub follow: Vec<String>,
}

impl<T: serde::Serialize> Output<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            follow: Vec::new(),
        }
    }

    /// Add a single item.
    pub fn item(mut self, item: T) -> Self {
        self.items.push(item);
        self
    }

    /// Add multiple items.
    pub fn items(mut self, items: Vec<T>) -> Self {
        self.items.extend(items);
        self
    }

    /// Enqueue a single URL to follow.
    pub fn follow(mut self, url: impl Into<String>) -> Self {
        self.follow.push(url.into());
        self
    }

    /// Enqueue multiple URLs to follow.
    pub fn follow_many(mut self, urls: Vec<String>) -> Self {
        self.follow.extend(urls);
        self
    }
}

impl<T: serde::Serialize> Default for Output<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// The primary interface users implement to define a spider.
///
/// # Minimal example
/// ```rust,ignore
/// use kumo::prelude::*;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Article { title: String }
///
/// struct MySite;
///
/// #[async_trait::async_trait]
/// impl Spider for MySite {
///     type Item = Article;
///
///     fn name(&self) -> &str { "my-site" }
///     fn start_urls(&self) -> Vec<String> { vec!["https://example.com".into()] }
///
///     async fn parse(&self, response: &Response) -> Result<Output<Self::Item>, KumoError> {
///         let title = response.css("h1").first().map(|e| e.text()).unwrap_or_default();
///         Output::new().item(Article { title })
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait Spider: Send + Sync {
    /// The type of item emitted by `parse`. Must implement `serde::Serialize`.
    /// Use `type Item = serde_json::Value` for untyped / ad-hoc items.
    type Item: serde::Serialize + Send;

    /// Unique identifier for this spider (used in logs).
    fn name(&self) -> &str;

    /// Seed URLs to begin crawling from.
    fn start_urls(&self) -> Vec<String>;

    /// Called for every successfully fetched page.
    async fn parse(&self, response: &Response) -> Result<Output<Self::Item>, KumoError>;

    /// Error handling policy for fetch or parse failures. Default: skip and log.
    fn on_error(&self, _url: &str, _err: &KumoError) -> ErrorPolicy {
        ErrorPolicy::Skip
    }

    /// Maximum crawl depth. `None` = unlimited.
    fn max_depth(&self) -> Option<usize> {
        None
    }

    /// Allowed domains. Empty = allow all.
    fn allowed_domains(&self) -> Vec<&str> {
        vec![]
    }

    /// Called once before the first URL is fetched.
    /// Use for setup: open connections, log "starting", validate config.
    async fn open(&self) -> Result<(), KumoError> {
        Ok(())
    }

    /// Called once after the crawl finishes (or is interrupted).
    /// `stats` contains the final crawl statistics.
    /// Use for cleanup: send notifications, flush custom buffers, log summary.
    async fn close(&self, _stats: &CrawlStats) -> Result<(), KumoError> {
        Ok(())
    }
}
