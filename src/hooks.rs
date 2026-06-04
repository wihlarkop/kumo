use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    error::KumoError,
    events::{CrawlEvent, EventEmitter},
};

/// Controls how the crawl engine handles errors returned by [`CrawlHook`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum HookErrorPolicy {
    /// Log hook failures and continue crawling.
    #[default]
    LogAndContinue,
    /// Abort the crawl on the first hook failure.
    AbortCrawl,
}

/// Async lifecycle hooks for extending a crawl.
///
/// Hooks are called in registration order. All methods are no-op by default,
/// so implementations can override only the lifecycle points they need.
#[async_trait]
pub trait CrawlHook: Send + Sync {
    /// Catch-all event callback. Override this when one handler should observe
    /// every event, or keep the default dispatcher to use per-event methods.
    async fn on_event(&self, event: &CrawlEvent) -> Result<(), KumoError> {
        match event {
            CrawlEvent::CrawlStarted { .. } => self.on_crawl_started(event).await,
            CrawlEvent::RequestScheduled { .. } => self.on_request_scheduled(event).await,
            CrawlEvent::RequestSkipped { .. } => self.on_request_skipped(event).await,
            CrawlEvent::RequestStarted { .. } => self.on_request_started(event).await,
            CrawlEvent::RequestCompleted { .. } => self.on_request_completed(event).await,
            CrawlEvent::RequestRetried { .. } => self.on_request_retried(event).await,
            CrawlEvent::RequestFailed { .. } => self.on_request_failed(event).await,
            CrawlEvent::TaskPanicked { .. } => self.on_task_panicked(event).await,
            CrawlEvent::ItemScraped { .. } => self.on_item_scraped(event).await,
            CrawlEvent::ItemDropped { .. } => self.on_item_dropped(event).await,
            CrawlEvent::CrawlFinished { .. } => self.on_crawl_finished(event).await,
        }
    }

    async fn on_crawl_started(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        Ok(())
    }

    async fn on_request_scheduled(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        Ok(())
    }

    async fn on_request_skipped(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        Ok(())
    }

    async fn on_request_started(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        Ok(())
    }

    async fn on_request_completed(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        Ok(())
    }

    async fn on_request_retried(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        Ok(())
    }

    async fn on_request_failed(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        Ok(())
    }

    async fn on_task_panicked(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        Ok(())
    }

    async fn on_item_scraped(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        Ok(())
    }

    async fn on_item_dropped(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        Ok(())
    }

    async fn on_crawl_finished(&self, _event: &CrawlEvent) -> Result<(), KumoError> {
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct HookDispatcher {
    hooks: Arc<Vec<Arc<dyn CrawlHook>>>,
    policy: HookErrorPolicy,
}

impl HookDispatcher {
    pub(crate) fn new(hooks: Vec<Arc<dyn CrawlHook>>, policy: HookErrorPolicy) -> Self {
        Self {
            hooks: Arc::new(hooks),
            policy,
        }
    }

    pub(crate) async fn dispatch(&self, event: &CrawlEvent) -> Result<(), KumoError> {
        for hook in self.hooks.iter() {
            if let Err(err) = hook.on_event(event).await {
                match self.policy {
                    HookErrorPolicy::LogAndContinue => {
                        tracing::warn!(
                            event = event.name(),
                            error = %err,
                            policy = "log_and_continue",
                            "crawl hook failed"
                        );
                    }
                    HookErrorPolicy::AbortCrawl => {
                        return Err(KumoError::hook(format!(
                            "crawl hook failed while handling {}: {err}",
                            event.name()
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    pub(crate) fn is_enabled(&self) -> bool {
        !self.hooks.is_empty()
    }
}

#[derive(Clone)]
pub(crate) struct CrawlObserver {
    events: Option<EventEmitter>,
    hooks: HookDispatcher,
}

impl CrawlObserver {
    pub(crate) fn new(events: Option<EventEmitter>, hooks: HookDispatcher) -> Self {
        Self { events, hooks }
    }

    pub(crate) async fn notify(&self, event: CrawlEvent) -> Result<(), KumoError> {
        if let Some(events) = &self.events {
            events.emit(event.clone());
        }

        self.hooks.dispatch(&event).await
    }

    pub(crate) async fn notify_with<F>(&self, build: F) -> Result<(), KumoError>
    where
        F: FnOnce() -> CrawlEvent,
    {
        if !self.is_enabled() {
            return Ok(());
        }

        self.notify(build()).await
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.events.is_some() || self.hooks.is_enabled()
    }
}
