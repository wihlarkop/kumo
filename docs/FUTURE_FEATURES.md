# Kumo — Future Feature Backlog

Filtered suggestions based on Grok's code review. Items already implemented (item pipeline,
proxy rotation, Redis frontier) are excluded. Items requiring ecosystem maturity we don't
have yet (CLI tool, TOML-driven spiders, GraphQL, Parquet/Arrow, plugin system) are deferred.

---

## 1. Advanced `LinkExtractor` configuration

**What:** Extend `LinkExtractor` with allow/deny URL patterns and a `restrict_css` option so
links are only followed from a specific part of the page.

```rust
LinkExtractor::new()
    .allow(r"https://example\.com/products/\d+")
    .deny(r"\?page=")
    .restrict_css("nav.pagination")
```

**Why it matters:** Every non-trivial crawler needs this. Without it users write manual filter
closures that duplicate the same logic. Scrapy's `LinkExtractor` has these exact options and
they're the most-used feature after the basic selector.

**Effort:** Medium — purely additive to the existing `LinkExtractor` struct.

---

## 2. OpenTelemetry metrics

**What:** Emit spans and counters for the key engine events — requests sent, responses received,
items yielded, errors by type, queue depth, requests/sec. Use the `opentelemetry` + `tracing-opentelemetry`
crates behind an optional `otel` feature flag.

```toml
kumo = { version = "0.1", features = ["otel"] }
```

**Why it matters:** Production crawlers need observability. Right now there's no way to know
if a job is progressing, stalled, or rate-limited without printf debugging. OTEL plugs into
any existing Grafana/Honeycomb/Jaeger stack with zero user code changes.

**Effort:** Medium — add `otel` feature, instrument `engine.rs` `run()` loop, no API changes.

---

## 3. Browser auto-fallback heuristic

**What:** When the HTTP fetcher receives a response that looks like a JS-gated page (empty
`<body>`, `<noscript>` redirect, React root with no content), automatically retry with the
browser fetcher — then continue with the same parse pipeline.

```rust
CrawlEngine::builder()
    .browser_fallback_on_empty_body(true)  // opt-in
    .browser_config(BrowserConfig::new().headless(true))
```

**Why it matters:** The most common user pain point is "I built my spider with HTTP, it works
on 80% of sites, then hits a React SPA and gets nothing." Auto-fallback eliminates that cliff
edge without requiring users to switch fetcher globally.

**Effort:** Medium-high — needs a heuristic detector and the engine to carry both a HTTP and
browser fetcher simultaneously.

---

## 4. Retry / back-off policy on the engine

**What:** Configurable retry strategy for failed requests — max retries, exponential back-off
with jitter, per-status-code rules (retry 429/503, never retry 404).

```rust
CrawlEngine::builder()
    .retry_policy(
        RetryPolicy::exponential(3)
            .on_status(StatusCode::TOO_MANY_REQUESTS)
            .on_status(StatusCode::SERVICE_UNAVAILABLE)
    )
```

**Why it matters:** Every crawler needs this, but right now users have to implement it
themselves in `parse()`. A first-class policy lets the engine handle transient failures
uniformly and pairs naturally with the `RateLimit` middleware.

**Effort:** Low-medium — add `RetryPolicy` struct, wire into the fetch loop in `engine.rs`.

---

## 5. `extract` attribute: `default = "value"` and `transform`

**What:** Two new modifiers for `#[extract(...)]`:

- `default = "N/A"` — use this literal when the selector returns empty (instead of `""`)
- `transform = "trim|lowercase|uppercase|slugify"` — apply a named transform to the extracted value

```rust
#[extract(css = ".price", default = "0.00", transform = "trim")]
price: String,
```

**Why it matters:** Users currently do post-processing in `parse()` for every field. These
two options cover 90% of the post-processing use cases declaratively, keeping the struct
definition as the single source of truth.

**Effort:** Low — extend `ExtractArgs` + codegen in `kumo-derive`, no runtime changes.

---

## 6. Sitemap spider helper

**What:** A `SitemapSpider` base (or mixin trait) that reads `sitemap.xml` / `sitemap_index.xml`,
follows index sitemaps recursively, and seeds the engine frontier with the discovered URLs.

```rust
struct MySpider;
impl SitemapSpider for MySpider {
    const SITEMAP_URL: &'static str = "https://example.com/sitemap.xml";
}
```

**Why it matters:** Sitemap crawling is the preferred entry point for search-engine-friendly
sites (no JS, structured, respects `lastmod`). It's a common enough pattern to deserve a
first-class abstraction rather than boilerplate.

**Effort:** Low — HTTP fetch + XML parse (already have `sxd-document`), no engine changes.

---

## Notes on deferred items

| Item | Why deferred |
|------|-------------|
| CLI tool (`kumo crawl myspider.toml`) | Requires stable API + TOML-driven spider spec first |
| GraphQL extractor | Niche; adds heavy deps; revisit after REST extractors mature |
| Parquet / Arrow exporter | Niche for a scraping library; users can convert from SQLite |
| Plugin / extension system | Premature; design space not clear until more built-ins exist |
| Benchmark suite | Useful but not user-facing; add when perf regressions appear |
