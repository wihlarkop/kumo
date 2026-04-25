# Middleware

Middleware intercepts requests and responses. Register middleware with `.middleware()` on the engine builder — they are applied in registration order.

## DefaultHeaders

Set fixed headers on every request:

```rust
.middleware(
    DefaultHeaders::new()
        .user_agent("my-bot/1.0")
        .header("Accept-Language", "en-US")
)
```

## RateLimiter

Token-bucket rate limiter via `governor`:

```rust
use std::num::NonZeroU32;

.middleware(RateLimiter::per_second(NonZeroU32::new(5).unwrap()))
// or
.middleware(RateLimiter::per_minute(NonZeroU32::new(60).unwrap()))
```

Requests that exceed the limit are held until a token is available — no requests are dropped.

## AutoThrottle

Adaptive delay based on EWMA server latency. Automatically slows down when the server is struggling and speeds up when it's fast:

```rust
.middleware(
    AutoThrottle::new()
        .target_latency(Duration::from_millis(500))  // aim for 500ms response time
        .min_delay(Duration::from_millis(100))
        .max_delay(Duration::from_secs(5))
)
```

Also backs off automatically on `429 Too Many Requests` and `503 Service Unavailable`.

## StatusRetry

Retry on specific HTTP status codes:

```rust
.middleware(
    StatusRetry::new()
        .on_status(429)
        .on_status(503)
        .max_attempts(3)
        .base_delay(Duration::from_millis(500))
)
```

## ProxyRotator

Rotate through a list of proxy URLs per request:

```rust
.middleware(
    ProxyRotator::new(vec![
        "http://proxy1:8080".into(),
        "http://proxy2:8080".into(),
        "socks5://proxy3:1080".into(),
    ])
)
```

Proxies are cycled in round-robin order.

## UserAgentRotator

Rotate User-Agent strings per request:

```rust
.middleware(
    UserAgentRotator::new(vec![
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ...".into(),
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 ...".into(),
    ])
)
```

## Retry Policy

For full retry control, use `.retry_policy()` instead of `.retry()`:

```rust
.retry_policy(
    RetryPolicy::new(3)
        .base_delay(Duration::from_millis(200))
        .max_delay(Duration::from_secs(30))
        .jitter(true)          // add random ±25% jitter
        .on_status(429)
        .on_status(503)
)
```

## Custom Middleware

Implement the `Middleware` trait:

```rust
use kumo::prelude::*;
use async_trait::async_trait;

pub struct AddApiKey(String);

#[async_trait]
impl Middleware for AddApiKey {
    async fn before_request(&self, req: &mut Request) -> Result<(), KumoError> {
        req.headers_mut().insert(
            "X-Api-Key",
            self.0.parse().unwrap(),
        );
        Ok(())
    }
}

// Register:
.middleware(AddApiKey("secret-key".into()))
```
