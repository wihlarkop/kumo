use std::{collections::HashMap, sync::Arc};

use super::{Fetcher, client_policy::HttpClientPolicy};
use crate::{
    error::KumoError,
    extract::{Response, response::ResponseBody},
    logging::{event, target},
    middleware::FetchRequest,
};
use reqwest::Client;
use tokio::sync::RwLock;

/// HTTP fetcher backed by `reqwest`. Handles TLS, redirects, and cookies
/// via the shared `Client` (which carries the cookie jar internally).
///
/// When `request.proxy` is set by a `ProxyRotator` middleware, the fetcher
/// lazily builds and caches a dedicated `Client` for that proxy URL so
/// connection pooling is preserved across requests through the same proxy.
/// Proxy clients inherit Kumo's User-Agent, pool size, request timeout, and
/// TCP keepalive policy while retaining isolated cookie jars and pools.
#[derive(Debug)]
pub struct HttpFetcher {
    client: Client,
    policy: HttpClientPolicy,
    proxy_clients: Arc<RwLock<HashMap<String, Client>>>,
}

impl HttpFetcher {
    pub fn new(client: Client, default_user_agent: impl Into<String>) -> Self {
        Self::with_policy(
            client,
            HttpClientPolicy::default_for(default_user_agent.into()),
        )
    }

    pub(crate) fn with_policy(client: Client, policy: HttpClientPolicy) -> Self {
        Self {
            client,
            policy,
            proxy_clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn client_for(&self, request: &FetchRequest) -> Result<Client, KumoError> {
        let Some(ref proxy_url) = request.proxy else {
            return Ok(self.client.clone());
        };

        // Fast path: proxy client already cached.
        {
            let cache = self.proxy_clients.read().await;
            if let Some(client) = cache.get(proxy_url) {
                return Ok(client.clone());
            }
        }

        let proxy = reqwest::Proxy::all(proxy_url.as_str()).map_err(KumoError::Fetch)?;
        let new_client = self
            .policy
            .reqwest_builder()
            .proxy(proxy)
            .build()
            .map_err(KumoError::Fetch)?;

        let mut cache = self.proxy_clients.write().await;
        Ok(cache.entry(proxy_url.clone()).or_insert(new_client).clone())
    }
}

#[async_trait::async_trait]
impl Fetcher for HttpFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<Response, KumoError> {
        let client = self.client_for(request).await?;

        let mut builder = client.request(request.method.clone(), request.url());

        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }

        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }

        let start = std::time::Instant::now();
        let res = builder.send().await.map_err(KumoError::Fetch)?;
        let status = res.status().as_u16();
        let headers = res.headers().clone();

        // Decode as text for text/* and application/json; store raw bytes otherwise.
        let is_text = super::is_text_content_type(
            headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
        );

        let body = if is_text {
            ResponseBody::Text(res.text().await.map_err(KumoError::Fetch)?)
        } else {
            ResponseBody::Bytes(res.bytes().await.map_err(KumoError::Fetch)?)
        };
        let elapsed = start.elapsed();
        let byte_count = match &body {
            ResponseBody::Text(s) => s.len() as u64,
            ResponseBody::Bytes(b) => b.len() as u64,
        };
        tracing::debug!(
            target: target::REQUEST,
            event = event::REQUEST_FETCH,
            url = %request.url(),
            method = %request.method,
            status,
            bytes = byte_count,
            elapsed_ms = elapsed.as_millis(),
            "request.fetch"
        );

        Ok(Response::new(
            request.url().to_string(),
            status,
            headers,
            elapsed,
            body,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::HttpFetcher;
    use crate::middleware::FetchRequest;

    #[test]
    fn public_constructor_remains_available() {
        let fetcher = HttpFetcher::new(reqwest::Client::new(), "test-agent");

        assert_eq!(fetcher.policy.concurrency(), 8);
    }

    #[tokio::test]
    async fn proxy_clients_are_cached_by_url() {
        let fetcher = HttpFetcher::new(reqwest::Client::new(), "test-agent");
        let mut request = FetchRequest::new("https://example.com", 0);
        request.proxy = Some("http://127.0.0.1:8080".to_string());

        let (first, second) =
            tokio::join!(fetcher.client_for(&request), fetcher.client_for(&request));

        assert!(first.is_ok());
        assert!(second.is_ok());
        assert_eq!(fetcher.proxy_clients.read().await.len(), 1);
    }
}
