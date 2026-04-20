use std::{path::PathBuf, sync::Arc, time::Duration};

use tokio::sync::Semaphore;

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

static STEALTH_PATCHES_JS: &str = include_str!("stealth_patches.js");

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
    proxy: Option<String>,
    /// When `true`, inject stealth JS patches and add anti-detection launch args.
    stealth: bool,
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
            proxy: None,
            stealth: false,
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

    /// Route all browser traffic through a static HTTP/HTTPS proxy.
    ///
    /// Pass the proxy URL in the form `http://host:port` or `socks5://host:port`.
    /// Note: per-request proxy rotation via `ProxyRotator` middleware is not
    /// supported in browser mode — use this instead.
    pub fn proxy(mut self, url: impl Into<String>) -> Self {
        self.proxy = Some(url.into());
        self
    }

    /// Enable stealth mode: inject JS fingerprint patches on every page and add
    /// anti-detection Chrome launch arguments.
    ///
    /// Patches applied:
    /// - `navigator.webdriver` → `undefined`
    /// - Fake non-empty `navigator.plugins` array
    /// - `window.chrome` stub
    /// - `navigator.permissions.query` patch (returns `"prompt"` for notifications)
    /// - Canvas fingerprint noise
    /// - WebGL vendor/renderer spoof
    pub fn stealth(mut self) -> Self {
        self.stealth = true;
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
    // Caps concurrent open tabs to the engine's concurrency setting.
    tab_semaphore: Arc<Semaphore>,
}

impl BrowserFetcher {
    /// Launch the browser process. `concurrency` caps how many tabs can be open simultaneously.
    pub async fn launch(config: BrowserConfig, concurrency: usize) -> Result<Self, KumoError> {
        let mut builder = CdpBrowserConfig::builder()
            .window_size(config.viewport.0, config.viewport.1)
            .launch_timeout(config.timeout);

        if !config.headless {
            builder = builder.with_head();
        }

        if let Some(ref path) = config.executable {
            builder = builder.chrome_executable(path);
        }

        if let Some(ref proxy_url) = config.proxy {
            builder = builder.arg(format!("--proxy-server={proxy_url}"));
        }

        if config.stealth {
            builder = builder
                .arg("--disable-blink-features=AutomationControlled")
                .arg("--disable-features=IsolateOrigins,site-per-process")
                .arg("--no-default-browser-check")
                .arg("--disable-infobars");
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
            tab_semaphore: Arc::new(Semaphore::new(concurrency.max(1))),
        })
    }
}

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

        if self.config.stealth {
            // Inject JS fingerprint patches before any page scripts run.
            page.evaluate_on_new_document(STEALTH_PATCHES_JS)
                .await
                .map_err(|e| KumoError::Browser(e.to_string()))?;

            // chromiumoxide built-in stealth shim — always apply in stealth mode.
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

        // Navigate to the target URL.
        page.goto(request.url())
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
            request.url().to_string(),
            200,
            HeaderMap::new(),
            elapsed,
            ResponseBody::Text(html),
        ))
    }
}
