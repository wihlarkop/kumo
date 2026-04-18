use std::{collections::HashMap, sync::Arc};

use super::Fetcher;
use crate::{error::KumoError, extract::Response, middleware::Request};
use reqwest::Client;
use tokio::sync::RwLock;

/// HTTP fetcher backed by `reqwest`. Handles TLS, redirects, and cookies
/// via the shared `Client` (which carries the cookie jar internally).
///
/// When `request.proxy` is set by a `ProxyRotator` middleware, the fetcher
/// lazily builds and caches a dedicated `Client` for that proxy URL so
/// connection pooling is preserved across requests through the same proxy.
pub struct HttpFetcher {
    client: Client,
    proxy_clients: Arc<RwLock<HashMap<String, Client>>>,
}

impl HttpFetcher {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            proxy_clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn client_for(&self, request: &Request) -> Result<Client, KumoError> {
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

        // Slow path: build and cache a new client for this proxy.
        let proxy = reqwest::Proxy::all(proxy_url.as_str())
            .map_err(KumoError::Fetch)?;
        let new_client = Client::builder()
            .cookie_store(true)
            .proxy(proxy)
            .build()
            .map_err(KumoError::Fetch)?;

        let mut cache = self.proxy_clients.write().await;
        // Another task may have inserted it while we waited for the write lock.
        Ok(cache
            .entry(proxy_url.clone())
            .or_insert(new_client)
            .clone())
    }
}

#[async_trait::async_trait]
impl Fetcher for HttpFetcher {
    async fn fetch(&self, request: &Request) -> Result<Response, KumoError> {
        let client = self.client_for(request).await?;

        let mut builder = client.get(&request.url);

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
