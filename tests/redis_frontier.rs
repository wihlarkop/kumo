//! Integration tests for RedisFrontier using an Upstash Redis instance.
//!
//! Requires REDIS_URL in the environment (or .env file).
//! These tests are `#[ignore]` by default — run with:
//!   cargo test --features redis-frontier --test redis_frontier -- --ignored

#[cfg(feature = "redis-frontier")]
mod tests {
    use kumo::RedisFrontier;
    use kumo::frontier::Frontier;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn redis_url() -> String {
        if let Ok(content) = std::fs::read_to_string(".env") {
            for line in content.lines() {
                if let Some(val) = line.strip_prefix("REDIS_URL=") {
                    return val.trim().to_string();
                }
            }
        }
        std::env::var("REDIS_URL").expect("REDIS_URL not set in .env or environment")
    }

    /// Generate unique Redis keys per test invocation to avoid cross-test interference.
    static KEY_COUNTER: AtomicU64 = AtomicU64::new(0);
    fn unique_keys() -> (String, String) {
        let id = KEY_COUNTER.fetch_add(1, Ordering::Relaxed);
        (
            format!("kumo_test:queue:{id}"),
            format!("kumo_test:seen:{id}"),
        )
    }

    #[tokio::test]
    #[ignore]
    async fn push_and_pop() {
        let (qk, sk) = unique_keys();
        let f = RedisFrontier::new(&redis_url(), &qk, &sk).await.unwrap();

        assert!(f.push("https://example.com/a".into(), 0).await);
        let item = f.pop().await.unwrap();
        assert_eq!(item.0, "https://example.com/a");
        assert_eq!(item.1, 0);
        assert_eq!(item.2, 0);

        f.clear().await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn deduplication_works() {
        let (qk, sk) = unique_keys();
        let f = RedisFrontier::new(&redis_url(), &qk, &sk).await.unwrap();

        assert!(f.push("https://example.com".into(), 0).await);
        assert!(
            !f.push("https://example.com".into(), 0).await,
            "duplicate should be rejected"
        );
        assert_eq!(f.len().await, 1);

        f.clear().await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn push_force_bypasses_dedup() {
        let (qk, sk) = unique_keys();
        let f = RedisFrontier::new(&redis_url(), &qk, &sk).await.unwrap();

        f.push("https://example.com".into(), 0).await;
        f.push_force("https://example.com".into(), 0, 1).await;
        assert_eq!(f.len().await, 2);

        f.clear().await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn fifo_order_preserved() {
        let (qk, sk) = unique_keys();
        let f = RedisFrontier::new(&redis_url(), &qk, &sk).await.unwrap();

        f.push("https://a.com".into(), 0).await;
        f.push("https://b.com".into(), 0).await;
        f.push("https://c.com".into(), 0).await;

        assert_eq!(f.pop().await.unwrap().0, "https://a.com");
        assert_eq!(f.pop().await.unwrap().0, "https://b.com");
        assert_eq!(f.pop().await.unwrap().0, "https://c.com");

        f.clear().await.unwrap();
    }
}
