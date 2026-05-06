use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::HeaderMap;

use super::{BrowserFetcher, config::WaitStrategy};
use crate::{
    error::KumoError,
    extract::{Response, response::ResponseBody},
    fetch::Fetcher,
    middleware::Request,
};

static STEALTH_PATCHES_JS: &str = include_str!("../stealth_patches.js");

#[async_trait]
impl Fetcher for BrowserFetcher {
    async fn fetch(&self, request: &Request) -> Result<Response, KumoError> {
        let start = std::time::Instant::now();

        let _permit = self
            .tab_semaphore
            .acquire()
            .await
            .map_err(|e| KumoError::Browser(e.to_string()))?;

        if request.proxy.is_some() && self.config.proxy.is_none() {
            tracing::warn!(
                "BrowserFetcher does not support per-request proxy rotation via ProxyRotator. \
                 Use BrowserConfig::proxy(url) to set a static proxy, or remove ProxyRotator \
                 when using the browser fetcher."
            );
        }

        let page = self
            .browser
            .new_page("about:blank")
            .await
            .map_err(|e| KumoError::Browser(e.to_string()))?;

        let ua = self.config.user_agent.as_deref().or_else(|| {
            request
                .headers
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
        });

        if self.config.stealth {
            page.evaluate_on_new_document(STEALTH_PATCHES_JS)
                .await
                .map_err(|e| KumoError::Browser(e.to_string()))?;

            let stealth_ua = ua.unwrap_or(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                 AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/131.0.0.0 Safari/537.36",
            );
            page.enable_stealth_mode_with_agent(stealth_ua)
                .await
                .map_err(|e| KumoError::Browser(e.to_string()))?;
        } else if let Some(ua_str) = ua {
            page.enable_stealth_mode_with_agent(ua_str)
                .await
                .map_err(|e| KumoError::Browser(e.to_string()))?;
        }

        page.goto(request.url())
            .await
            .map_err(|e| KumoError::Browser(e.to_string()))?;

        match &self.config.wait_strategy {
            WaitStrategy::Navigation => {
                page.wait_for_navigation()
                    .await
                    .map_err(|e| KumoError::Browser(e.to_string()))?;
            }
            WaitStrategy::Selector(sel) => {
                page.find_element(sel.as_str())
                    .await
                    .map_err(|e| KumoError::Browser(format!("selector '{sel}' not found: {e}")))?;
            }
            WaitStrategy::Millis(ms) => {
                tokio::time::sleep(Duration::from_millis(*ms)).await;
            }
        }

        let html = page
            .content()
            .await
            .map_err(|e| KumoError::Browser(e.to_string()))?;

        let elapsed = start.elapsed();

        page.close().await.ok();

        Ok(Response::new(
            request.url().to_string(),
            200,
            HeaderMap::new(),
            elapsed,
            ResponseBody::Text(html),
        ))
    }
}
