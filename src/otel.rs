//! OpenTelemetry OTLP integration for kumo.
//!
//! Requires the `otel` feature flag.
//!
//! Wires the existing `tracing` instrumentation into an OTLP pipeline so every
//! span and event emitted by kumo (requests, retries, item drops, etc.) is
//! exported to your collector automatically — no changes to spider code needed.
//!
//! # Example
//! ```rust,ignore
//! #[tokio::main]
//! async fn main() -> Result<(), kumo::error::KumoError> {
//!     kumo::otel::init("my-crawler", "http://localhost:4317").await?;
//!
//!     CrawlEngine::builder()
//!         .run(MySpider)
//!         .await?;
//!
//!     kumo::otel::shutdown();
//!     Ok(())
//! }
//! ```

use std::{
    sync::{Mutex, OnceLock},
    time::Duration,
};

use crate::error::KumoError;

static TRACER_PROVIDER: OnceLock<Mutex<Option<opentelemetry_sdk::trace::SdkTracerProvider>>> =
    OnceLock::new();
static METER_PROVIDER: OnceLock<Mutex<Option<opentelemetry_sdk::metrics::SdkMeterProvider>>> =
    OnceLock::new();
static PRODUCTION_METRICS: OnceLock<Mutex<Option<ProductionMetrics>>> = OnceLock::new();

#[derive(Clone)]
struct ProductionMetrics {
    requests_scheduled: opentelemetry::metrics::Counter<u64>,
    requests_deduped: opentelemetry::metrics::Counter<u64>,
    requests_skipped: opentelemetry::metrics::Counter<u64>,
    pages_crawled: opentelemetry::metrics::Counter<u64>,
    items_scraped: opentelemetry::metrics::Counter<u64>,
    items_dropped: opentelemetry::metrics::Counter<u64>,
    errors: opentelemetry::metrics::Counter<u64>,
    retries: opentelemetry::metrics::Counter<u64>,
    retries_exhausted: opentelemetry::metrics::Counter<u64>,
    robots_blocked: opentelemetry::metrics::Counter<u64>,
    fetch_latency: opentelemetry::metrics::Histogram<f64>,
    request_duration: opentelemetry::metrics::Histogram<f64>,
    store_queued: opentelemetry::metrics::Counter<u64>,
    store_written: opentelemetry::metrics::Counter<u64>,
    store_failed_writes: opentelemetry::metrics::Counter<u64>,
    store_failed_batches: opentelemetry::metrics::Counter<u64>,
    store_queue_full_waits: opentelemetry::metrics::Counter<u64>,
    store_queue_wait: opentelemetry::metrics::Histogram<f64>,
    store_write: opentelemetry::metrics::Histogram<f64>,
}

impl ProductionMetrics {
    fn new(provider: &opentelemetry_sdk::metrics::SdkMeterProvider) -> Self {
        use opentelemetry::metrics::MeterProvider as _;

        let meter = provider.meter("kumo");
        Self {
            requests_scheduled: meter
                .u64_counter("kumo.requests.scheduled")
                .with_description("Requests accepted by the crawl scheduler")
                .with_unit("{request}")
                .build(),
            requests_deduped: meter
                .u64_counter("kumo.requests.deduped")
                .with_description("Requests skipped because their fingerprint was already seen")
                .with_unit("{request}")
                .build(),
            requests_skipped: meter
                .u64_counter("kumo.requests.skipped")
                .with_description("Requests skipped before fetching")
                .with_unit("{request}")
                .build(),
            pages_crawled: meter
                .u64_counter("kumo.pages.crawled")
                .with_description("Successful pages crawled")
                .with_unit("{page}")
                .build(),
            items_scraped: meter
                .u64_counter("kumo.items.scraped")
                .with_description("Items scraped and accepted by the item store")
                .with_unit("{item}")
                .build(),
            items_dropped: meter
                .u64_counter("kumo.items.dropped")
                .with_description("Items dropped by pipelines")
                .with_unit("{item}")
                .build(),
            errors: meter
                .u64_counter("kumo.errors")
                .with_description("Permanent crawl errors")
                .with_unit("{error}")
                .build(),
            retries: meter
                .u64_counter("kumo.retries")
                .with_description("Retry attempts scheduled")
                .with_unit("{retry}")
                .build(),
            retries_exhausted: meter
                .u64_counter("kumo.retries.exhausted")
                .with_description("Requests that failed after retry capacity was exhausted")
                .with_unit("{request}")
                .build(),
            robots_blocked: meter
                .u64_counter("kumo.robots.blocked")
                .with_description("Requests blocked by robots.txt")
                .with_unit("{request}")
                .build(),
            fetch_latency: meter
                .f64_histogram("kumo.fetch.latency")
                .with_description("Successful request fetch latency")
                .with_unit("s")
                .build(),
            request_duration: meter
                .f64_histogram("kumo.request.duration")
                .with_description("Successful request processing duration")
                .with_unit("s")
                .build(),
            store_queued: meter
                .u64_counter("kumo.store.queued")
                .with_description("Items accepted into the bounded store queue")
                .with_unit("{item}")
                .build(),
            store_written: meter
                .u64_counter("kumo.store.written")
                .with_description("Items written by the store writer")
                .with_unit("{item}")
                .build(),
            store_failed_writes: meter
                .u64_counter("kumo.store.failed_writes")
                .with_description("Items in store batches that returned an error")
                .with_unit("{item}")
                .build(),
            store_failed_batches: meter
                .u64_counter("kumo.store.failed_batches")
                .with_description("Store batch writes that returned an error")
                .with_unit("{batch}")
                .build(),
            store_queue_full_waits: meter
                .u64_counter("kumo.store.queue_full_waits")
                .with_description("Item sends that observed a full store queue")
                .with_unit("{wait}")
                .build(),
            store_queue_wait: meter
                .f64_histogram("kumo.store.queue_wait")
                .with_description("Store queue wait time")
                .with_unit("s")
                .build(),
            store_write: meter
                .f64_histogram("kumo.store.write")
                .with_description("Store batch write attempt time")
                .with_unit("s")
                .build(),
        }
    }
}

/// Initialise the OpenTelemetry OTLP pipeline and register it with the
/// global `tracing` subscriber.
///
/// - `service_name` — identifies this process in your APM dashboard.
/// - `otlp_endpoint` — gRPC endpoint of your collector,
///   e.g. `"http://localhost:4317"` for a local Jaeger or OTel Collector.
///
/// Call **once** at the start of `main`, before creating any `CrawlEngine`.
/// After this call every `tracing` span/event emitted by kumo is exported
/// to the collector automatically. Stdout logging via the `fmt` layer
/// continues as before; level filtering uses `RUST_LOG`.
///
/// Returns an error if the exporter or subscriber cannot be initialised
/// (e.g. a subscriber is already registered in this process).
pub async fn init(
    service_name: impl Into<String>,
    otlp_endpoint: impl Into<String>,
) -> Result<(), KumoError> {
    use opentelemetry::KeyValue;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::{Resource, metrics::SdkMeterProvider, trace::SdkTracerProvider};
    use tracing_subscriber::prelude::*;

    let service_name = service_name.into();
    let endpoint = otlp_endpoint.into();

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&endpoint)
        .build()
        .map_err(|e| KumoError::store_msg(format!("otel exporter: {e}")))?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_attribute(KeyValue::new("service.name", service_name.clone()))
                .build(),
        )
        .build();

    opentelemetry::global::set_tracer_provider(provider.clone());
    let provider_slot = TRACER_PROVIDER.get_or_init(|| Mutex::new(None));
    if let Ok(mut current) = provider_slot.lock() {
        *current = Some(provider.clone());
    }

    let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(&endpoint)
        .build()
        .map_err(|e| KumoError::store_msg(format!("otel metric exporter: {e}")))?;
    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(
            Resource::builder()
                .with_attribute(KeyValue::new("service.name", service_name.clone()))
                .build(),
        )
        .build();
    opentelemetry::global::set_meter_provider(meter_provider.clone());
    let metrics = ProductionMetrics::new(&meter_provider);
    let provider_slot = METER_PROVIDER.get_or_init(|| Mutex::new(None));
    if let Ok(mut current) = provider_slot.lock() {
        *current = Some(meter_provider);
    }
    let metrics_slot = PRODUCTION_METRICS.get_or_init(|| Mutex::new(None));
    if let Ok(mut current) = metrics_slot.lock() {
        *current = Some(metrics);
    }

    let otel_layer = tracing_opentelemetry::layer().with_tracer(provider.tracer("kumo"));

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .with(otel_layer)
        .try_init()
        .map_err(|e| KumoError::store_msg(format!("tracing subscriber: {e}")))?;

    tracing::info!(
        service = %service_name,
        endpoint = %endpoint,
        "otel initialized"
    );
    Ok(())
}

/// Flush all pending spans and shut down the global tracer provider.
///
/// Call at the end of `main` to ensure all in-flight telemetry is exported
/// before the process exits. Safe to call even if [`init`] was not called.
pub fn shutdown() {
    if let Some(provider_slot) = TRACER_PROVIDER.get()
        && let Ok(mut provider) = provider_slot.lock()
        && let Some(provider) = provider.take()
    {
        let _ = provider.shutdown();
    }
    if let Some(metrics_slot) = PRODUCTION_METRICS.get()
        && let Ok(mut metrics) = metrics_slot.lock()
    {
        *metrics = None;
    }
    if let Some(provider_slot) = METER_PROVIDER.get()
        && let Ok(mut provider) = provider_slot.lock()
        && let Some(provider) = provider.take()
    {
        let _ = provider.shutdown();
    }
}

pub(crate) fn record_fetch_latency(spider: &str, spider_index: Option<usize>, latency: Duration) {
    if let Some(metrics) = production_metrics() {
        metrics.fetch_latency.record(
            latency.as_secs_f64(),
            &crawl_attributes(spider, spider_index),
        );
    }
}

pub(crate) fn record_request_scheduled(spider: &str, spider_index: Option<usize>, domain: &str) {
    if let Some(metrics) = production_metrics() {
        metrics
            .requests_scheduled
            .add(1, &domain_attributes(spider, spider_index, domain));
    }
}

pub(crate) fn record_request_deduped(spider: &str, spider_index: Option<usize>, domain: &str) {
    if let Some(metrics) = production_metrics() {
        metrics
            .requests_deduped
            .add(1, &domain_attributes(spider, spider_index, domain));
    }
}

pub(crate) fn record_request_skipped(
    spider: &str,
    spider_index: Option<usize>,
    domain: &str,
    reason: crate::events::RequestSkipReason,
) {
    if let Some(metrics) = production_metrics() {
        metrics.requests_skipped.add(
            1,
            &skip_attributes(spider, spider_index, domain, reason.as_str()),
        );
    }
}

pub(crate) fn record_robots_blocked(spider: &str, spider_index: Option<usize>, domain: &str) {
    if let Some(metrics) = production_metrics() {
        let attrs = skip_attributes(
            spider,
            spider_index,
            domain,
            crate::events::RequestSkipReason::RobotsTxt.as_str(),
        );
        metrics.robots_blocked.add(1, &attrs);
        metrics.requests_skipped.add(1, &attrs);
    }
}

pub(crate) fn record_request_completed(
    spider: &str,
    spider_index: Option<usize>,
    domain: &str,
    elapsed: Duration,
) {
    if let Some(metrics) = production_metrics() {
        let attrs = domain_attributes(spider, spider_index, domain);
        metrics.pages_crawled.add(1, &attrs);
        metrics
            .request_duration
            .record(elapsed.as_secs_f64(), &attrs);
    }
}

pub(crate) fn record_item_scraped(spider: &str, spider_index: Option<usize>, domain: &str) {
    if let Some(metrics) = production_metrics() {
        metrics
            .items_scraped
            .add(1, &domain_attributes(spider, spider_index, domain));
    }
}

pub(crate) fn record_item_dropped(
    spider: &str,
    spider_index: Option<usize>,
    reason: crate::events::ItemDropReason,
    error_kind: Option<crate::error::KumoErrorKind>,
) {
    if let Some(metrics) = production_metrics() {
        metrics.items_dropped.add(
            1,
            &item_drop_attributes(spider, spider_index, reason.as_str(), error_kind),
        );
    }
}

pub(crate) fn record_retry_scheduled(spider: &str, spider_index: Option<usize>, domain: &str) {
    if let Some(metrics) = production_metrics() {
        metrics
            .retries
            .add(1, &domain_attributes(spider, spider_index, domain));
    }
}

pub(crate) fn record_retry_exhausted(spider: &str, spider_index: Option<usize>, domain: &str) {
    if let Some(metrics) = production_metrics() {
        metrics
            .retries_exhausted
            .add(1, &domain_attributes(spider, spider_index, domain));
    }
}

pub(crate) fn record_request_failed(
    spider: &str,
    spider_index: Option<usize>,
    domain: &str,
    error_kind: impl Into<String>,
) {
    if let Some(metrics) = production_metrics() {
        metrics.errors.add(
            1,
            &error_attributes(spider, spider_index, domain, error_kind.into()),
        );
    }
}

pub(crate) fn record_crawl_report(
    spider: &str,
    spider_index: Option<usize>,
    report: &crate::stats::CrawlReport,
) {
    let Some(metrics) = production_metrics() else {
        return;
    };
    let attrs = final_report_attributes(spider, spider_index, report.stop_reason);

    metrics.store_queued.add(report.store.queued, &attrs);
    metrics.store_written.add(report.store.written, &attrs);
    metrics
        .store_failed_writes
        .add(report.store.failed_writes, &attrs);
    metrics
        .store_failed_batches
        .add(report.store.failed_batches, &attrs);
    metrics
        .store_queue_full_waits
        .add(report.store.queue_full_waits, &attrs);

    if report.store.queued > 0 {
        metrics.store_queue_wait.record(
            report.store.average_queue_wait_per_item().as_secs_f64(),
            &attrs,
        );
    }
    if report.store.batches + report.store.failed_batches > 0 {
        metrics
            .store_write
            .record(report.store.average_write_per_batch().as_secs_f64(), &attrs);
    }
}

fn production_metrics() -> Option<ProductionMetrics> {
    PRODUCTION_METRICS
        .get()
        .and_then(|slot| slot.lock().ok().and_then(|metrics| metrics.clone()))
}

fn crawl_attributes(spider: &str, spider_index: Option<usize>) -> Vec<opentelemetry::KeyValue> {
    let mut attrs = vec![opentelemetry::KeyValue::new("spider", spider.to_string())];
    if let Some(index) = spider_index {
        attrs.push(opentelemetry::KeyValue::new("spider.index", index as i64));
    }
    attrs
}

fn domain_attributes(
    spider: &str,
    spider_index: Option<usize>,
    domain: &str,
) -> Vec<opentelemetry::KeyValue> {
    let mut attrs = crawl_attributes(spider, spider_index);
    attrs.push(opentelemetry::KeyValue::new("domain", domain.to_string()));
    attrs
}

fn skip_attributes(
    spider: &str,
    spider_index: Option<usize>,
    domain: &str,
    reason: &'static str,
) -> Vec<opentelemetry::KeyValue> {
    let mut attrs = domain_attributes(spider, spider_index, domain);
    attrs.push(opentelemetry::KeyValue::new("skip.reason", reason));
    attrs
}

fn error_attributes(
    spider: &str,
    spider_index: Option<usize>,
    domain: &str,
    error_kind: String,
) -> Vec<opentelemetry::KeyValue> {
    let mut attrs = domain_attributes(spider, spider_index, domain);
    attrs.push(opentelemetry::KeyValue::new("error.kind", error_kind));
    attrs
}

fn item_drop_attributes(
    spider: &str,
    spider_index: Option<usize>,
    reason: &'static str,
    error_kind: Option<crate::error::KumoErrorKind>,
) -> Vec<opentelemetry::KeyValue> {
    let mut attrs = crawl_attributes(spider, spider_index);
    attrs.push(opentelemetry::KeyValue::new("drop.reason", reason));
    if let Some(kind) = error_kind {
        attrs.push(opentelemetry::KeyValue::new("error.kind", kind.as_str()));
    }
    attrs
}

fn final_report_attributes(
    spider: &str,
    spider_index: Option<usize>,
    stop_reason: Option<crate::stats::StopReason>,
) -> Vec<opentelemetry::KeyValue> {
    let mut attrs = crawl_attributes(spider, spider_index);
    if let Some(reason) = stop_reason {
        attrs.push(opentelemetry::KeyValue::new("stop.reason", reason.as_str()));
    }
    attrs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(attrs: &[opentelemetry::KeyValue]) -> Vec<(String, String)> {
        attrs
            .iter()
            .map(|attr| (attr.key.as_str().to_string(), attr.value.to_string()))
            .collect()
    }

    #[test]
    fn domain_attributes_include_spider_index_and_domain() {
        let attrs = domain_attributes("books", Some(2), "example.com");
        let labels = labels(&attrs);

        assert!(labels.contains(&("spider".to_string(), "books".to_string())));
        assert!(labels.contains(&("spider.index".to_string(), "2".to_string())));
        assert!(labels.contains(&("domain".to_string(), "example.com".to_string())));
    }

    #[test]
    fn skip_attributes_include_stable_reason() {
        let attrs = skip_attributes("books", None, "example.com", "robots_txt");
        let labels = labels(&attrs);

        assert!(labels.contains(&("skip.reason".to_string(), "robots_txt".to_string())));
    }

    #[test]
    fn item_drop_attributes_include_optional_error_kind() {
        let attrs = item_drop_attributes(
            "books",
            None,
            "pipeline_error",
            Some(crate::error::KumoErrorKind::Parse),
        );
        let labels = labels(&attrs);

        assert!(labels.contains(&("drop.reason".to_string(), "pipeline_error".to_string())));
        assert!(labels.contains(&("error.kind".to_string(), "parse".to_string())));
    }
}
