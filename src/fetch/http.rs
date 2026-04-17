use super::Fetcher;
use crate::{error::KumoError, extract::Response, middleware::Request};
use reqwest::Client;

/// HTTP fetcher backed by `reqwest`. Handles TLS, redirects, and cookies
/// via the shared `Client` (which carries the cookie jar internally).
pub struct HttpFetcher {
    client: Client,
}

impl HttpFetcher {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl Fetcher for HttpFetcher {
    async fn fetch(&self, request: &Request) -> Result<Response, KumoError> {
        let mut builder = self.client.get(&request.url);

        // Merge headers injected by middleware.
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }

        let start = std::time::Instant::now();
        let res = builder.send().await.map_err(KumoError::Fetch)?;
        let status = res.status().as_u16();
        let headers = res.headers().clone();
        let body = res.text().await.map_err(KumoError::Fetch)?;
        let elapsed = start.elapsed();

        Ok(Response {
            url: request.url.clone(),
            status,
            headers,
            elapsed,
            body,
        })
    }
}
