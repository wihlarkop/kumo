//! Demonstrates the stealth HTTP fetcher with TLS fingerprint spoofing.
//!
//! Fetches https://tls.browserleaks.com/json and prints the JA3/JA4 fingerprint.
//! When stealth is working correctly, the fingerprint will match Chrome 131, not
//! the default reqwest fingerprint.
//!
//! Run with:
//!   cargo run --example stealth --features stealth
//!
//! NOTE: The `stealth` feature requires cmake and NASM build tools for BoringSSL.
//! On Ubuntu/Debian: apt install cmake nasm
//! On macOS: brew install cmake nasm
//! On Windows: install CMake and NASM manually, or use WSL.

use kumo::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct TlsInfo {
    ja3: Option<String>,
    ja3_hash: Option<String>,
    ja4: Option<String>,
    user_agent: Option<String>,
}

struct StealthSpider;

#[async_trait::async_trait]
impl Spider for StealthSpider {
    type Item = TlsInfo;

    fn name(&self) -> &str {
        "stealth-test"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://tls.browserleaks.com/json".into()]
    }

    async fn parse(&self, res: &Response) -> Result<Output<Self::Item>, KumoError> {
        let info: TlsInfo =
            serde_json::from_str(res.text().unwrap_or("{}")).unwrap_or_else(|_| TlsInfo {
                ja3: None,
                ja3_hash: None,
                ja4: None,
                user_agent: None,
            });

        println!("JA3 hash: {}", info.ja3_hash.as_deref().unwrap_or("(none)"));
        println!("JA4:      {}", info.ja4.as_deref().unwrap_or("(none)"));
        println!(
            "UA:       {}",
            info.user_agent.as_deref().unwrap_or("(none)")
        );

        Ok(Output::new().item(info))
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    tracing_subscriber::fmt()
        .with_env_filter("kumo=info")
        .init();

    CrawlEngine::builder()
        .stealth(StealthProfile::Chrome131)
        .concurrency(1)
        .store(StdoutStore)
        .run(StealthSpider)
        .await?;

    Ok(())
}
