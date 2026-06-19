pub mod memory;

#[cfg(feature = "persistence")]
pub mod file;

#[cfg(feature = "redis-frontier")]
pub mod redis_frontier;

pub use memory::MemoryFrontier;

#[cfg(feature = "persistence")]
pub use file::{FileFrontier, FileFrontierState};

#[cfg(feature = "redis-frontier")]
pub use redis_frontier::RedisFrontier;

use std::{
    fmt,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

use crate::error::KumoError;
use crate::request::{CrawlRequest, FrontierRequest};

static EPHEMERAL_LEASE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Identifier for a leased frontier request.
///
/// Durable frontier implementations may persist this value and use it to ack,
/// release, or dead-letter an in-flight request. The default frontier lease
/// implementation creates ephemeral IDs and keeps the current pop-only
/// semantics.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrontierLeaseId(String);

impl FrontierLeaseId {
    /// Create a lease ID from a stable string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Return the lease ID as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FrontierLeaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A request that has been leased from a frontier for in-flight processing.
#[derive(Debug, Clone)]
pub struct FrontierLease {
    id: FrontierLeaseId,
    request: FrontierRequest,
    expires_at: Option<SystemTime>,
    delivery_count: u32,
}

impl FrontierLease {
    /// Create a durable lease record.
    pub fn new(
        id: FrontierLeaseId,
        request: FrontierRequest,
        expires_at: Option<SystemTime>,
        delivery_count: u32,
    ) -> Self {
        Self {
            id,
            request,
            expires_at,
            delivery_count,
        }
    }

    /// Create an ephemeral lease from a popped request.
    ///
    /// This is used by the default [`Frontier::lease_request`] implementation
    /// so existing frontiers retain current behavior until they opt into
    /// durable lease storage.
    pub fn ephemeral(request: FrontierRequest, ttl: Duration) -> Self {
        let sequence = EPHEMERAL_LEASE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self::new(
            FrontierLeaseId::new(format!("ephemeral-{sequence}")),
            request,
            Some(SystemTime::now() + ttl),
            1,
        )
    }

    /// Lease identifier used for ack, release, and dead-letter operations.
    pub fn id(&self) -> &FrontierLeaseId {
        &self.id
    }

    /// The leased request.
    pub fn request(&self) -> &FrontierRequest {
        &self.request
    }

    /// Consume the lease and return the request.
    pub fn into_request(self) -> FrontierRequest {
        self.request
    }

    /// When the lease should be considered expired.
    pub fn expires_at(&self) -> Option<SystemTime> {
        self.expires_at
    }

    /// Number of times this request has been delivered by the frontier.
    pub fn delivery_count(&self) -> u32 {
        self.delivery_count
    }
}

/// Reason a leased request was moved to a dead-letter queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadLetterReason {
    /// The request exhausted retry capacity.
    RetryExhausted,
    /// The request failed permanently.
    Failed,
    /// The crawl was interrupted before the request could complete.
    Interrupted,
    /// Application-specific reason.
    Custom(String),
}

impl DeadLetterReason {
    /// Return a stable label for reports and storage.
    pub fn as_str(&self) -> &str {
        match self {
            Self::RetryExhausted => "retry_exhausted",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Custom(reason) => reason.as_str(),
        }
    }
}

/// URL queue with deduplication. The frontier drives the crawl loop.
#[async_trait::async_trait]
pub trait Frontier: Send + Sync {
    /// Enqueue a URL if it has not been seen before.
    /// Returns `true` if added, `false` if it was a duplicate.
    async fn push(&self, url: String, depth: usize) -> bool;

    /// Enqueue a URL unconditionally, bypassing the deduplication filter.
    /// Used by `ErrorPolicy::Retry` to re-queue a URL that previously failed.
    /// `retry_count` tracks how many times this URL has been retried.
    async fn push_force(&self, url: String, depth: usize, retry_count: u32);

    /// Dequeue the next URL. Returns `None` if the queue is currently empty.
    async fn pop(&self) -> Option<(String, usize, u32)>;

    /// Enqueue a crawl request if it has not been seen before.
    async fn push_request(&self, request: CrawlRequest, depth: usize) -> bool {
        self.push(request.url().to_string(), depth).await
    }

    /// Enqueue a crawl request unconditionally, bypassing deduplication.
    async fn push_request_force(&self, queued: FrontierRequest) {
        self.push_force(
            queued.request.url().to_string(),
            queued.depth,
            queued.retry_count,
        )
        .await;
    }

    /// Dequeue the next crawl request.
    async fn pop_request(&self) -> Option<FrontierRequest> {
        self.pop().await.map(|(url, depth, retry_count)| {
            FrontierRequest::new(CrawlRequest::get(url), depth, retry_count)
        })
    }

    /// Lease the next crawl request for in-flight processing.
    ///
    /// The default implementation preserves current frontier semantics by
    /// popping a request and wrapping it in an ephemeral lease. Durable
    /// frontiers can override this to move the request into persisted
    /// in-flight state until it is acked, released, or dead-lettered.
    async fn lease_request(&self, ttl: Duration) -> Option<FrontierLease> {
        self.pop_request()
            .await
            .map(|request| FrontierLease::ephemeral(request, ttl))
    }

    /// Mark a leased request as completed.
    ///
    /// Default frontiers use ephemeral leases and therefore have no durable
    /// state to clear.
    async fn ack_lease(&self, _lease_id: &FrontierLeaseId) -> Result<(), KumoError> {
        Ok(())
    }

    /// Return a leased request to the frontier for future delivery.
    ///
    /// Default frontiers use current pop semantics and cannot restore a request
    /// without a durable lease store, so this is a no-op unless overridden.
    async fn release_lease(&self, _lease_id: &FrontierLeaseId) -> Result<(), KumoError> {
        Ok(())
    }

    /// Move a leased request to a dead-letter queue.
    ///
    /// Default frontiers do not store dead letters. Durable frontiers can
    /// override this to persist terminal failures for audit or replay.
    async fn dead_letter(
        &self,
        _lease_id: &FrontierLeaseId,
        _reason: DeadLetterReason,
    ) -> Result<(), KumoError> {
        Ok(())
    }

    /// Whether this frontier persists leased in-flight requests.
    ///
    /// The scheduler only uses [`lease_request`](Self::lease_request) when this
    /// returns `true`; otherwise it keeps the existing pop/requeue behavior so
    /// legacy frontiers do not lose deferred requests.
    fn supports_leases(&self) -> bool {
        false
    }

    /// Number of URLs waiting in the queue.
    async fn len(&self) -> usize;

    /// Returns `true` if the queue is empty.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Flush any pending frontier state to durable storage.
    async fn flush(&self) -> Result<(), KumoError> {
        Ok(())
    }
}
