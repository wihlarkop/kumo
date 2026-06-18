# Browser Fetcher

The `browser` feature uses headless Chromium (via `chromiumoxide`) to fetch pages. This renders JavaScript and executes client-side code before `parse()` receives the response — needed for React, Vue, Angular, or any site that builds its content in the browser.

## Installation

```toml
kumo = { version = "0.2", features = ["browser"] }
```

Chrome or Chromium must be available on the machine running the crawler. Use `BrowserConfig::executable(...)` when you need to point kumo at a specific binary.

## Basic Usage

```rust
use kumo::prelude::*;

CrawlEngine::builder()
    .browser(BrowserConfig::headless())  // use default headless Chromium
    .run(MySpider)
    .await?;
```

`parse()` receives the fully-rendered HTML — `res.css()` works on JS-generated content.

## HTTP-First Browser Fallback

For production crawls, prefer HTTP first and only pay the browser cost when a
page looks JavaScript-gated:

```rust
use kumo::prelude::*;

CrawlEngine::builder()
    .concurrency(16)
    .browser_fallback(BrowserConfig::headless())
    .run(MySpider)
    .await?;
```

`browser_fallback(...)` fetches each request with the normal HTTP fetcher first.
When the HTTP response looks empty, contains an empty `#root` or `#app` mount
with scripts, or says JavaScript is required, Kumo retries that same request
through the browser fetcher and passes the rendered response to `parse()`.

If the browser retry fails, Kumo keeps the original HTTP response and records a
fallback failure counter instead of failing the whole request. This keeps the
fallback path useful for mixed static and JavaScript-heavy sites.

Use `browser_fallback_on(...)` when you need a custom detector:

```rust
let fallback = BrowserFallbackConfig::new(BrowserConfig::headless())
    .on_response(|response| {
        response.status() == 200
            && response
                .text()
                .is_some_and(|body| body.contains("window.__NUXT__"))
    });

CrawlEngine::builder()
    .browser_fallback_on(fallback)
    .run(MySpider)
    .await?;
```

`CrawlReport` exposes `browser_fallbacks`,
`browser_fallback_successes`, and `browser_fallback_failures` so production
runs can alert when many pages require browser rendering.

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
    .browser(BrowserConfig::headless())
    .run(MySpider)
    .await?;
```

## When to Use the Browser

Use the browser fetcher when:

- The page content is built by JavaScript (SPA)
- The site requires login via JavaScript forms
- You need to interact with the page (click, scroll, fill forms)

Use plain HTTP (default) for everything else — it is 10–100× faster.

