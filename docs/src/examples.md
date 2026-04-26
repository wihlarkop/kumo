---
description: Runnable kumo examples covering basic spiders, selectors, LLM extraction, browser mode, stealth, stores, and more.
---

# Examples

All examples live in the [`examples/`](https://github.com/wihlarkop/kumo/tree/main/examples) folder. Run any of them with `cargo run --example <name>`.

## Basic Spiders

### `quotes.rs` — minimal spider

Scrapes all quotes from [quotes.toscrape.com](https://quotes.toscrape.com), following pagination. The simplest possible kumo spider — CSS selectors and `JsonlStore`.

```bash
cargo run --example quotes
```

### `books.rs` — rate limiting + retry

Scrapes all 1000 books from [books.toscrape.com](https://books.toscrape.com) across 50 pages. Demonstrates `RateLimiter`, exponential retry, `allowed_domains`, `max_depth`, and `JsonStore`.

```bash
cargo run --example books
```

### `books_derive.rs` — `#[derive(Extract)]`

Same as `books.rs` but uses `#[derive(Extract)]` with field annotations instead of manual CSS selectors.

```bash
cargo run --example books_derive --features derive
```

### `multi_spider.rs` — multiple spiders

Runs two independent spiders (quotes + books) concurrently in a single engine using `.add_spider()` / `.run_all()`.

```bash
cargo run --example multi_spider
```

## Selectors

### `selectors.rs` — CSS, regex, JSONPath

Demonstrates CSS, regex, and JSONPath selectors against local HTML and JSON — no network required.

```bash
# CSS + regex
cargo run --example selectors

# CSS + regex + JSONPath
cargo run --example selectors --features jsonpath
```

### `xpath.rs` — XPath selectors

Demonstrates XPath selectors on an HTML response using the `xpath` feature.

```bash
cargo run --example xpath --features xpath
```

## Middleware

### `autothrottle.rs` — adaptive throttling

Shows `AutoThrottle` middleware adapting request delay based on server latency and 429/503 responses.

```bash
cargo run --example autothrottle
```

### `proxy_rotation.rs` — proxy rotation

Demonstrates `ProxyRotator` middleware cycling through a list of proxy URLs.

```bash
cargo run --example proxy_rotation
```

## Stores

### `sqlite.rs` — SQLite store

Stores scraped items into a local SQLite file.

```bash
cargo run --example sqlite --features sqlite
```

### `postgres.rs` — PostgreSQL store

Stores scraped items into PostgreSQL. Requires a running Postgres instance.

```bash
cargo run --example postgres --features postgres
```

### `cloud.rs` — Cloud storage (S3 / GCS / Azure / local)

Stores scraped items as JSONL via the backend-agnostic `CloudStore`. The example uses `LocalFileSystem` — no cloud credentials needed. Swap the backend for `AmazonS3`, `GoogleCloudStorage`, or `MicrosoftAzure` with no other code changes.

```bash
cargo run --example cloud --features cloud
```

## LLM Extraction

### `llm_extract.rs` — LLM extraction

Scrapes [quotes.toscrape.com](https://quotes.toscrape.com) without any CSS selectors — the LLM reads the HTML and fills in the struct automatically.

```bash
ANTHROPIC_API_KEY=sk-ant-... cargo run --example llm_extract --features claude
```

Swap the feature flag and client to use a different provider:

| Provider | Flag | Client |
|---|---|---|
| Anthropic Claude | `claude` | `AnthropicClient` |
| OpenAI | `openai` | `OpenAiClient` |
| Google Gemini | `gemini` | `GeminiClient` |
| Ollama (local) | `ollama` | `OllamaClient` |

### `llm_fallback.rs` — CSS + LLM fallback

Uses `#[extract(llm_fallback = "hint")]` — tries CSS first and falls back to the LLM only when the selector returns nothing.

```bash
ANTHROPIC_API_KEY=sk-ant-... cargo run --example llm_fallback --features claude,derive
```

## Advanced

### `http_cache.rs` — HTTP response cache

Demonstrates disk-backed response caching. Run once to populate the cache, run again to see instant responses from disk.

```bash
cargo run --example http_cache
```

### `link_extractor.rs` — link extraction with filtering

Demonstrates `LinkExtractor` with `allow_domains`, `allow`, `deny`, `restrict_css`, and `canonicalize`.

```bash
cargo run --example link_extractor
```

### `browser.rs` — headless browser

Fetches a JS-rendered page using headless Chromium. Requires the `browser` feature.

```bash
cargo run --example browser --features browser
```

### `stealth.rs` — stealth mode

Sends requests with a Chrome 131 TLS fingerprint using the `stealth` feature. Requires cmake and nasm.

```bash
cargo run --example stealth --features stealth
```
