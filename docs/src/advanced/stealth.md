# Stealth Mode

The `stealth` feature replaces the default `reqwest` HTTP client with `rquest`, which spoofs TLS fingerprints and HTTP/2 settings to mimic real browsers. Combined with `BrowserConfig::stealth()`, it also patches JavaScript APIs (navigator, plugins, webGL) that bot-detection systems probe.

!!! warning "Build requirements"
    The `stealth` feature compiles BoringSSL from source. You need **cmake** and **nasm** on your build machine:

    - Ubuntu/Debian: `sudo apt install cmake nasm`
    - macOS: `brew install cmake nasm`
    - Windows: install via [cmake.org](https://cmake.org/download/) and [nasm.us](https://www.nasm.us/)

## Installation

```toml
kumo = { version = "0.1", features = ["stealth"] }
```

## HTTP-Level Stealth

`StealthHttpFetcher` sends requests with a realistic TLS ClientHello and HTTP/2 SETTINGS frame:

```rust
use kumo::prelude::*;

CrawlEngine::builder()
    .stealth(StealthProfile::Chrome131)   // spoof Chrome 131 TLS/H2 fingerprint
    .run(MySpider)
    .await?;
```

Available profiles:

| Profile | Mimics |
|---------|--------|
| `StealthProfile::Chrome131` | Chrome 131 on Windows 10 |

## Browser-Level Stealth

When combined with the `browser` feature, `BrowserConfig::stealth()` also patches JavaScript APIs:

```toml
kumo = { version = "0.1", features = ["stealth", "browser"] }
```

```rust
CrawlEngine::builder()
    .browser(
        BrowserConfig::new()
            .stealth()       // patch navigator, plugins, webGL, etc.
    )
    .run(MySpider)
    .await?;
```

## When to Use Stealth

Use stealth when the target site:

- Returns 403 or CAPTCHA to requests with non-browser TLS fingerprints (common with Cloudflare, Akamai, PerimeterX)
- Checks `navigator.webdriver` or `navigator.plugins` in JavaScript

For most sites, standard HTTP with a realistic User-Agent header is sufficient. Stealth adds significant build time and is only needed for bot-detection-hardened sites.
