use std::sync::Arc;

use chromiumoxide::browser::{Browser, BrowserConfig as CdpBrowserConfig};
use futures::StreamExt;
use tokio::sync::Semaphore;

use super::{BrowserConfig, BrowserFetcher};
use crate::error::KumoError;

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
