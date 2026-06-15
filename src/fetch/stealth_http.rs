//! HTTP fetcher with TLS/HTTP2 fingerprint spoofing via [`wreq`].
//!
//! Requires the `stealth` feature flag. Building with `stealth` also requires
//! cmake and NASM (for BoringSSL compilation) to be present on the system.
//!
//! [`wreq`]: https://crates.io/crates/wreq

use super::{Fetcher, client_policy::HttpClientPolicy};
use crate::{
    error::KumoError,
    extract::{Response, response::ResponseBody},
    middleware::FetchRequest,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use wreq::{Client, Method};
use wreq_util::Emulation;

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
/// Wraps [`wreq`] which compiles BoringSSL and reproduces the exact TLS
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
    client: Client,
    proxy_clients: Arc<RwLock<HashMap<String, Client>>>,
    profile: StealthProfile,
    policy: HttpClientPolicy,
}

impl StealthHttpFetcher {
    pub fn new(profile: StealthProfile) -> Result<Self, KumoError> {
        Self::with_policy(
            profile,
            HttpClientPolicy::default_for(crate::engine::USER_AGENT),
        )
    }

    pub(crate) fn with_policy(
        profile: StealthProfile,
        policy: HttpClientPolicy,
    ) -> Result<Self, KumoError> {
        let client = policy
            .wreq_builder(profile.to_emulation())
            .build()
            .map_err(|e| KumoError::Browser(format!("stealth client: {e}")))?;

        Ok(Self {
            client,
            proxy_clients: Arc::new(RwLock::new(HashMap::new())),
            profile,
            policy,
        })
    }

    async fn client_for(&self, request: &FetchRequest) -> Result<Client, KumoError> {
        let Some(ref proxy_url) = request.proxy else {
            return Ok(self.client.clone());
        };

        {
            let cache = self.proxy_clients.read().await;
            if let Some(client) = cache.get(proxy_url) {
                return Ok(client.clone());
            }
        }

        let new_client = self
            .policy
            .wreq_builder(self.profile.to_emulation())
            .proxy(proxy_url)
            .build()
            .map_err(|e| KumoError::Browser(format!("stealth proxy client: {e}")))?;

        let mut cache = self.proxy_clients.write().await;
        Ok(cache.entry(proxy_url.clone()).or_insert(new_client).clone())
    }
}

fn to_wreq_method(method: &reqwest::Method) -> Result<Method, KumoError> {
    Method::from_bytes(method.as_str().as_bytes())
        .map_err(|e| KumoError::Browser(format!("stealth method: {e}")))
}

#[async_trait::async_trait]
impl Fetcher for StealthHttpFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<Response, KumoError> {
        let client = self.client_for(request).await?;

        let mut builder = client.request(to_wreq_method(&request.method)?, request.url());
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.to_str().unwrap_or(""));
        }
        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }

        let start = std::time::Instant::now();
        let res = builder
            .send()
            .await
            .map_err(|e| KumoError::Browser(format!("stealth fetch: {e}")))?;
        let status = res.status().as_u16();

        // Convert wreq headers to reqwest headers before consuming the response body.
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

#[cfg(test)]
mod tests {
    use super::{StealthHttpFetcher, StealthProfile};
    use crate::middleware::FetchRequest;
    use wreq_util::Emulation;

    #[test]
    fn stealth_profiles_map_to_expected_wreq_emulations() {
        let cases = [
            (StealthProfile::Chrome131, Emulation::Chrome131),
            (StealthProfile::Firefox128, Emulation::Firefox128),
            (StealthProfile::Safari18, Emulation::Safari18),
            (StealthProfile::Edge127, Emulation::Edge127),
        ];

        for (profile, expected) in cases {
            assert_eq!(profile.to_emulation(), expected);
        }
    }

    #[test]
    fn public_constructor_remains_available() {
        let fetcher = StealthHttpFetcher::new(StealthProfile::Chrome131);

        assert!(fetcher.is_ok());
    }

    #[tokio::test]
    async fn proxy_clients_are_cached_by_url() {
        let fetcher =
            StealthHttpFetcher::new(StealthProfile::Chrome131).expect("stealth client builds");
        let mut request = FetchRequest::new("https://example.com", 0);
        request.proxy = Some("http://127.0.0.1:8080".to_string());

        let (first, second) =
            tokio::join!(fetcher.client_for(&request), fetcher.client_for(&request));

        assert!(first.is_ok());
        assert!(second.is_ok());
        assert_eq!(fetcher.proxy_clients.read().await.len(), 1);
    }
}
