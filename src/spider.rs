use crate::{
    error::{ErrorPolicy, KumoError},
    extract::Response,
};

/// Carries extracted items and URLs to follow — returned by `Spider::parse`.
pub struct Output {
    /// Extracted data items, pre-serialized to JSON.
    pub items: Vec<serde_json::Value>,
    /// URLs to enqueue for crawling.
    pub follow: Vec<String>,
}

impl Output {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            follow: Vec::new(),
        }
    }

    /// Add a single serializable item.
    pub fn item<T: serde::Serialize>(mut self, item: T) -> Self {
        if let Ok(v) = serde_json::to_value(item) {
            self.items.push(v);
        }
        self
    }

    /// Add multiple serializable items.
    pub fn items<T: serde::Serialize>(mut self, items: Vec<T>) -> Self {
        for item in items {
            if let Ok(v) = serde_json::to_value(item) {
                self.items.push(v);
            }
        }
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

impl Default for Output {
    fn default() -> Self {
        Self::new()
    }
}

/// The primary interface users implement to define a spider.
///
/// Modeled after Scrapy's Spider class, but idiomatic Rust:
/// trait-based, async, strongly typed.
#[async_trait::async_trait]
pub trait Spider: Send + Sync {
    /// Unique identifier for this spider (used in logs).
    fn name(&self) -> &str;

    /// Seed URLs to begin crawling from.
    fn start_urls(&self) -> Vec<String>;

    /// Called for every successfully fetched page.
    async fn parse(&self, response: Response) -> Result<Output, KumoError>;

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
}
