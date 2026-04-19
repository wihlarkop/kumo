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

    /// Add a single serializable item. Returns an error if serialization fails.
    pub fn item<T: serde::Serialize>(mut self, item: T) -> Result<Self, KumoError> {
        let v = serde_json::to_value(item).map_err(|e| KumoError::parse("item serialization", e))?;
        self.items.push(v);
        Ok(self)
    }

    /// Add multiple serializable items. Returns an error if any item fails to serialize.
    pub fn items<T: serde::Serialize>(mut self, items: Vec<T>) -> Result<Self, KumoError> {
        for item in items {
            let v = serde_json::to_value(item).map_err(|e| KumoError::parse("item serialization", e))?;
            self.items.push(v);
        }
        Ok(self)
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
    async fn parse(&self, response: &Response) -> Result<Output, KumoError>;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Item {
        value: i32,
    }

    #[test]
    fn new_output_is_empty() {
        let output = Output::new();
        assert!(output.items.is_empty());
        assert!(output.follow.is_empty());
    }

    #[test]
    fn default_output_is_empty() {
        let output = Output::default();
        assert!(output.items.is_empty());
        assert!(output.follow.is_empty());
    }

    #[test]
    fn item_adds_serialized_value() {
        let output = Output::new().item(Item { value: 42 }).unwrap();
        assert_eq!(output.items.len(), 1);
        assert_eq!(output.items[0]["value"], 42);
    }

    #[test]
    fn items_adds_multiple_values() {
        let output = Output::new()
            .items(vec![Item { value: 1 }, Item { value: 2 }])
            .unwrap();
        assert_eq!(output.items.len(), 2);
        assert_eq!(output.items[0]["value"], 1);
        assert_eq!(output.items[1]["value"], 2);
    }

    #[test]
    fn follow_adds_url() {
        let output = Output::new().follow("https://example.com/page/2");
        assert_eq!(output.follow, vec!["https://example.com/page/2"]);
    }

    #[test]
    fn follow_many_adds_multiple_urls() {
        let urls = vec![
            "https://example.com/1".to_string(),
            "https://example.com/2".to_string(),
        ];
        let output = Output::new().follow_many(urls.clone());
        assert_eq!(output.follow, urls);
    }

    #[test]
    fn builder_is_chainable() {
        let output = Output::new()
            .item(Item { value: 99 })
            .unwrap()
            .follow("https://example.com/next");
        assert_eq!(output.items.len(), 1);
        assert_eq!(output.follow.len(), 1);
    }
}
