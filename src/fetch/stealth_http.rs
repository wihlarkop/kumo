//! HTTP fetcher with TLS/HTTP2 fingerprint spoofing via [`rquest`].
//!
//! Requires the `stealth` feature flag. Building with `stealth` also requires
//! cmake and NASM (for BoringSSL compilation) to be present on the system.
//!
//! [`rquest`]: https://crates.io/crates/rquest

use super::Fetcher;
use crate::{
    error::KumoError,
    extract::{Response, response::ResponseBody},
    middleware::Request,
};
use rquest_util::Emulation;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A browser profile for TLS + HTTP/2 fingerprint impersonation.
///
/// Each variant matches a real browser's exact TLS extension ordering,
/// cipher suites, ALPN, and HTTP/2 SETTINGS frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StealthProfile {
    /// Chrome 131 on Windows 10 (recommended for most sites).
    Chrome131,
    /// Firefox 128 LTS.
    Firefox128,
    /// Safari 18 on macOS Sequoia.
    Safari18,
    /// Microsoft Edge 127.
    Edge127,
}

impl StealthProfile {
    fn to_emulation(self) -> Emulation {
        match self {
            Self::Chrome131 => Emulation::Chrome131,
            Self::Firefox128 => Emulation::Firefox128,
            Self::Safari18 => Emulation::Safari18,
            Self::Edge127 => Emulation::Edge127,
        }
    }
}

/// HTTP fetcher with TLS + HTTP/2 fingerprint spoofing.
///
/// Wraps [`rquest`] which compiles BoringSSL and reproduces the exact TLS
/// client hello + HTTP/2 SETTINGS of real browsers, defeating JA3/JA4 detection.
///
/// # Example
/// ```rust,ignore
/// CrawlEngine::builder()
///     .stealth(StealthProfile::Chrome131)
///     .run(MySpider)
///     .await?;
/// ```
pub struct StealthHttpFetcher {
    client: rquest::Client,
    proxy_clients: Arc<RwLock<HashMap<String, rquest::Client>>>,
    profile: StealthProfile,
}

impl StealthHttpFetcher {
    pub fn new(profile: StealthProfile) -> Result<Self, KumoError> {
        let client = rquest::Client::builder()
            .emulation(profile.to_emulation())
            .cookie_store(true)
            .build()
            .map_err(|e| KumoError::Browser(format!("stealth client: {e}")))?;

        Ok(Self {
            client,
            proxy_clients: Arc::new(RwLock::new(HashMap::new())),
            profile,
        })
    }

    async fn client_for(&self, request: &Request) -> Result<rquest::Client, KumoError> {
        let Some(ref proxy_url) = request.proxy else {
            return Ok(self.client.clone());
        };

        {
            let cache = self.proxy_clients.read().await;
            if let Some(client) = cache.get(proxy_url) {
                return Ok(client.clone());
            }
        }

        let proxy = rquest::Proxy::all(proxy_url.as_str())
            .map_err(|e| KumoError::Browser(format!("stealth proxy: {e}")))?;
        let new_client = rquest::Client::builder()
            .emulation(self.profile.to_emulation())
            .cookie_store(true)
            .proxy(proxy)
            .build()
            .map_err(|e| KumoError::Browser(format!("stealth proxy client: {e}")))?;

        let mut cache = self.proxy_clients.write().await;
        Ok(cache.entry(proxy_url.clone()).or_insert(new_client).clone())
    }
}

#[async_trait::async_trait]
impl Fetcher for StealthHttpFetcher {
    async fn fetch(&self, request: &Request) -> Result<Response, KumoError> {
        let client = self.client_for(request).await?;

        let mut builder = client.get(request.url());
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.to_str().unwrap_or(""));
        }

        let start = std::time::Instant::now();
        let res = builder
            .send()
            .await
            .map_err(|e| KumoError::Browser(format!("stealth fetch: {e}")))?;
        let status = res.status().as_u16();

        // Convert rquest headers to reqwest headers before consuming the response body.
        let headers = {
            let mut h = reqwest::header::HeaderMap::new();
            for (name, value) in res.headers() {
                if let (Ok(n), Ok(v)) = (
                    reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()),
                    reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
                ) {
                    h.insert(n, v);
                }
            }
            h
        };

        let is_text = super::is_text_content_type(
            headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
        );

        let body = if is_text {
            ResponseBody::Text(
                res.text()
                    .await
                    .map_err(|e| KumoError::Browser(format!("stealth body: {e}")))?,
            )
        } else {
            ResponseBody::Bytes(
                res.bytes()
                    .await
                    .map_err(|e| KumoError::Browser(format!("stealth body: {e}")))?
                    .into(),
            )
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
