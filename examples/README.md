# Examples

Runnable examples live in this directory. Run any example with:

```bash
cargo run --example <name>
```

## Basic Spiders

### `quotes.rs` - minimal spider

Scrapes quotes from [quotes.toscrape.com](https://quotes.toscrape.com), follows
pagination, and writes JSONL.

```bash
cargo run --example quotes
```

### `books.rs` - rate limiting + retry

Scrapes [books.toscrape.com](https://books.toscrape.com) with rate limiting,
retry, domain limits, depth limits, and JSON output.

```bash
cargo run --example books
```

### `books_derive.rs` - derive-based extraction

Uses `#[derive(Extract)]` instead of manual CSS selector code.

```bash
cargo run --example books_derive --features derive
```

### `multi_spider.rs` - multiple spiders

Runs multiple spiders concurrently with `.add_spider()` and `.run_all()`.

```bash
cargo run --example multi_spider
```

## Selectors

### `selectors.rs` - CSS, regex, and JSONPath

Demonstrates selectors against local HTML and JSON.

```bash
cargo run --example selectors
cargo run --example selectors --features jsonpath
```

### `xpath.rs` - XPath selectors

Demonstrates XPath extraction.

```bash
cargo run --example xpath --features xpath
```

## Middleware

### `autothrottle.rs` - adaptive throttling

Adjusts request delay based on latency and backoff status codes.

```bash
cargo run --example autothrottle
```

### `proxy_rotation.rs` - proxy rotation

Cycles through proxy URLs with `ProxyRotator`.

```bash
cargo run --example proxy_rotation
```

### `polite_crawling.rs` - polite crawl scheduling

Shows per-domain concurrency, per-domain delay, request priority, metadata,
fingerprint deduplication, and crawl stats.

```bash
cargo run --example polite_crawling
```

## Stores

### `sqlite.rs` - SQLite store

Stores scraped items in SQLite.

```bash
cargo run --example sqlite --features sqlite
```

### `postgres.rs` - PostgreSQL store

Stores scraped items in PostgreSQL. Requires a running Postgres instance.

```bash
cargo run --example postgres --features postgres
```

### `cloud.rs` - cloud storage

Writes JSONL through the backend-agnostic `CloudStore`. The example uses local
filesystem storage and can be adapted to S3, GCS, or Azure.

```bash
cargo run --example cloud --features cloud
```

## LLM Extraction

### `llm_extract.rs` - LLM extraction

Uses an LLM provider to fill a typed item from HTML.

```bash
ANTHROPIC_API_KEY=sk-ant-... cargo run --example llm_extract --features claude
```

### `llm_fallback.rs` - CSS + LLM fallback

Tries CSS extraction first and falls back to the LLM when selectors miss.

```bash
ANTHROPIC_API_KEY=sk-ant-... cargo run --example llm_fallback --features claude,derive
```

## Advanced

### `production_crawler.rs` - production crawl controls

Combines robots.txt, per-domain politeness, retries, persistent frontier state,
metrics, and JSONL output.

```bash
cargo run --example production_crawler --features persistence
```

### `http_cache.rs` - HTTP cache

Demonstrates disk-backed response caching.

```bash
cargo run --example http_cache
```

### `link_extractor.rs` - filtered link extraction

Demonstrates `LinkExtractor` allow/deny rules and canonicalization.

```bash
cargo run --example link_extractor
```

### `request_scheduling.rs` - request scheduling

Demonstrates custom request method/body, headers, priority, and metadata.

```bash
cargo run --example request_scheduling
```

### `browser.rs` - headless browser

Fetches a JavaScript-rendered page with Chromium.

```bash
cargo run --example browser --features browser
```

### `stealth.rs` - stealth mode

Sends requests with a browser-like TLS fingerprint. Requires cmake and nasm.

```bash
cargo run --example stealth --features stealth
```
