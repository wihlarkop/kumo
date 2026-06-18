use std::{
    sync::{
        Arc,
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

/// Configuration for Kumo's bounded asynchronous store writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreBufferConfig {
    /// Maximum number of accepted items waiting for the store writer.
    pub queue_capacity: usize,
    /// Maximum number of items written per `ItemStore::store_many` call.
    pub batch_size: usize,
}

impl StoreBufferConfig {
    /// Create a bounded store buffer configuration.
    ///
    /// `queue_capacity` and `batch_size` are clamped to at least `1`.
    pub fn new(queue_capacity: usize, batch_size: usize) -> Self {
        Self {
            queue_capacity: queue_capacity.max(1),
            batch_size: batch_size.max(1),
        }
    }
}

impl Default for StoreBufferConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 1024,
            batch_size: 64,
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
    flushes: AtomicU64,
    queue_full_waits: AtomicU64,
    queue_wait_nanos: AtomicU64,
    write_nanos: AtomicU64,
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
            flushes: self.flushes.load(Ordering::Relaxed),
            queue_full_waits: self.queue_full_waits.load(Ordering::Relaxed),
            queue_wait: Duration::from_nanos(self.queue_wait_nanos.load(Ordering::Relaxed)),
            write: Duration::from_nanos(self.write_nanos.load(Ordering::Relaxed)),
        }
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
            .map_err(|_| KumoError::store_msg("buffered store writer stopped"))?;
        self.counters.queued.fetch_add(1, Ordering::Relaxed);
        add_duration(&self.counters.queue_wait_nanos, started.elapsed());
        Ok(())
    }

    async fn flush(&self) -> Result<(), KumoError> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StoreMessage::Flush(tx))
            .await
            .map_err(|_| KumoError::store_msg("buffered store writer stopped before flush"))?;
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
                    tracing::error!(
                        target: target::ITEM,
                        event = event::STORE_BUFFER_ERROR,
                        error = %err,
                        error_kind = err.kind().as_str(),
                        "store.buffer_error"
                    );
                    break;
                }
            }
            StoreMessage::Flush(reply) => {
                let result = match flush_batch(&inner, &mut batch, &counters).await {
                    Ok(()) => inner.flush().await,
                    Err(err) => Err(err),
                };
                if result.is_ok() {
                    counters.flushes.fetch_add(1, Ordering::Relaxed);
                }
                let stop = result.is_err();
                let _ = reply.send(result);
                if stop {
                    break;
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
    inner.store_many(batch).await?;
    add_duration(&counters.write_nanos, started.elapsed());
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
