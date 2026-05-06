use std::{sync::Arc, time::Duration};

use crate::{
    engine::USER_AGENT,
    error::KumoError,
    extract::Response,
    fetch::{Fetcher, http::HttpFetcher},
    robots::RobotsCache,
};

#[cfg(feature = "browser")]
use crate::fetch::{BrowserConfig, BrowserFetcher};

pub(super) fn build_http_client(
    concurrency: usize,
    timeout: Option<Duration>,
    customize: Option<Box<dyn FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder + Send>>,
) -> Result<reqwest::Client, KumoError> {
    let mut builder = reqwest::Client::builder()
        .cookie_store(true)
        .user_agent(USER_AGENT)
        .pool_max_idle_per_host(concurrency)
        .tcp_keepalive(Duration::from_secs(60));
    if let Some(t) = timeout {
        builder = builder.timeout(t);
    }
    if let Some(f) = customize {
        builder = f(builder);
    }
    builder.build().map_err(KumoError::Fetch)
}

pub(super) fn build_robots_cache(respect: bool, ttl: Duration) -> Option<Arc<RobotsCache>> {
    if respect {
        Some(Arc::new(RobotsCache::with_ttl(USER_AGENT, ttl)))
    } else {
        None
    }
}

pub(super) fn wrap_with_cache(
    fetcher: Arc<dyn Fetcher>,
    cache_dir: Option<std::path::PathBuf>,
    cache_ttl: Option<Duration>,
) -> Result<Arc<dyn Fetcher>, KumoError> {
    if let Some(dir) = cache_dir {
        let mut cf = crate::fetch::CachingFetcher::new(ArcFetcher(fetcher), dir)?;
        if let Some(ttl) = cache_ttl {
            cf = cf.ttl(ttl);
        }
        Ok(Arc::new(cf))
    } else {
        Ok(fetcher)
    }
}

#[allow(dead_code)]
pub(super) struct FetcherArgs {
    pub(super) fetcher_override: Option<Arc<dyn Fetcher>>,
    pub(super) client: reqwest::Client,
    pub(super) concurrency: usize,
    #[cfg(feature = "stealth")]
    pub(super) stealth_profile: Option<crate::fetch::StealthProfile>,
    #[cfg(feature = "browser")]
    pub(super) browser: Option<BrowserConfig>,
}

pub(super) async fn build_raw_fetcher(args: FetcherArgs) -> Result<Arc<dyn Fetcher>, KumoError> {
    if let Some(f) = args.fetcher_override {
        return Ok(f);
    }

    #[cfg(feature = "stealth")]
    if let Some(profile) = args.stealth_profile {
        return Ok(Arc::new(crate::fetch::StealthHttpFetcher::new(profile)?));
    }

    #[cfg(feature = "browser")]
    if let Some(cfg) = args.browser {
        return Ok(Arc::new(
            BrowserFetcher::launch(cfg, args.concurrency).await?,
        ));
    }

    Ok(Arc::new(HttpFetcher::new(args.client, USER_AGENT)))
}

struct ArcFetcher(Arc<dyn Fetcher>);

#[async_trait::async_trait]
impl Fetcher for ArcFetcher {
    async fn fetch(&self, request: &crate::middleware::Request) -> Result<Response, KumoError> {
        self.0.fetch(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_http_client_accepts_concurrency() {
        let _c1 = build_http_client(16, None, None).unwrap();
        let _c2 = build_http_client(1, None, None).unwrap();
    }
}
