use std::{collections::HashMap, sync::Arc};

use super::Fetcher;
use crate::{
    error::KumoError,
    extract::{Response, response::ResponseBody},
    middleware::Request,
};
use reqwest::Client;
use tokio::sync::RwLock;

/// HTTP fetcher backed by `reqwest`. Handles TLS, redirects, and cookies
/// via the shared `Client` (which carries the cookie jar internally).
///
/// When `request.proxy` is set by a `ProxyRotator` middleware, the fetcher
/// lazily builds and caches a dedicated `Client` for that proxy URL so
/// connection pooling is preserved across requests through the same proxy.
/// Proxy clients inherit the same User-Agent as the default client.
pub struct HttpFetcher {
    client: Client,
    default_user_agent: String,
    proxy_clients: Arc<RwLock<HashMap<String, Client>>>,
}

impl HttpFetcher {
    pub fn new(client: Client, default_user_agent: impl Into<String>) -> Self {
        Self {
            client,
            default_user_agent: default_user_agent.into(),
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

        // Slow path: build and cache a new client for this proxy,
        // inheriting the same UA and cookie settings as the default client.
        let proxy = reqwest::Proxy::all(proxy_url.as_str()).map_err(KumoError::Fetch)?;
        let new_client = Client::builder()
            .cookie_store(true)
            .user_agent(&self.default_user_agent)
            .proxy(proxy)
            .build()
            .map_err(KumoError::Fetch)?;

        let mut cache = self.proxy_clients.write().await;
        Ok(cache.entry(proxy_url.clone()).or_insert(new_client).clone())
    }
}

#[async_trait::async_trait]
impl Fetcher for HttpFetcher {
    async fn fetch(&self, request: &Request) -> Result<Response, KumoError> {
        let client = self.client_for(request).await?;

        let mut builder = client.get(request.url());

        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }

        let start = std::time::Instant::now();
        let res = builder.send().await.map_err(KumoError::Fetch)?;
        let status = res.status().as_u16();
        let headers = res.headers().clone();

        // Decode as text for text/* and application/json; store raw bytes otherwise.
        let is_text = headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.starts_with("text/") || ct.contains("application/json"))
            .unwrap_or(true); // assume text when Content-Type is absent

        let body = if is_text {
            ResponseBody::Text(res.text().await.map_err(KumoError::Fetch)?)
        } else {
            ResponseBody::Bytes(res.bytes().await.map_err(KumoError::Fetch)?)
        };
        let elapsed = start.elapsed();

        Ok(Response::new(
            request.url().to_string(),
            status,
            headers,
            elapsed,
            body,
        ))
    }
}
