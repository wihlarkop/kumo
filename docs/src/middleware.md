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
.middleware(RateLimiter::per_second(5.0))   // 5 requests per second
.middleware(RateLimiter::per_second(0.5))   // 1 request every 2 seconds
```

Requests that exceed the limit are held until a token is available — no requests are dropped.

## AutoThrottle

Adaptive delay based on EWMA server latency. Automatically slows down when the server is struggling and speeds up when it's fast:

```rust
.middleware(
    AutoThrottle::new()
        .target_concurrency(1.0)              // aim for 1 concurrent request (default)
        .start_delay(Duration::from_millis(500))
        .min_delay(Duration::from_millis(100))
        .max_delay(Duration::from_secs(60))
)
```

Also backs off automatically on `429 Too Many Requests` and `503 Service Unavailable`.

## StatusRetry

Retry on specific HTTP status codes:

```rust
.middleware(
    StatusRetry::new()
)
```

`StatusRetry::new()` retries 429, 500, 502, 503, and 504 by default. Use
`StatusRetry::with_codes(vec![429, 503])` to replace that global set, or
`for_pattern()` to override retryable statuses for matching URLs:

```rust
.middleware(
    StatusRetry::with_codes(vec![429, 503])
        .for_pattern(r"\.(js|css|png|jpg)$", vec![])
)
```

`StatusRetry` only turns matching responses into retryable HTTP-status errors.
The engine's `.retry()` or `.retry_policy()` setting controls how many times
the request is retried and how long each retry waits.

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
        .jitter(true)          // add up to 25% extra delay
        .on_status(429)
        .on_status(503)
)
```

`RetryPolicy::new(3)` means up to three retries after the initial fetch. Without
`.on_status()`, the policy retries any `KumoError::HttpStatus` or
`KumoError::Fetch`. Once a status filter is configured, only matching
HTTP-status errors are retried.

## Custom Middleware

Implement the `Middleware` trait:

```rust
use kumo::prelude::*;
use async_trait::async_trait;
use reqwest::header::{HeaderName, HeaderValue};

pub struct AddApiKey(String);

#[async_trait]
impl Middleware for AddApiKey {
    async fn before_request(&self, req: &mut FetchRequest) -> Result<(), KumoError> {
        req.headers_mut().insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_str(&self.0).unwrap(),
        );
        Ok(())
    }
}

// Register:
.middleware(AddApiKey("secret-key".into()))
```

