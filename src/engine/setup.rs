use std::{sync::Arc, time::Duration};

use crate::{
    engine::USER_AGENT,
    error::KumoError,
    extract::Response,
    fetch::{FetchStatsSnapshot, Fetcher, client_policy::HttpClientPolicy, http::HttpFetcher},
    robots::RobotsCache,
};

#[cfg(feature = "browser")]
use crate::fetch::{BrowserConfig, BrowserFallbackConfig, BrowserFallbackFetcher, BrowserFetcher};

pub(super) fn build_http_client(
    policy: &HttpClientPolicy,
    customize: Option<Box<dyn FnOnce(reqwest::ClientBuilder) -> reqwest::ClientBuilder + Send>>,
) -> Result<reqwest::Client, KumoError> {
    let mut builder = policy.reqwest_builder();
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
    pub(super) client_policy: HttpClientPolicy,
    pub(super) concurrency: usize,
    #[cfg(feature = "stealth")]
    pub(super) stealth_profile: Option<crate::fetch::StealthProfile>,
    #[cfg(feature = "browser")]
    pub(super) browser: Option<BrowserConfig>,
    #[cfg(feature = "browser")]
    pub(super) browser_fallback: Option<BrowserFallbackConfig>,
}

pub(super) async fn build_raw_fetcher(args: FetcherArgs) -> Result<Arc<dyn Fetcher>, KumoError> {
    if let Some(f) = args.fetcher_override {
        return Ok(f);
    }

    #[cfg(feature = "browser")]
    if let Some(cfg) = args.browser {
        return Ok(Arc::new(
            BrowserFetcher::launch(cfg, args.concurrency).await?,
        ));
    }

    #[cfg(feature = "stealth")]
    let http_fetcher: Arc<dyn Fetcher> = if let Some(profile) = args.stealth_profile {
        Arc::new(crate::fetch::StealthHttpFetcher::with_policy(
            profile,
            args.client_policy,
        )?)
    } else {
        Arc::new(HttpFetcher::with_policy(args.client, args.client_policy))
    };

    #[cfg(not(feature = "stealth"))]
    let http_fetcher: Arc<dyn Fetcher> =
        Arc::new(HttpFetcher::with_policy(args.client, args.client_policy));

    #[cfg(feature = "browser")]
    if let Some(cfg) = args.browser_fallback {
        let (browser_cfg, should_fallback) = cfg.split();
        let browser = Arc::new(BrowserFetcher::launch(browser_cfg, args.concurrency).await?);
        return Ok(Arc::new(BrowserFallbackFetcher::from_parts(
            http_fetcher,
            browser,
            should_fallback,
        )));
    }

    Ok(http_fetcher)
}

struct ArcFetcher(Arc<dyn Fetcher>);

#[async_trait::async_trait]
impl Fetcher for ArcFetcher {
    async fn fetch(
        &self,
        request: &crate::middleware::FetchRequest,
    ) -> Result<Response, KumoError> {
        self.0.fetch(request).await
    }

    fn stats(&self) -> FetchStatsSnapshot {
        self.0.stats()
    }
}
