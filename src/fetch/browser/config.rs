use std::{path::PathBuf, time::Duration};

pub(super) enum WaitStrategy {
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
    pub(super) headless: bool,
    pub(super) wait_strategy: WaitStrategy,
    pub(super) timeout: Duration,
    pub(super) viewport: (u32, u32),
    pub(super) user_agent: Option<String>,
    pub(super) executable: Option<PathBuf>,
    pub(super) proxy: Option<String>,
    /// When `true`, inject stealth JS patches and add anti-detection launch args.
    pub(super) stealth: bool,
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

    /// Browser window / viewport size (default: 1920x1080).
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
    /// supported in browser mode - use this instead.
    pub fn proxy(mut self, url: impl Into<String>) -> Self {
        self.proxy = Some(url.into());
        self
    }

    /// Enable stealth mode: inject JS fingerprint patches on every page and add
    /// anti-detection Chrome launch arguments.
    ///
    /// Patches applied:
    /// - `navigator.webdriver` -> `undefined`
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
