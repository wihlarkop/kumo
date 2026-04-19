# Kumo Roadmap

## Polish / Correctness

These are quality improvements that make the framework more robust and production-ready.

| # | Item | Description |
|---|------|-------------|
| P1 | Integration tests | End-to-end tests using `mockito`: spin up a mock server, run a real spider, assert on `CrawlStats` and output. Catches pipeline regressions. |
| P2 | BrowserFetcher tab pool | Maintain a pool of open tabs sized to `concurrency` instead of open/close per fetch. Saves ~100–300ms per page for browser-heavy crawls. |
| P3 | RateLimiter + AutoThrottle conflict warning | Emit `tracing::warn!` on engine startup if both are in the middleware chain — they compound and slow the crawl 2x. |
| P4 | CI feature-flag matrix | Test each feature flag (`browser`, `postgres`, `sqlite`, `mysql`, `llm`, `jsonpath`) and combinations in GitHub Actions. Catches stale `#[cfg]` guards. |
| P5 | Structured `KumoError` variants | Replace `Parse(String)` and `Store(String)` with `#[source]`-bearing variants so downstream callers can pattern-match on the root cause. |
| P6 | `Spider::parse` takes `&Response` | Change from owned `Response` to `&Response` so spiders can pass it to multiple helpers without field cloning. Pre-publish breaking change. |
| P7 | Document proxy cookie isolation | Add a doc comment on `ProxyRotator` explaining each proxy client has its own cookie jar — desired for anonymity but surprising if the user expects cookie sharing. |

## Nice to Have / Future

Larger features that would make kumo significantly more competitive.

| # | Item | Description |
|---|------|-------------|
| F1 | Prometheus metrics endpoint | Expose `CrawlStats` + per-middleware counters via a `/metrics` HTTP endpoint. Bonus: periodic `tracing::info!` of live stats during long crawls. |
| F2 | `RedisFrontier` for distributed crawls | Redis-backed `Frontier` impl so multiple kumo processes can share work. Scrapy-Redis is the most-used Scrapy extension. Requires `redis` feature flag. |
| F3 | `SitemapSpider` base type | Auto-fetches `/sitemap.xml` and sitemap index files, extracts all URLs as `start_urls`. Low effort, commonly requested. |
| F4 | Typed `Output<T>` generic | Replace `Output` with `Output<T: Serialize>` so spiders return typed items. Eliminates stringly-typed `serde_json::Value` field access and gives compile-time safety. |

---

> Items in **Polish** are planned for the next sprint.
> Items in **Nice to Have** are designed but not yet scheduled.
