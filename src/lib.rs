pub mod engine;
pub mod error;
pub mod extract;
pub mod fetch;
pub mod frontier;
#[cfg(feature = "llm")]
pub mod llm;
pub mod middleware;
pub mod robots;
pub mod spider;
pub mod store;

/// Convenience re-exports for writing spiders with minimal `use` statements.
///
/// ```rust,ignore
/// use kumo::prelude::*;
/// ```
pub mod prelude {
    pub use crate::engine::{CrawlEngine, CrawlStats};
    pub use crate::error::{ErrorPolicy, KumoError};
    pub use crate::extract::{
        CssExtractor, Element, ElementList, ExtractedNode, Extractor, Response,
    };
    #[cfg(feature = "claude")]
    pub use crate::llm::AnthropicClient;
    #[cfg(feature = "gemini")]
    pub use crate::llm::GeminiClient;
    #[cfg(feature = "llm")]
    pub use crate::llm::ResponseExtractExt;
    #[cfg(feature = "ollama")]
    pub use crate::llm::OllamaClient;
    #[cfg(feature = "openai")]
    pub use crate::llm::OpenAiClient;
    pub use crate::middleware::{AutoThrottle, DefaultHeaders, RateLimiter};
    pub use crate::spider::{Output, Spider};
    #[cfg(feature = "mysql")]
    pub use crate::store::MySqlStore;
    #[cfg(feature = "postgres")]
    pub use crate::store::PostgresStore;
    #[cfg(feature = "sqlite")]
    pub use crate::store::SqliteStore;
    pub use crate::store::{JsonStore, JsonlStore, StdoutStore};
}
