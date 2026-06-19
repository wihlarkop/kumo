use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};

use crate::{
    error::KumoError,
    request::{CrawlRequest, FrontierRequest, StoredFrontierRequest},
};

use super::{DeadLetterReason, Frontier, FrontierLease, FrontierLeaseId};

const REDIS_BATCH_LIMIT: usize = 128;

const LEASE_SCRIPT: &str = r#"
local raw = redis.call('LPOP', KEYS[1])
if not raw then
  return nil
end

local delivery = redis.call('HINCRBY', KEYS[4], raw, 1)
local lease = cjson.encode({ request = raw, delivery_count = delivery })
redis.call('HSET', KEYS[2], ARGV[1], lease)
redis.call('ZADD', KEYS[3], ARGV[2], ARGV[1])
return { raw, delivery }
"#;

const ACK_SCRIPT: &str = r#"
local lease = redis.call('HGET', KEYS[1], ARGV[1])
if lease then
  local decoded = cjson.decode(lease)
  redis.call('HDEL', KEYS[3], decoded.request)
end
redis.call('HDEL', KEYS[1], ARGV[1])
redis.call('ZREM', KEYS[2], ARGV[1])
return 1
"#;

const RELEASE_SCRIPT: &str = r#"
local lease = redis.call('HGET', KEYS[1], ARGV[1])
if not lease then
  redis.call('ZREM', KEYS[2], ARGV[1])
  return 0
end

local decoded = cjson.decode(lease)
redis.call('HDEL', KEYS[1], ARGV[1])
redis.call('ZREM', KEYS[2], ARGV[1])
redis.call('RPUSH', KEYS[3], decoded.request)
return 1
"#;

const DEAD_LETTER_SCRIPT: &str = r#"
local lease = redis.call('HGET', KEYS[1], ARGV[1])
if not lease then
  redis.call('ZREM', KEYS[2], ARGV[1])
  return 0
end

local decoded = cjson.decode(lease)
redis.call('HDEL', KEYS[1], ARGV[1])
redis.call('ZREM', KEYS[2], ARGV[1])
redis.call('HDEL', KEYS[4], decoded.request)
redis.call('RPUSH', KEYS[3], cjson.encode({
  id = ARGV[1],
  request = decoded.request,
  reason = ARGV[2],
  delivery_count = decoded.delivery_count,
  dead_lettered_at_ms = ARGV[3]
}))
return 1
"#;

const RECLAIM_LEASES_SCRIPT: &str = r#"
local ids = redis.call('ZRANGEBYSCORE', KEYS[2], '-inf', ARGV[1], 'LIMIT', 0, ARGV[2])
local reclaimed = 0
for _, id in ipairs(ids) do
  local lease = redis.call('HGET', KEYS[1], id)
  if lease then
    local decoded = cjson.decode(lease)
    redis.call('RPUSH', KEYS[3], decoded.request)
    redis.call('HDEL', KEYS[1], id)
    reclaimed = reclaimed + 1
  end
  redis.call('ZREM', KEYS[2], id)
end
return reclaimed
"#;

const PROMOTE_DELAYED_SCRIPT: &str = r#"
local members = redis.call('ZRANGEBYSCORE', KEYS[1], '-inf', ARGV[1], 'LIMIT', 0, ARGV[2])
local promoted = 0
for _, member in ipairs(members) do
  if redis.call('ZREM', KEYS[1], member) == 1 then
    local decoded = cjson.decode(member)
    redis.call('RPUSH', KEYS[2], decoded.request)
    promoted = promoted + 1
  end
end
return promoted
"#;

fn system_time_to_ms(time: SystemTime) -> Option<u64> {
    let duration = time.duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(duration.as_millis()).ok()
}

fn now_ms() -> u64 {
    system_time_to_ms(SystemTime::now()).unwrap_or(0)
}

fn system_time_from_ms(ms: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RedisDelayedRequest {
    id: String,
    request: String,
}

/// Frontier backed by Redis for multi-process distributed crawling.
///
/// The public constructor keeps the simple queue/seen key API. Kumo derives
/// related keys from `queue_key` for delayed requests, in-progress leases,
/// lease deadlines, delivery counts, and dead letters.
///
/// Supports both standard Redis (`redis://`) and TLS connections (`rediss://`).
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
    delayed_key: String,
    leases_key: String,
    lease_deadlines_key: String,
    delivery_counts_key: String,
    dead_letters_key: String,
    sequence: AtomicU64,
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

        let queue_key = queue_key.into();
        Ok(Self {
            client,
            delayed_key: format!("{queue_key}:delayed"),
            leases_key: format!("{queue_key}:leases"),
            lease_deadlines_key: format!("{queue_key}:lease_deadlines"),
            delivery_counts_key: format!("{queue_key}:delivery_counts"),
            dead_letters_key: format!("{queue_key}:dead_letters"),
            queue_key,
            seen_key: seen_key.into(),
            sequence: AtomicU64::new(0),
        })
    }

    /// Delete all Redis keys owned by this frontier to start a fresh crawl.
    pub async fn clear(&self) -> Result<(), KumoError> {
        let mut conn = self.conn().await?;
        redis::pipe()
            .del(&self.queue_key)
            .del(&self.seen_key)
            .del(&self.delayed_key)
            .del(&self.leases_key)
            .del(&self.lease_deadlines_key)
            .del(&self.delivery_counts_key)
            .del(&self.dead_letters_key)
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

    fn next_id(&self, prefix: &str) -> String {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{}-{sequence}", now_ms())
    }

    fn serialize_request(queued: &FrontierRequest) -> Option<String> {
        serde_json::to_string(&StoredFrontierRequest::from(queued)).ok()
    }

    fn deserialize_request(raw: &str) -> Option<FrontierRequest> {
        serde_json::from_str::<StoredFrontierRequest>(raw)
            .ok()
            .and_then(|stored| FrontierRequest::try_from(stored).ok())
    }

    async fn reclaim_expired_leases(
        &self,
        conn: &mut redis::aio::MultiplexedConnection,
    ) -> Result<(), KumoError> {
        redis::cmd("EVAL")
            .arg(RECLAIM_LEASES_SCRIPT)
            .arg(3)
            .arg(&self.leases_key)
            .arg(&self.lease_deadlines_key)
            .arg(&self.queue_key)
            .arg(now_ms())
            .arg(REDIS_BATCH_LIMIT)
            .query_async::<i64>(conn)
            .await
            .map(|_| ())
            .map_err(|e| KumoError::store("redis reclaim leases", e))
    }

    async fn promote_due_delayed(
        &self,
        conn: &mut redis::aio::MultiplexedConnection,
    ) -> Result<(), KumoError> {
        redis::cmd("EVAL")
            .arg(PROMOTE_DELAYED_SCRIPT)
            .arg(2)
            .arg(&self.delayed_key)
            .arg(&self.queue_key)
            .arg(now_ms())
            .arg(REDIS_BATCH_LIMIT)
            .query_async::<i64>(conn)
            .await
            .map(|_| ())
            .map_err(|e| KumoError::store("redis promote delayed", e))
    }

    async fn prepare_ready_queue(
        &self,
        conn: &mut redis::aio::MultiplexedConnection,
    ) -> Result<(), KumoError> {
        self.reclaim_expired_leases(conn).await?;
        self.promote_due_delayed(conn).await
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
        let added: i64 = if request.dont_filter_enabled() {
            1
        } else {
            conn.sadd(&self.seen_key, request.dedup_key())
                .await
                .unwrap_or(0)
        };
        if !request.dont_filter_enabled() && added == 0 {
            return false;
        }

        let queued = FrontierRequest::new(request, depth, 0);
        let Some(entry) = Self::serialize_request(&queued) else {
            return false;
        };
        let _: () = conn.rpush(&self.queue_key, entry).await.unwrap_or(());
        true
    }

    async fn push_request_force(&self, queued: FrontierRequest) {
        let Ok(mut conn) = self.conn().await else {
            return;
        };
        let Some(entry) = Self::serialize_request(&queued) else {
            return;
        };

        let scheduled_at = queued.scheduled_at().and_then(system_time_to_ms);
        if scheduled_at.is_some_and(|scheduled_at| scheduled_at > now_ms()) {
            let delayed = RedisDelayedRequest {
                id: self.next_id("delayed"),
                request: entry,
            };
            if let (Some(score), Ok(member)) = (scheduled_at, serde_json::to_string(&delayed)) {
                let _: () = conn
                    .zadd(&self.delayed_key, member, score)
                    .await
                    .unwrap_or(());
            }
        } else {
            let _: () = conn.rpush(&self.queue_key, entry).await.unwrap_or(());
        }
    }

    async fn pop_request(&self) -> Option<FrontierRequest> {
        let mut conn = self.conn().await.ok()?;
        self.prepare_ready_queue(&mut conn).await.ok()?;
        let raw: Option<String> = conn.lpop(&self.queue_key, None).await.ok()?;
        Self::deserialize_request(&raw?)
    }

    async fn lease_request(&self, ttl: Duration) -> Option<FrontierLease> {
        let mut conn = self.conn().await.ok()?;
        self.prepare_ready_queue(&mut conn).await.ok()?;

        let id = FrontierLeaseId::new(self.next_id("redis-lease"));
        let expires_at = SystemTime::now() + ttl;
        let expires_at_ms = system_time_to_ms(expires_at)?;
        let result: Option<(String, u32)> = redis::cmd("EVAL")
            .arg(LEASE_SCRIPT)
            .arg(4)
            .arg(&self.queue_key)
            .arg(&self.leases_key)
            .arg(&self.lease_deadlines_key)
            .arg(&self.delivery_counts_key)
            .arg(id.as_str())
            .arg(expires_at_ms)
            .query_async(&mut conn)
            .await
            .ok()?;

        let (raw, delivery_count) = result?;
        let request = Self::deserialize_request(&raw)?;
        Some(FrontierLease::new(
            id,
            request,
            Some(expires_at),
            delivery_count,
        ))
    }

    async fn ack_lease(&self, lease_id: &FrontierLeaseId) -> Result<(), KumoError> {
        let mut conn = self.conn().await?;
        redis::cmd("EVAL")
            .arg(ACK_SCRIPT)
            .arg(3)
            .arg(&self.leases_key)
            .arg(&self.lease_deadlines_key)
            .arg(&self.delivery_counts_key)
            .arg(lease_id.as_str())
            .query_async::<i64>(&mut conn)
            .await
            .map(|_| ())
            .map_err(|e| KumoError::store("redis ack lease", e))
    }

    async fn release_lease(&self, lease_id: &FrontierLeaseId) -> Result<(), KumoError> {
        let mut conn = self.conn().await?;
        redis::cmd("EVAL")
            .arg(RELEASE_SCRIPT)
            .arg(3)
            .arg(&self.leases_key)
            .arg(&self.lease_deadlines_key)
            .arg(&self.queue_key)
            .arg(lease_id.as_str())
            .query_async::<i64>(&mut conn)
            .await
            .map(|_| ())
            .map_err(|e| KumoError::store("redis release lease", e))
    }

    async fn dead_letter(
        &self,
        lease_id: &FrontierLeaseId,
        reason: DeadLetterReason,
    ) -> Result<(), KumoError> {
        let mut conn = self.conn().await?;
        redis::cmd("EVAL")
            .arg(DEAD_LETTER_SCRIPT)
            .arg(4)
            .arg(&self.leases_key)
            .arg(&self.lease_deadlines_key)
            .arg(&self.dead_letters_key)
            .arg(&self.delivery_counts_key)
            .arg(lease_id.as_str())
            .arg(reason.as_str())
            .arg(now_ms())
            .query_async::<i64>(&mut conn)
            .await
            .map(|_| ())
            .map_err(|e| KumoError::store("redis dead letter", e))
    }

    fn supports_leases(&self) -> bool {
        true
    }

    async fn next_ready_delay(&self) -> Option<Duration> {
        let mut conn = self.conn().await.ok()?;
        let due: Vec<(String, f64)> = conn.zrange_withscores(&self.delayed_key, 0, 0).await.ok()?;
        let (_, score) = due.into_iter().next()?;
        let score = u64::try_from(score.max(0.0) as u128).ok()?;
        Some(
            system_time_from_ms(score)
                .duration_since(SystemTime::now())
                .unwrap_or(Duration::ZERO),
        )
    }

    async fn len(&self) -> usize {
        let Ok(mut conn) = self.conn().await else {
            return 0;
        };
        self.prepare_ready_queue(&mut conn).await.ok();
        let queued: usize = conn.llen(&self.queue_key).await.unwrap_or(0);
        let delayed: usize = conn.zcard(&self.delayed_key).await.unwrap_or(0);
        queued + delayed
    }
}
