# Sitemap Spider

`SitemapSpider` reads `sitemap.xml` (and sitemap index files) and emits structured `SitemapEntry` items containing the URL, last-modification date, priority, and change frequency.

## Basic Usage

```rust
use kumo::prelude::*;

// Crawl a known sitemap URL
CrawlEngine::builder()
    .run(SitemapSpider::new("https://example.com/sitemap.xml"))
    .await?;
```

## Auto-Discovery from robots.txt

`from_robots()` reads `robots.txt` and extracts all `Sitemap:` directives:

```rust
CrawlEngine::builder()
    .run(SitemapSpider::from_robots("https://example.com"))
    .await?;
```

## URL Filtering

Only emit entries whose URL matches a predicate:

```rust
CrawlEngine::builder()
    .run(
        SitemapSpider::new("https://example.com/sitemap.xml")
            .filter_url(|url| url.contains("/blog/")),
    )
    .await?;
```

## SitemapEntry Fields

```rust
pub struct SitemapEntry {
    pub loc: String,                // the URL
    pub lastmod: Option<String>,    // ISO 8601 date
    pub priority: Option<f32>,      // 0.0–1.0
    pub changefreq: Option<String>, // "daily", "weekly", etc.
}
```

## Sitemap Index Files

Sitemap index files (`<sitemapindex>`) are followed automatically. kumo fetches each child sitemap and merges the entries.

## Storing Results

`SitemapEntry` implements `Serialize`, so it works with any store:

```rust
CrawlEngine::builder()
    .store(JsonlStore::new("urls.jsonl")?)
    .run(SitemapSpider::new("https://example.com/sitemap.xml"))
    .await?;
```
