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

When a matching response includes a valid `Retry-After` header, Kumo uses that
server-provided delay before retrying. Both delta-seconds and HTTP-date formats
are supported, and the delay is capped by the policy's `.max_delay(...)` value.

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

Proxies are cycled in round-robin order. Kumo lazily creates and caches one
HTTP client per proxy URL, so connections are reused without sharing cookies
or connection pools between proxies. These clients inherit the engine's
concurrency, request timeout, User-Agent, and TCP keepalive settings.

`ProxyRotator` also tracks per-proxy successes, failures, consecutive failures,
and circuit-breaker state. By default, a proxy circuit opens for 60 seconds
after three consecutive failed request attempts, and open proxies are skipped:

```rust
use std::time::Duration;

.middleware(
    ProxyRotator::new(vec![
        "http://proxy1:8080".into(),
        "http://proxy2:8080".into(),
    ])
    .cooldown_after(2, Duration::from_secs(30))
)
```

Use `.without_cooldown()` to keep health counters without skipping proxies.
`ProxyRotator` clones share health state, so keep a clone before registering
middleware when you want to inspect `ProxyHealthSnapshot` counters later:

```rust
let proxies = ProxyRotator::new(vec!["http://proxy1:8080".to_string()]);
let proxy_health = proxies.clone();

let report = CrawlEngine::builder()
    .middleware(proxies)
    .run(MySpider)
    .await?;

for proxy in proxy_health.circuit_health() {
    println!(
        "{} state={:?} successes={} failures={}",
        proxy.proxy, proxy.circuit_state, proxy.successes, proxy.failures
    );
}
```

`ProxyCircuitSnapshot::circuit_state` is `Healthy` when the proxy is selectable
with a closed circuit, `Open` while the proxy is cooling down, and `Recovering`
after the cooldown has elapsed and the next selection is a trial request. Use
`ProxyRotator::health()` when you only need the backward-compatible success,
failure, and cooldown counters. When every configured proxy is open,
`ProxyRotator` leaves `request.proxy` unset for that request instead of forcing
a known unhealthy proxy.

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

If middleware provides a retry delay hint, such as `StatusRetry` parsing a
`Retry-After` header, that hint is preferred over exponential backoff and capped
by `.max_delay(...)`.

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

