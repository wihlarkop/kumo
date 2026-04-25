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

use crate::error::KumoError;

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
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::{Resource, runtime, trace as sdktrace};
    use tracing_subscriber::prelude::*;

    let service_name = service_name.into();
    let endpoint = otlp_endpoint.into();

    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(&endpoint)
        .build_span_exporter()
        .map_err(|e| KumoError::store_msg(format!("otel exporter: {e}")))?;

    let provider = sdktrace::TracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_config(
            sdktrace::config().with_resource(Resource::new(vec![KeyValue::new(
                "service.name",
                service_name.clone(),
            )])),
        )
        .build();

    opentelemetry::global::set_tracer_provider(provider.clone());

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
    opentelemetry::global::shutdown_tracer_provider();
}
