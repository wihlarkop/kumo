pub mod engine;
pub mod error;
pub mod extract;
pub mod fetch;
pub mod frontier;
#[cfg(feature = "persistence")]
pub use frontier::FileFrontier;
#[cfg(feature = "redis-frontier")]
pub use frontier::RedisFrontier;
pub mod llm;
pub mod middleware;
#[cfg(feature = "otel")]
pub mod otel;
pub mod pipeline;
pub mod retry;
pub mod robots;
pub mod sitemap;
pub mod spider;
pub mod store;

/// Convenience re-exports for writing spiders with minimal `use` statements.
///
/// ```rust,ignore
/// use kumo::prelude::*;
/// ```
pub mod prelude {
    pub use crate::engine::{CrawlEngine, CrawlStats, ItemStream};
    pub use crate::error::{ErrorPolicy, KumoError};
    #[cfg(feature = "derive")]
    pub use crate::extract::Extract;
    #[cfg(feature = "jsonpath")]
    pub use crate::extract::JsonPathExtractor;
    pub use crate::extract::{
        CssExtractor, Element, ElementList, ExtractedNode, Extractor, LinkExtractor,
        RegexExtractor, Response, ValueExtractor,
    };
    #[cfg(feature = "browser")]
    pub use crate::fetch::{BrowserConfig, BrowserFetcher};
    pub use crate::fetch::{CachingFetcher, MockFetcher};
    #[cfg(feature = "stealth")]
    pub use crate::fetch::{StealthHttpFetcher, StealthProfile};
    #[cfg(feature = "claude")]
    pub use crate::llm::AnthropicClient;
    #[cfg(feature = "gemini")]
    pub use crate::llm::GeminiClient;
    #[cfg(feature = "ollama")]
    pub use crate::llm::OllamaClient;
    #[cfg(feature = "openai")]
    pub use crate::llm::OpenAiClient;
    #[cfg(feature = "llm")]
    pub use crate::llm::ResponseExtractExt;
    pub use crate::llm::{LlmClient, TokenUsage};
    pub use crate::middleware::{
        AutoThrottle, DefaultHeaders, Middleware, ProxyRotator, RateLimiter, Request, StatusRetry,
        UserAgentRotator,
    };
    pub use crate::pipeline::{DropDuplicates, FilterPipeline, Pipeline, RequireFields};
    pub use crate::retry::RetryPolicy;
    pub use crate::sitemap::{SitemapEntry, SitemapSpider};
    pub use crate::spider::{Output, Spider};
    #[cfg(feature = "mysql")]
    pub use crate::store::MySqlStore;
    #[cfg(feature = "postgres")]
    pub use crate::store::PostgresStore;
    #[cfg(feature = "sqlite")]
    pub use crate::store::SqliteStore;
    pub use crate::store::{CsvStore, JsonStore, JsonlStore, StdoutStore};
    #[cfg(feature = "derive")]
    pub use kumo_derive::Extract as ExtractDerive;
    pub use tokio_stream::StreamExt;
}
