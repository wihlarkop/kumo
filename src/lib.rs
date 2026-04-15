pub mod engine;
pub mod error;
pub mod extract;
pub mod fetch;
pub mod frontier;
pub mod middleware;
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
    pub use crate::extract::{Element, ElementList, Response};
    pub use crate::middleware::DefaultHeaders;
    pub use crate::spider::{Output, Spider};
    pub use crate::store::{JsonlStore, StdoutStore};
}
