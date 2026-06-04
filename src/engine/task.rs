use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::{
    engine::erased::ErasedSpider,
    error::KumoError,
    events::{CrawlEvent, ItemDropReason, RequestSkipReason},
    fetch::Fetcher,
    hooks::CrawlObserver,
    logging::{event, target},
    middleware::{FetchRequest, Middleware},
    pipeline::Pipeline,
    request::{CrawlRequest, FrontierRequest},
    store::ItemStore,
};

pub(super) struct TaskContext {
    pub(super) spider: Arc<dyn ErasedSpider>,
    pub(super) spider_index: Option<usize>,
    pub(super) store: Arc<dyn ItemStore>,
    pub(super) middleware: Arc<Vec<Arc<dyn Middleware>>>,
    pub(super) pipelines: Arc<Vec<Arc<dyn Pipeline>>>,
    pub(super) fetcher: Arc<dyn Fetcher>,
    pub(super) observer: CrawlObserver,
    pub(super) stream_cancelled: Option<Arc<AtomicBool>>,
}

pub(super) struct RequestTaskOutput {
    pub(super) item_count: u64,
    pub(super) bytes_downloaded: u64,
    pub(super) follows: Vec<(CrawlRequest, usize)>,
}

async fn process_request(
    queued: &FrontierRequest,
    ctx: &TaskContext,
) -> Result<RequestTaskOutput, KumoError> {
    let url = queued.request.url();
    let depth = queued.depth;
    let domain = crate::stats::domain_key(url);
    let started_at = std::time::Instant::now();

    ctx.observer
        .notify_with(|| CrawlEvent::RequestStarted {
            spider: ctx.spider.name().to_string(),
            spider_index: ctx.spider_index,
            url: url.to_string(),
            domain: domain.clone(),
            depth,
            attempt: queued.retry_count,
        })
        .await?;

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
                    ctx.observer
                        .notify_with(|| CrawlEvent::ItemDropped {
                            spider: ctx.spider.name().to_string(),
                            spider_index: ctx.spider_index,
                            url: url.to_string(),
                            depth,
                            reason: ItemDropReason::PipelineFiltered,
                            error_kind: None,
                        })
                        .await?;
                    tracing::debug!(
                        target: target::ITEM,
                        event = event::ITEM_DROP,
                        spider = ctx.spider.name(),
                        url,
                        depth,
                        "item.drop"
                    );
                    continue 'items;
                }
                Err(e) => {
                    ctx.observer
                        .notify_with(|| CrawlEvent::ItemDropped {
                            spider: ctx.spider.name().to_string(),
                            spider_index: ctx.spider_index,
                            url: url.to_string(),
                            depth,
                            reason: ItemDropReason::PipelineError,
                            error_kind: Some(e.kind()),
                        })
                        .await?;
                    tracing::warn!(
                        target: target::ITEM,
                        event = event::ITEM_DROP_PIPELINE_ERROR,
                        spider = ctx.spider.name(),
                        url,
                        depth,
                        error = %e,
                        error_kind = e.kind().as_str(),
                        "item.drop.pipeline_error"
                    );
                    continue 'items;
                }
            }
        }
        ctx.store.store(&current).await?;
        ctx.observer
            .notify_with(|| CrawlEvent::ItemScraped {
                spider: ctx.spider.name().to_string(),
                spider_index: ctx.spider_index,
                url: url.to_string(),
                depth,
            })
            .await?;
        if is_cancelled(&ctx.stream_cancelled) {
            return Ok(RequestTaskOutput {
                item_count,
                bytes_downloaded,
                follows: Vec::new(),
            });
        }
        item_count += 1;
    }

    tracing::debug!(
        target: target::REQUEST,
        event = event::REQUEST_OK,
        spider = ctx.spider.name(),
        url,
        status = response.status(),
        bytes = bytes_downloaded,
        depth,
        items = item_count,
        "request.ok"
    );

    let follows = output.follow.into_iter().map(|r| (r, depth + 1)).collect();

    ctx.observer
        .notify_with(|| CrawlEvent::RequestCompleted {
            spider: ctx.spider.name().to_string(),
            spider_index: ctx.spider_index,
            url: url.to_string(),
            domain,
            depth,
            attempt: queued.retry_count,
            status: response.status(),
            bytes: bytes_downloaded,
            items: item_count,
            elapsed: started_at.elapsed(),
        })
        .await?;

    Ok(RequestTaskOutput {
        item_count,
        bytes_downloaded,
        follows,
    })
}

pub(super) async fn process_request_once(
    queued: FrontierRequest,
    ctx: TaskContext,
) -> Result<RequestTaskOutput, KumoError> {
    process_request(&queued, &ctx).await
}

pub(super) fn skip_reason(
    request: &CrawlRequest,
    depth: usize,
    spider: &dyn ErasedSpider,
) -> Option<RequestSkipReason> {
    if spider.max_depth().is_some_and(|max| depth > max) {
        return Some(RequestSkipReason::DepthLimit);
    }
    let allowed = spider.allowed_domains();
    if allowed.is_empty() {
        return None;
    }
    let allowed = url::Url::parse(request.url())
        .ok()
        .and_then(|u| u.host_str().map(String::from))
        .map(|host| allowed.iter().any(|d| host.ends_with(*d)))
        .unwrap_or(false);
    if allowed {
        None
    } else {
        Some(RequestSkipReason::DomainDenied)
    }
}

pub(super) fn should_enqueue(
    request: &CrawlRequest,
    depth: usize,
    spider: &dyn ErasedSpider,
) -> bool {
    skip_reason(request, depth, spider).is_none()
}

pub(super) fn is_cancelled(cancelled: &Option<Arc<AtomicBool>>) -> bool {
    cancelled
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
}
