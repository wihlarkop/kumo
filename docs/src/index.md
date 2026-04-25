---
description: kumo is an async web crawling framework for Rust — type-safe spiders, CSS/XPath/LLM extraction, pluggable stores, and OpenTelemetry built in.
---

# kumo

[![CI](https://github.com/wihlarkop/kumo/actions/workflows/ci.yml/badge.svg)](https://github.com/wihlarkop/kumo/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/kumo.svg)](https://crates.io/crates/kumo)
[![docs.rs](https://docs.rs/kumo/badge.svg)](https://docs.rs/kumo)

**kumo** (蜘蛛/雲 — spider/cloud) is an async web crawling framework for Rust — **Scrapy for Rust**.

It gives you a trait-based, async-first API for writing spiders that scrape, follow links, and store structured data — with batteries included for production crawls.

## Why kumo?

| Feature | Scrapy (Python) | gocolly (Go) | **kumo (Rust)** |
|---------|----------------|--------------|-----------------|
| Async-first | ✅ (Twisted) | ✅ | ✅ (Tokio) |
| Type-safe items | ❌ (dicts) | ❌ | ✅ |
| CSS selectors | ✅ | ✅ | ✅ |
| XPath selectors | ✅ | ❌ | ✅ |
| LLM extraction | ❌ | ❌ | ✅ |
| Item Stream API | ❌ | ❌ | ✅ |
| OpenTelemetry | plugin | ❌ | ✅ |
| Stealth / TLS spoof | plugin | ❌ | ✅ |

## Quick Install

```toml
[dependencies]
kumo = "0.1"
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
```

## 30-Second Example

```rust
use kumo::prelude::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct Quote {
    text: String,
    author: String,
}

struct QuotesSpider;

#[async_trait::async_trait]
impl Spider for QuotesSpider {
    type Item = Quote;

    fn name(&self) -> &str { "quotes" }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://quotes.toscrape.com".into()]
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let quotes: Vec<Quote> = res.css(".quote").iter().map(|el| Quote {
            text:   el.css(".text").first().map(|e| e.text()).unwrap_or_default(),
            author: el.css(".author").first().map(|e| e.text()).unwrap_or_default(),
        }).collect();

        let next = res.css("li.next a").first()
            .and_then(|el| el.attr("href"))
            .map(|href| res.urljoin(&href));

        let mut output = Output::new().items(quotes);
        if let Some(url) = next { output = output.follow(url); }
        Ok(output)
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    CrawlEngine::builder()
        .concurrency(5)
        .middleware(DefaultHeaders::new().user_agent("kumo/0.1"))
        .store(JsonlStore::new("quotes.jsonl")?)
        .run(QuotesSpider)
        .await?;
    Ok(())
}
```

[Get started →](getting-started.md){ .md-button .md-button--primary }
[Feature flags →](feature-flags.md){ .md-button }
