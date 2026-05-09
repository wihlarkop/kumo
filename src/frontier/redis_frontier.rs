use redis::{AsyncCommands, Client};

use crate::{
    error::KumoError,
    request::{CrawlRequest, FrontierRequest, StoredFrontierRequest},
};

use super::Frontier;

/// Frontier backed by Redis — enables multi-process distributed crawling.
///
/// Uses a Redis LIST (`queue_key`) for the URL queue and a Redis SET
/// (`seen_key`) for deduplication. Multiple kumo processes pointing at
/// the same Redis instance and key names will cooperatively drain the frontier.
///
/// Supports both standard Redis (via `redis://`) and TLS connections
/// (Upstash and other managed Redis providers via `rediss://`).
///
/// # Example
/// ```rust,ignore
/// let frontier = RedisFrontier::new(
///     "rediss://default:token@host:6379",
///     "kumo:queue",
///     "kumo:seen",
/// ).await?;
///
/// CrawlEngine::builder()
///     .frontier(frontier)
///     .run(MySpider)
///     .await?;
/// ```
pub struct RedisFrontier {
    client: Client,
    queue_key: String,
    seen_key: String,
}

impl RedisFrontier {
    /// Connect to Redis and verify the connection with a PING.
    pub async fn new(
        url: &str,
        queue_key: impl Into<String>,
        seen_key: impl Into<String>,
    ) -> Result<Self, KumoError> {
        let client = Client::open(url).map_err(|e| KumoError::store("redis connect", e))?;

        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| KumoError::store("redis get connection", e))?;

        let _pong: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| KumoError::store("redis ping", e))?;

        Ok(Self {
            client,
            queue_key: queue_key.into(),
            seen_key: seen_key.into(),
        })
    }

    /// Delete the queue and seen keys to start a fresh crawl.
    pub async fn clear(&self) -> Result<(), KumoError> {
        let mut conn = self.conn().await?;
        redis::pipe()
            .del(&self.queue_key)
            .del(&self.seen_key)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| KumoError::store("redis clear", e))
    }

    async fn conn(&self) -> Result<redis::aio::MultiplexedConnection, KumoError> {
        self.client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| KumoError::store("redis connection", e))
    }
}

#[async_trait::async_trait]
impl Frontier for RedisFrontier {
    async fn push(&self, url: String, depth: usize) -> bool {
        self.push_request(CrawlRequest::get(url), depth).await
    }

    async fn push_force(&self, url: String, depth: usize, retry_count: u32) {
        self.push_request_force(FrontierRequest::new(
            CrawlRequest::get(url),
            depth,
            retry_count,
        ))
        .await;
    }

    async fn pop(&self) -> Option<(String, usize, u32)> {
        self.pop_request().await.map(|queued| {
            (
                queued.request().url().to_string(),
                queued.depth(),
                queued.retry_count(),
            )
        })
    }

    async fn push_request(&self, request: CrawlRequest, depth: usize) -> bool {
        let Ok(mut conn) = self.conn().await else {
            return false;
        };
        // SADD returns 1 if new member, 0 if already present.
        let added: i64 = if request.dont_filter_enabled() {
            1
        } else {
            conn.sadd(&self.seen_key, request.url()).await.unwrap_or(0)
        };
        if !request.dont_filter_enabled() && added == 0 {
            return false;
        }
        let Ok(entry) = serde_json::to_string(&StoredFrontierRequest::from(&FrontierRequest::new(
            request, depth, 0,
        ))) else {
            return false;
        };
        let _: () = conn.rpush(&self.queue_key, entry).await.unwrap_or(());
        true
    }

    async fn push_request_force(&self, queued: FrontierRequest) {
        let Ok(mut conn) = self.conn().await else {
            return;
        };
        let Ok(entry) = serde_json::to_string(&StoredFrontierRequest::from(&queued)) else {
            return;
        };
        let _: () = conn.rpush(&self.queue_key, entry).await.unwrap_or(());
    }

    async fn pop_request(&self) -> Option<FrontierRequest> {
        let mut conn = self.conn().await.ok()?;
        // LPOP returns the leftmost element (oldest enqueued).
        let raw: Option<String> = conn.lpop(&self.queue_key, None).await.ok()?;
        let raw = raw?;
        serde_json::from_str::<StoredFrontierRequest>(&raw)
            .ok()
            .and_then(|stored| FrontierRequest::try_from(stored).ok())
    }

    async fn len(&self) -> usize {
        let Ok(mut conn) = self.conn().await else {
            return 0;
        };
        conn.llen(&self.queue_key).await.unwrap_or(0)
    }
}
