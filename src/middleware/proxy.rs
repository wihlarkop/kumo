use async_trait::async_trait;

use crate::error::KumoError;

use super::{Middleware, Request, RotationStrategy};

/// Middleware that assigns a proxy URL to each request, rotating through a pool.
///
/// The selected proxy URL is written to `request.proxy`; `HttpFetcher` picks it up
/// and routes the request through the specified proxy.
///
/// ## Cookie isolation
///
/// Each proxy gets its own `reqwest::Client` with an independent cookie jar.
/// This is intentional for anonymity — requests through proxy A and proxy B
/// won't share session cookies. If you need shared cookies across proxies,
/// implement a custom `Fetcher`.
///
/// Proxy URLs follow reqwest's format: `"http://user:pass@host:port"` or
/// `"socks5://host:port"`.
///
/// # Examples
/// ```rust,ignore
/// ProxyRotator::new(vec![
///     "http://user:pass@proxy1.example.com:8080",
///     "http://proxy2.example.com:8080",
/// ])
///
/// ProxyRotator::random(vec!["socks5://p1:1080", "http://p2:8080"])
/// ```
pub struct ProxyRotator {
    proxies: Vec<String>,
    strategy: RotationStrategy,
}

impl ProxyRotator {
    /// Rotate through `proxies` in round-robin order.
    pub fn new(proxies: Vec<impl Into<String>>) -> Self {
        Self {
            proxies: proxies.into_iter().map(Into::into).collect(),
            strategy: RotationStrategy::round_robin(),
        }
    }

    /// Pick a proxy pseudo-randomly on each request.
    pub fn random(proxies: Vec<impl Into<String>>) -> Self {
        Self {
            proxies: proxies.into_iter().map(Into::into).collect(),
            strategy: RotationStrategy::random(),
        }
    }

    fn pick(&self) -> Option<&str> {
        if self.proxies.is_empty() {
            return None;
        }
        Some(&self.proxies[self.strategy.pick_index(self.proxies.len())])
    }
}

#[async_trait]
impl Middleware for ProxyRotator {
    async fn before_request(&self, request: &mut Request) -> Result<(), KumoError> {
        if let Some(proxy) = self.pick() {
            request.proxy = Some(proxy.to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request() -> Request {
        Request::new("https://example.com", 0)
    }

    #[tokio::test]
    async fn round_robin_assigns_proxies_in_order() {
        let rotator = ProxyRotator::new(vec!["http://p1:8080", "http://p2:8080"]);
        let mut req = make_request();
        rotator.before_request(&mut req).await.unwrap();
        assert_eq!(req.proxy.as_deref(), Some("http://p1:8080"));
        rotator.before_request(&mut req).await.unwrap();
        assert_eq!(req.proxy.as_deref(), Some("http://p2:8080"));
        rotator.before_request(&mut req).await.unwrap();
        assert_eq!(req.proxy.as_deref(), Some("http://p1:8080"));
    }

    #[tokio::test]
    async fn random_picks_from_pool() {
        let proxies = vec!["http://p1:8080", "http://p2:8080", "http://p3:8080"];
        let rotator = ProxyRotator::random(proxies.clone());
        for _ in 0..30 {
            let mut req = make_request();
            rotator.before_request(&mut req).await.unwrap();
            let picked = req.proxy.unwrap();
            assert!(
                proxies.contains(&picked.as_str()),
                "unexpected proxy: {picked}"
            );
        }
    }

    #[tokio::test]
    async fn empty_pool_leaves_proxy_none() {
        let rotator = ProxyRotator::new(Vec::<String>::new());
        let mut req = make_request();
        rotator.before_request(&mut req).await.unwrap();
        assert!(req.proxy.is_none());
    }
}
