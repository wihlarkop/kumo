use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::{
    engine::erased::ErasedSpider,
    error::KumoError,
    fetch::Fetcher,
    middleware::{FetchRequest, Middleware},
    pipeline::Pipeline,
    request::{CrawlRequest, FrontierRequest},
    store::ItemStore,
};

pub(super) struct TaskContext {
    pub(super) spider: Arc<dyn ErasedSpider>,
    pub(super) store: Arc<dyn ItemStore>,
    pub(super) middleware: Arc<Vec<Arc<dyn Middleware>>>,
    pub(super) pipelines: Arc<Vec<Arc<dyn Pipeline>>>,
    pub(super) fetcher: Arc<dyn Fetcher>,
    pub(super) stream_cancelled: Option<Arc<AtomicBool>>,
}

async fn process_request(
    queued: &FrontierRequest,
    ctx: &TaskContext,
) -> Result<(u64, u64, Vec<(CrawlRequest, usize)>), KumoError> {
    let url = queued.request.url();
    let depth = queued.depth;

    let mut request = FetchRequest::from_crawl_request(&queued.request, depth);
    for mw in ctx.middleware.iter() {
        mw.before_request(&mut request).await?;
    }

    let mut response = ctx.fetcher.fetch(&request).await?;
    let bytes_downloaded = response.bytes().len() as u64;

    for mw in ctx.middleware.iter() {
        mw.after_response(&mut response).await?;
    }

    let output = ctx.spider.parse_erased(&response).await?;

    let mut item_count = 0u64;
    'items: for item in output.items {
        if is_cancelled(&ctx.stream_cancelled) {
            break 'items;
        }

        let mut current = item;
        for pipeline in ctx.pipelines.iter() {
            match pipeline.process(current).await {
                Ok(Some(v)) => current = v,
                Ok(None) => {
                    tracing::debug!(
                        target: "kumo::item",
                        spider = ctx.spider.name(),
                        url,
                        "item.drop"
                    );
                    continue 'items;
                }
                Err(e) => {
                    tracing::warn!(
                        target: "kumo::item",
                        error = %e,
                        "item.drop.pipeline_error"
                    );
                    continue 'items;
                }
            }
        }
        ctx.store.store(&current).await?;
        if is_cancelled(&ctx.stream_cancelled) {
            return Ok((item_count, bytes_downloaded, Vec::new()));
        }
        item_count += 1;
    }

    tracing::debug!(
        target: "kumo::request",
        spider = ctx.spider.name(),
        url,
        status = response.status(),
        bytes = bytes_downloaded,
        depth,
        items = item_count,
        "request.ok"
    );

    let follows = output.follow.into_iter().map(|r| (r, depth + 1)).collect();

    Ok((item_count, bytes_downloaded, follows))
}

pub(super) async fn process_request_once(
    queued: FrontierRequest,
    ctx: TaskContext,
) -> Result<(u64, u64, Vec<(CrawlRequest, usize)>), KumoError> {
    process_request(&queued, &ctx).await
}

pub(super) fn should_enqueue(
    request: &CrawlRequest,
    depth: usize,
    spider: &dyn ErasedSpider,
) -> bool {
    if spider.max_depth().is_some_and(|max| depth > max) {
        return false;
    }
    let allowed = spider.allowed_domains();
    if allowed.is_empty() {
        return true;
    }
    url::Url::parse(request.url())
        .ok()
        .and_then(|u| u.host_str().map(String::from))
        .map(|host| allowed.iter().any(|d| host.ends_with(*d)))
        .unwrap_or(false)
}

pub(super) fn is_cancelled(cancelled: &Option<Arc<AtomicBool>>) -> bool {
    cancelled
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
}
