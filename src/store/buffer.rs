use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use tokio::sync::{mpsc, oneshot};

use crate::{
    error::KumoError,
    logging::{event, target},
    stats::StoreStats,
};

use super::ItemStore;

/// Store-buffer behavior after the background writer observes a store error.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum StoreFailurePolicy {
    /// Abort the buffered writer and report the first store error to callers.
    #[default]
    Abort,
}

impl StoreFailurePolicy {
    /// Stable snake_case label for reports and configuration logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Abort => "abort",
        }
    }
}

/// Configuration for Kumo's bounded asynchronous store writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreBufferConfig {
    /// Maximum number of accepted items waiting for the store writer.
    pub queue_capacity: usize,
    /// Maximum number of items written per `ItemStore::store_many` call.
    pub batch_size: usize,
    /// Behavior after the background writer observes a store error.
    pub failure_policy: StoreFailurePolicy,
}

impl StoreBufferConfig {
    /// Create a bounded store buffer configuration.
    ///
    /// `queue_capacity` and `batch_size` are clamped to at least `1`.
    pub fn new(queue_capacity: usize, batch_size: usize) -> Self {
        Self {
            queue_capacity: queue_capacity.max(1),
            batch_size: batch_size.max(1),
            failure_policy: StoreFailurePolicy::Abort,
        }
    }

    /// Set how the buffer handles background store write failures.
    pub fn failure_policy(mut self, policy: StoreFailurePolicy) -> Self {
        self.failure_policy = policy;
        self
    }
}

impl Default for StoreBufferConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 1024,
            batch_size: 64,
            failure_policy: StoreFailurePolicy::Abort,
        }
    }
}

enum StoreMessage {
    Item(serde_json::Value),
    Flush(oneshot::Sender<Result<(), KumoError>>),
}

#[derive(Default)]
struct StoreBufferCounters {
    queued: AtomicU64,
    written: AtomicU64,
    batches: AtomicU64,
    failed_writes: AtomicU64,
    failed_batches: AtomicU64,
    flushes: AtomicU64,
    queue_full_waits: AtomicU64,
    queue_wait_nanos: AtomicU64,
    queue_wait_max_nanos: AtomicU64,
    write_nanos: AtomicU64,
    write_max_nanos: AtomicU64,
    first_failure: Mutex<Option<StoreBufferFailure>>,
}

#[derive(Debug, Clone)]
struct StoreBufferFailure {
    kind: &'static str,
    message: String,
}

impl StoreBufferCounters {
    fn snapshot(&self, config: StoreBufferConfig) -> StoreStats {
        StoreStats {
            buffered: true,
            queue_capacity: config.queue_capacity as u64,
            batch_size: config.batch_size as u64,
            queued: self.queued.load(Ordering::Relaxed),
            written: self.written.load(Ordering::Relaxed),
            batches: self.batches.load(Ordering::Relaxed),
            failed_writes: self.failed_writes.load(Ordering::Relaxed),
            failed_batches: self.failed_batches.load(Ordering::Relaxed),
            flushes: self.flushes.load(Ordering::Relaxed),
            queue_full_waits: self.queue_full_waits.load(Ordering::Relaxed),
            queue_wait: Duration::from_nanos(self.queue_wait_nanos.load(Ordering::Relaxed)),
            queue_wait_max: Duration::from_nanos(self.queue_wait_max_nanos.load(Ordering::Relaxed)),
            write: Duration::from_nanos(self.write_nanos.load(Ordering::Relaxed)),
            write_max: Duration::from_nanos(self.write_max_nanos.load(Ordering::Relaxed)),
        }
    }

    fn record_failure(&self, err: &KumoError) {
        let mut first_failure = self
            .first_failure
            .lock()
            .expect("store buffer failure mutex poisoned");
        first_failure.get_or_insert_with(|| StoreBufferFailure {
            kind: err.kind().as_str(),
            message: err.to_string(),
        });
    }

    fn failure_error(&self) -> Option<KumoError> {
        self.first_failure
            .lock()
            .expect("store buffer failure mutex poisoned")
            .as_ref()
            .map(|failure| {
                KumoError::store_msg(format!(
                    "buffered store writer aborted after {} error: {}",
                    failure.kind, failure.message
                ))
            })
    }
}

pub(crate) struct BufferedStore {
    tx: mpsc::Sender<StoreMessage>,
    config: StoreBufferConfig,
    counters: Arc<StoreBufferCounters>,
}

impl BufferedStore {
    pub(crate) fn new(inner: Arc<dyn ItemStore>, config: StoreBufferConfig) -> Self {
        let (tx, rx) = mpsc::channel(config.queue_capacity);
        let counters = Arc::new(StoreBufferCounters::default());
        tokio::spawn(store_worker(inner, rx, config, counters.clone()));
        Self {
            tx,
            config,
            counters,
        }
    }

    pub(crate) fn stats(&self) -> StoreStats {
        self.counters.snapshot(self.config)
    }
}

#[async_trait::async_trait]
impl ItemStore for BufferedStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        if let Some(err) = self.counters.failure_error() {
            return Err(err);
        }
        let waited_for_capacity = self.tx.capacity() == 0;
        if waited_for_capacity {
            self.counters
                .queue_full_waits
                .fetch_add(1, Ordering::Relaxed);
        }

        let started = std::time::Instant::now();
        self.tx
            .send(StoreMessage::Item(item.clone()))
            .await
            .map_err(|_| {
                self.counters
                    .failure_error()
                    .unwrap_or_else(|| KumoError::store_msg("buffered store writer stopped"))
            })?;
        self.counters.queued.fetch_add(1, Ordering::Relaxed);
        let elapsed = started.elapsed();
        add_duration(&self.counters.queue_wait_nanos, elapsed);
        update_max_duration(&self.counters.queue_wait_max_nanos, elapsed);
        Ok(())
    }

    async fn flush(&self) -> Result<(), KumoError> {
        if let Some(err) = self.counters.failure_error() {
            return Err(err);
        }
        let (tx, rx) = oneshot::channel();
        self.tx.send(StoreMessage::Flush(tx)).await.map_err(|_| {
            self.counters.failure_error().unwrap_or_else(|| {
                KumoError::store_msg("buffered store writer stopped before flush")
            })
        })?;
        rx.await
            .map_err(|_| KumoError::store_msg("buffered store flush response dropped"))?
    }
}

async fn store_worker(
    inner: Arc<dyn ItemStore>,
    mut rx: mpsc::Receiver<StoreMessage>,
    config: StoreBufferConfig,
    counters: Arc<StoreBufferCounters>,
) {
    let mut batch = Vec::with_capacity(config.batch_size);
    while let Some(message) = rx.recv().await {
        match message {
            StoreMessage::Item(item) => {
                batch.push(item);
                if batch.len() >= config.batch_size
                    && let Err(err) = flush_batch(&inner, &mut batch, &counters).await
                {
                    counters.record_failure(&err);
                    tracing::error!(
                        target: target::ITEM,
                        event = event::STORE_BUFFER_ERROR,
                        error = %err,
                        error_kind = err.kind().as_str(),
                        store_failure_policy = config.failure_policy.as_str(),
                        "store.buffer_error"
                    );
                    match config.failure_policy {
                        StoreFailurePolicy::Abort => break,
                    }
                }
            }
            StoreMessage::Flush(reply) => {
                let result = match flush_batch(&inner, &mut batch, &counters).await {
                    Ok(()) => inner.flush().await,
                    Err(err) => Err(err),
                };
                if result.is_ok() {
                    counters.flushes.fetch_add(1, Ordering::Relaxed);
                } else if let Err(err) = &result {
                    counters.record_failure(err);
                }
                let stop = result.is_err();
                let _ = reply.send(result);
                if stop {
                    match config.failure_policy {
                        StoreFailurePolicy::Abort => break,
                    }
                }
            }
        }
    }
}

async fn flush_batch(
    inner: &Arc<dyn ItemStore>,
    batch: &mut Vec<serde_json::Value>,
    counters: &StoreBufferCounters,
) -> Result<(), KumoError> {
    if batch.is_empty() {
        return Ok(());
    }

    let started = std::time::Instant::now();
    let result = inner.store_many(batch).await;
    let elapsed = started.elapsed();
    add_duration(&counters.write_nanos, elapsed);
    update_max_duration(&counters.write_max_nanos, elapsed);
    if let Err(err) = result {
        counters
            .failed_writes
            .fetch_add(batch.len() as u64, Ordering::Relaxed);
        counters.failed_batches.fetch_add(1, Ordering::Relaxed);
        return Err(err);
    }
    counters
        .written
        .fetch_add(batch.len() as u64, Ordering::Relaxed);
    counters.batches.fetch_add(1, Ordering::Relaxed);
    batch.clear();
    Ok(())
}

fn add_duration(target: &AtomicU64, duration: Duration) {
    let nanos = duration.as_nanos().min(u128::from(u64::MAX)) as u64;
    target.fetch_add(nanos, Ordering::Relaxed);
}

fn update_max_duration(target: &AtomicU64, duration: Duration) {
    let nanos = duration.as_nanos().min(u128::from(u64::MAX)) as u64;
    let mut current = target.load(Ordering::Relaxed);
    while nanos > current {
        match target.compare_exchange_weak(current, nanos, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}
