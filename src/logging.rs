//! Internal structured logging contract.

pub(crate) mod target {
    pub(crate) const CRAWL: &str = "kumo::crawl";
    pub(crate) const REQUEST: &str = "kumo::request";
    pub(crate) const ITEM: &str = "kumo::item";
    pub(crate) const CACHE: &str = "kumo::cache";
}

pub(crate) mod event {
    pub(crate) const CRAWL_ABORT: &str = "crawl.abort";
    pub(crate) const CRAWL_COMPLETE: &str = "crawl.complete";
    pub(crate) const CRAWL_INTERRUPTED: &str = "crawl.interrupted";
    pub(crate) const CRAWL_METRICS: &str = "crawl.metrics";
    pub(crate) const CRAWL_START: &str = "crawl.start";
    pub(crate) const CRAWL_STREAM_ERROR: &str = "crawl.stream_error";
    pub(crate) const CRAWL_TASK_PANIC: &str = "crawl.task_panic";
    pub(crate) const CACHE_BYPASS: &str = "cache.bypass";
    pub(crate) const CACHE_HIT: &str = "cache.hit";
    pub(crate) const CACHE_MISS: &str = "cache.miss";
    pub(crate) const CACHE_STORE_SKIP: &str = "cache.store_skip";
    pub(crate) const ITEM_DROP: &str = "item.drop";
    pub(crate) const ITEM_DROP_PIPELINE_ERROR: &str = "item.drop.pipeline_error";
    pub(crate) const STORE_BUFFER_ERROR: &str = "store.buffer_error";
    pub(crate) const REQUEST_FETCH: &str = "request.fetch";
    pub(crate) const REQUEST_OK: &str = "request.ok";
    pub(crate) const REQUEST_AUTOTHROTTLE: &str = "request.autothrottle";
    #[cfg(feature = "browser")]
    pub(crate) const REQUEST_PROXY_IGNORED: &str = "request.proxy_ignored";
    pub(crate) const REQUEST_RATE_LIMIT: &str = "request.rate_limit";
    pub(crate) const REQUEST_RETRY: &str = "request.retry";
    pub(crate) const REQUEST_RETRY_EXHAUSTED: &str = "request.retry_exhausted";
    pub(crate) const REQUEST_ROBOTS_BLOCKED: &str = "request.robots_blocked";
    pub(crate) const REQUEST_SKIP: &str = "request.skip";
    pub(crate) const SPIDER_CLOSE_FAILED: &str = "spider.close_failed";
}

#[cfg(test)]
mod tests {
    use super::{event, target};

    #[test]
    fn targets_are_stable() {
        assert_eq!(target::CRAWL, "kumo::crawl");
        assert_eq!(target::REQUEST, "kumo::request");
        assert_eq!(target::ITEM, "kumo::item");
        assert_eq!(target::CACHE, "kumo::cache");
    }

    #[test]
    fn event_names_are_dot_scoped() {
        let events = [
            event::CRAWL_ABORT,
            event::CRAWL_COMPLETE,
            event::CRAWL_INTERRUPTED,
            event::CRAWL_METRICS,
            event::CRAWL_START,
            event::CRAWL_STREAM_ERROR,
            event::CRAWL_TASK_PANIC,
            event::CACHE_BYPASS,
            event::CACHE_HIT,
            event::CACHE_MISS,
            event::CACHE_STORE_SKIP,
            event::ITEM_DROP,
            event::ITEM_DROP_PIPELINE_ERROR,
            event::STORE_BUFFER_ERROR,
            event::REQUEST_FETCH,
            event::REQUEST_OK,
            event::REQUEST_AUTOTHROTTLE,
            #[cfg(feature = "browser")]
            event::REQUEST_PROXY_IGNORED,
            event::REQUEST_RATE_LIMIT,
            event::REQUEST_RETRY,
            event::REQUEST_RETRY_EXHAUSTED,
            event::REQUEST_ROBOTS_BLOCKED,
            event::REQUEST_SKIP,
            event::SPIDER_CLOSE_FAILED,
        ];

        for event in events {
            assert!(event.contains('.'), "{event} should be namespace-scoped");
            assert_eq!(event, event.trim());
        }
    }
}
