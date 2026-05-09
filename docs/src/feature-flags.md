# Feature Flags

All optional capabilities in kumo are behind feature flags, so you only compile what you need.

## Reference Table

| Flag | Pulls in | Purpose |
|---|---|---|
| _(default)_ | — | CSS + regex selectors, all file stores, all middleware, HTTP cache, link extractor |
| `derive` | `kumo-derive` | `#[derive(Extract)]` for zero-boilerplate CSS extraction |
| `jsonpath` | `jsonpath-rust` | JSONPath selector on `Response` |
| `xpath` | `sxd-xpath` | XPath selector on `Response` |
| `browser` | `chromiumoxide` | Headless Chromium fetcher for JS-rendered pages |
| `stealth` | `rquest`, `rquest-util` | TLS/HTTP2 fingerprint spoofing + browser stealth patches¹ |
| `claude` | `rig-core` | `AnthropicClient` for LLM extraction |
| `openai` | `rig-core` | `OpenAiClient` for LLM extraction |
| `gemini` | `rig-core` | `GeminiClient` for LLM extraction |
| `ollama` | `rig-core` | `OllamaClient` for local LLM extraction |
| `llm` | `rig-core`, `schemars` | Base LLM traits (implied by all provider flags) |
| `postgres` | `sqlx` | `PostgresStore` |
| `sqlite` | `sqlx` | `SqliteStore` |
| `mysql` | `sqlx` | `MySqlStore` |
| `cloud` | `object_store` | `CloudStore` — backend-agnostic cloud storage (LocalFileSystem + InMemory included) |
| `cloud-s3` | `object_store/aws` | Adds Amazon S3 backend support to `CloudStore` |
| `cloud-gcs` | `object_store/gcp` | Adds Google Cloud Storage backend support to `CloudStore` |
| `cloud-azure` | `object_store/azure` | Adds Azure Blob Storage backend support to `CloudStore` |
| `persistence` | — | `FileFrontier` — file-backed URL frontier that survives restarts |
| `redis-frontier` | `redis` | `RedisFrontier` — distributed URL frontier via Redis |
| `otel` | `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `tracing-opentelemetry` | OTLP/gRPC export of all spans and events |

> ¹ The `stealth` feature compiles BoringSSL from source. It requires **cmake** and **nasm** on the build machine. See [Stealth Mode](advanced/stealth.md) for setup instructions.

## Common Combinations

### Data science / scripting

```toml
kumo = { version = "0.1", features = ["sqlite", "derive"] }
```

### Production crawl

```toml
kumo = { version = "0.1", features = ["postgres", "redis-frontier", "otel"] }
```

### Cloud storage

```toml
# Write to S3
kumo = { version = "0.1", features = ["cloud-s3"] }

# Write to GCS
kumo = { version = "0.1", features = ["cloud-gcs"] }

# Write to Azure Blob
kumo = { version = "0.1", features = ["cloud-azure"] }
```

### LLM extraction

```toml
# Cloud provider
kumo = { version = "0.1", features = ["claude"] }

# Local model
kumo = { version = "0.1", features = ["ollama"] }
```

### Full-stack

```toml
kumo = { version = "0.1", features = [
    "derive", "xpath", "jsonpath",
    "browser", "stealth",
    "claude",
    "postgres", "redis-frontier",
    "cloud-s3",
    "otel",
] }
```
