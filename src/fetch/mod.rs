pub mod http;

#[cfg(feature = "browser")]
pub mod browser;

pub use http::HttpFetcher;

#[cfg(feature = "browser")]
pub use browser::{BrowserConfig, BrowserFetcher};

use crate::{error::KumoError, extract::Response, middleware::Request};

/// Abstracts over different HTTP backends.
#[async_trait::async_trait]
pub trait Fetcher: Send + Sync {
    async fn fetch(&self, request: &Request) -> Result<Response, KumoError>;
}
