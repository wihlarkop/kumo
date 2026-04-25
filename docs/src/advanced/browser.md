# Browser Fetcher

The `browser` feature uses headless Chromium (via `chromiumoxide`) to fetch pages. This renders JavaScript and executes client-side code before `parse()` receives the response — needed for React, Vue, Angular, or any site that builds its content in the browser.

## Installation

```toml
kumo = { version = "0.1", features = ["browser"] }
```

Chromium is downloaded automatically on first build via `chromiumoxide`.

## Basic Usage

```rust
use kumo::prelude::*;

CrawlEngine::builder()
    .browser(BrowserConfig::headless())  // use default headless Chromium
    .run(MySpider)
    .await?;
```

`parse()` receives the fully-rendered HTML — `res.css()` works on JS-generated content.

## BrowserConfig

```rust
// Headless (production)
BrowserConfig::headless()
    .viewport(1920, 1080)            // set viewport size
    .user_agent("Mozilla/5.0 ...")   // override User-Agent
    .stealth()                       // enable JS stealth patches (requires stealth feature)
    .timeout(Duration::from_secs(45))

// Headed (debugging — shows the browser window)
BrowserConfig::headed()
    .wait_for_selector(".content")   // wait for element before reading page
```

## Performance Considerations

The browser fetcher is significantly slower than HTTP fetching:

- Each page opens a new Chromium tab
- JS execution adds 1–5s per page
- Memory usage is ~100MB per concurrent tab

Reduce concurrency for browser crawls:

```rust
CrawlEngine::builder()
    .concurrency(3)   // don't open too many tabs at once
    .browser(BrowserConfig::new())
    .run(MySpider)
    .await?;
```

## When to Use the Browser

Use the browser fetcher when:

- The page content is built by JavaScript (SPA)
- The site requires login via JavaScript forms
- You need to interact with the page (click, scroll, fill forms)

Use plain HTTP (default) for everything else — it is 10–100× faster.
