use std::sync::Arc;

use chromiumoxide::browser::Browser;
use tokio::sync::Semaphore;

use super::BrowserConfig;

/// Fetcher that drives a real Chromium browser via the Chrome DevTools Protocol.
///
/// One browser process is launched per `BrowserFetcher`; each call to `fetch`
/// opens a new tab, navigates, waits for content, and closes the tab.
pub struct BrowserFetcher {
    pub(super) browser: Arc<Browser>,
    // Kept alive to ensure the CDP event-loop task runs for the engine lifetime.
    pub(super) _handler: tokio::task::JoinHandle<()>,
    pub(super) config: BrowserConfig,
    // Caps concurrent open tabs to the engine's concurrency setting.
    pub(super) tab_semaphore: Arc<Semaphore>,
}
