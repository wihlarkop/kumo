use std::{path::PathBuf, sync::Arc, time::Duration};

use async_trait::async_trait;
use chromiumoxide::browser::{Browser, BrowserConfig as CdpBrowserConfig};
use futures::StreamExt;
use reqwest::header::HeaderMap;

use crate::{
    error::KumoError,
    extract::{Response, response::ResponseBody},
    middleware::Request,
};

use super::Fetcher;

enum WaitStrategy {
    Navigation,
    Selector(String),
    Millis(u64),
}

/// Configuration for the headless/headed browser fetcher.
///
/// ```rust,ignore
/// BrowserConfig::headless()
///     .wait_for_selector(".main-content")
///     .timeout(Duration::from_secs(45))
/// ```
pub struct BrowserConfig {
    headless: bool,
    wait_strategy: WaitStrategy,
    timeout: Duration,
    viewport: (u32, u32),
    user_agent: Option<String>,
    executable: Option<PathBuf>,
}

impl BrowserConfig {
    /// Launch a headless (invisible) browser. This is the default for production scraping.
    pub fn headless() -> Self {
        Self {
            headless: true,
            wait_strategy: WaitStrategy::Navigation,
            timeout: Duration::from_secs(30),
            viewport: (1920, 1080),
            user_agent: None,
            executable: None,
        }
    }

    /// Launch a headed (visible) browser. Useful for debugging.
    pub fn headed() -> Self {
        Self {
            headless: false,
            ..Self::headless()
        }
    }

    /// After navigation, wait until the given CSS selector appears in the DOM.
    /// Use this for SPAs where content is rendered by JavaScript after load.
    pub fn wait_for_selector(mut self, selector: impl Into<String>) -> Self {
        self.wait_strategy = WaitStrategy::Selector(selector.into());
        self
    }

    /// After navigation, wait a fixed number of milliseconds before reading the page.
    pub fn wait_millis(mut self, ms: u64) -> Self {
        self.wait_strategy = WaitStrategy::Millis(ms);
        self
    }

    /// Hard timeout for the entire page load + wait cycle (default: 30s).
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    /// Browser window / viewport size (default: 1920×1080).
    pub fn viewport(mut self, width: u32, height: u32) -> Self {
        self.viewport = (width, height);
        self
    }

    /// Override the User-Agent sent by the browser.
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// Path to the Chrome/Chromium executable. Uses the system default if not set.
    pub fn executable(mut self, path: PathBuf) -> Self {
        self.executable = Some(path);
        self
    }
}

/// Fetcher that drives a real Chromium browser via the Chrome DevTools Protocol.
///
/// One browser process is launched per `BrowserFetcher`; each call to `fetch`
/// opens a new tab, navigates, waits for content, and closes the tab.
pub struct BrowserFetcher {
    browser: Arc<Browser>,
    // Kept alive to ensure the CDP event-loop task runs for the engine lifetime.
    _handler: tokio::task::JoinHandle<()>,
    config: BrowserConfig,
}

impl BrowserFetcher {
    /// Launch the browser process. Call once when the engine starts.
    pub async fn launch(config: BrowserConfig) -> Result<Self, KumoError> {
        let mut builder = CdpBrowserConfig::builder()
            .window_size(config.viewport.0, config.viewport.1)
            .launch_timeout(config.timeout);

        if !config.headless {
            builder = builder.with_head();
        }

        if let Some(ref path) = config.executable {
            builder = builder.chrome_executable(path);
        }

        let cdp_config = builder
            .build()
            .map_err(|e| KumoError::Browser(e.to_string()))?;

        let (browser, mut handler) = Browser::launch(cdp_config)
            .await
            .map_err(|e| KumoError::Browser(e.to_string()))?;

        let handler_task = tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if h.is_err() {
                    break;
                }
            }
        });

        Ok(Self {
            browser: Arc::new(browser),
            _handler: handler_task,
            config,
        })
    }
}

#[async_trait]
impl Fetcher for BrowserFetcher {
    async fn fetch(&self, request: &Request) -> Result<Response, KumoError> {
        let start = std::time::Instant::now();

        if request.proxy.is_some() {
            tracing::warn!(
                "BrowserFetcher does not support per-request proxy rotation. \
                 Set a static proxy via BrowserConfig::proxy() or remove ProxyRotator \
                 when using the browser fetcher."
            );
        }

        // Open a blank tab so we can set headers before navigation.
        let page = self
            .browser
            .new_page("about:blank")
            .await
            .map_err(|e| KumoError::Browser(e.to_string()))?;

        // Apply User-Agent from BrowserConfig, or from middleware DefaultHeaders if present.
        let ua = self.config.user_agent.as_deref().or_else(|| {
            request
                .headers
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
        });
        if let Some(ua_str) = ua {
            page.enable_stealth_mode_with_agent(ua_str)
                .await
                .map_err(|e| KumoError::Browser(e.to_string()))?;
        }

        // Navigate to the target URL.
        page.goto(&request.url)
            .await
            .map_err(|e| KumoError::Browser(e.to_string()))?;

        // Wait for content to be ready based on configured strategy.
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
            request.url.clone(),
            200,
            HeaderMap::new(),
            elapsed,
            ResponseBody::Text(html),
        ))
    }
}
