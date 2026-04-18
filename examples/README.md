# Examples

## quotes.rs — basic spider

Scrapes all quotes from [quotes.toscrape.com](https://quotes.toscrape.com), following pagination. Demonstrates the minimal spider setup with CSS selectors and `JsonlStore`.

```bash
cargo run --example quotes
```

## books.rs — rate limiting + retry

Scrapes all 1000 books from [books.toscrape.com](https://books.toscrape.com) across 50 pages. Demonstrates `RateLimiter`, exponential retry, `allowed_domains`, `max_depth`, and `JsonStore`.

```bash
cargo run --example books
```

## autothrottle.rs — adaptive throttling

Shows `AutoThrottle` middleware which automatically adjusts request delay based on observed server latency and 429/503 responses.

```bash
cargo run --example autothrottle
```

## selectors.rs — CSS, regex, and JSONPath

Demonstrates all three selector types against local HTML and JSON — no network required.

```bash
# CSS + regex only
cargo run --example selectors

# CSS + regex + JSONPath
cargo run --example selectors --features jsonpath
```

## postgres.rs — PostgreSQL store

Stores scraped items into PostgreSQL, with custom table name and promoted columns. Requires a running Postgres instance.

```bash
cargo run --example postgres --features postgres
```

## sqlite.rs — SQLite store

Stores scraped items into a local SQLite file, with custom table name and promoted columns.

```bash
cargo run --example sqlite --features sqlite
```

## llm_extract.rs — LLM extraction

Scrapes [quotes.toscrape.com](https://quotes.toscrape.com) without any CSS selectors — the LLM reads the HTML and fills in the struct automatically. Requires an Anthropic API key.

```bash
ANTHROPIC_API_KEY=sk-ant-... cargo run --example llm_extract --features claude
```

To use a different provider, swap the feature flag and client:

| Provider | Feature flag | Client |
|---|---|---|
| Anthropic Claude | `claude` | `AnthropicClient` |
| OpenAI | `openai` | `OpenAiClient` |
| Google Gemini | `gemini` | `GeminiClient` |
| Ollama (local) | `ollama` | `OllamaClient` |
