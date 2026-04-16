pub mod engine;
pub mod error;
pub mod extract;
pub mod fetch;
pub mod frontier;
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
    pub use crate::middleware::{DefaultHeaders, RateLimiter};
    pub use crate::spider::{Output, Spider};
    pub use crate::store::{JsonStore, JsonlStore, StdoutStore};
}
