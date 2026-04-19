/// Demonstrates proxy rotation and User-Agent rotation working together.
///
/// Hits httpbin.org/anything repeatedly so you can see the IP and User-Agent
/// each request uses. Plug in real proxy URLs to observe IP rotation.
///
/// Run with:
///     cargo run --example proxy_rotation
use kumo::prelude::*;
use std::time::Duration;

struct HttpbinSpider {
    urls: Vec<String>,
}

#[async_trait::async_trait]
impl Spider for HttpbinSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "httpbin"
    }

    fn start_urls(&self) -> Vec<String> {
        self.urls.clone()
    }

    async fn parse(&self, response: &Response) -> Result<Output<Self::Item>, KumoError> {
        // httpbin returns a JSON body with the headers it received.
        if let Ok(json) = response.json::<serde_json::Value>() {
            let ua = json["headers"]["User-Agent"].as_str().unwrap_or("?");
            let origin = json["origin"].as_str().unwrap_or("?");
            println!("IP: {origin}  |  UA: {ua}");
            return Output::new().item(json);
        }
        Ok(Output::new())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("kumo=info")
        .init();

    // Repeat the same URL several times so we can observe rotation.
    let urls = vec!["https://httpbin.org/anything".to_string(); 5];

    // Replace these with real proxy URLs to test actual IP rotation.
    // Leaving the list empty means ProxyRotator is a no-op and the
    // default client is used — safe to run in CI without proxies.
    let proxies: Vec<String> = vec![
        // "http://user:pass@proxy1.example.com:8080".to_string(),
        // "http://proxy2.example.com:8080".to_string(),
    ];

    let mut engine = CrawlEngine::builder()
        .concurrency(2)
        .middleware(UserAgentRotator::common_browsers())
        .middleware(RateLimiter::per_second(2.0));

    if !proxies.is_empty() {
        engine = engine.middleware(ProxyRotator::new(proxies));
    }

    let stats = engine
        .crawl_delay(Duration::from_millis(500))
        .store(StdoutStore)
        .run(HttpbinSpider { urls })
        .await?;

    println!(
        "\nCrawled {} page(s) in {:.2}s",
        stats.pages_crawled,
        stats.duration.as_secs_f64()
    );

    Ok(())
}
