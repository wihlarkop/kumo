use crate::{
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
    pub fn item(mut self, item: T) -> Result<Self, KumoError> {
        self.items.push(item);
        Ok(self)
    }

    /// Add multiple items.
    pub fn items(mut self, items: Vec<T>) -> Result<Self, KumoError> {
        self.items.extend(items);
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
        let output = Output::<Item>::new();
        assert!(output.items.is_empty());
        assert!(output.follow.is_empty());
    }

    #[test]
    fn default_output_is_empty() {
        let output = Output::<Item>::default();
        assert!(output.items.is_empty());
        assert!(output.follow.is_empty());
    }

    #[test]
    fn item_adds_to_list() {
        let output = Output::new().item(Item { value: 42 }).unwrap();
        assert_eq!(output.items.len(), 1);
    }

    #[test]
    fn items_adds_multiple() {
        let output = Output::new()
            .items(vec![Item { value: 1 }, Item { value: 2 }])
            .unwrap();
        assert_eq!(output.items.len(), 2);
    }

    #[test]
    fn follow_adds_url() {
        let output = Output::<Item>::new().follow("https://example.com/page/2");
        assert_eq!(output.follow, vec!["https://example.com/page/2"]);
    }

    #[test]
    fn follow_many_adds_multiple_urls() {
        let urls = vec![
            "https://example.com/1".to_string(),
            "https://example.com/2".to_string(),
        ];
        let output = Output::<Item>::new().follow_many(urls.clone());
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
