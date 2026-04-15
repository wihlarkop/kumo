pub mod http;

pub use http::HttpFetcher;

use crate::{error::KumoError, extract::Response, middleware::Request};

/// Abstracts over different HTTP backends.
/// v0.1 provides `HttpFetcher`; v0.2 will add a headless-browser fetcher.
#[async_trait::async_trait]
pub trait Fetcher: Send + Sync {
    async fn fetch(&self, request: &Request) -> Result<Response, KumoError>;
}
