use kumo::prelude::*;
use reqwest::header::{HeaderName, HeaderValue};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct Page {
    url: String,
}

struct RequestSchedulingSpider;

#[async_trait::async_trait]
impl Spider for RequestSchedulingSpider {
    type Item = Page;

    fn name(&self) -> &str {
        "request-scheduling"
    }

    fn start_urls(&self) -> Vec<String> {
        vec!["https://example.com".into()]
    }

    async fn parse(&self, response: &Response) -> Result<Output<Self::Item>, KumoError> {
        let api_request = CrawlRequest::post(
            response.urljoin("/api/search"),
            br#"{"q":"rust scraping"}"#.to_vec(),
        )
        .header(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        )
        .priority(10)
        .meta("source", "search");

        Ok(Output::new()
            .item(Page {
                url: response.url().to_string(),
            })
            .request(api_request)
            .follow(response.urljoin("/next-page")))
    }
}

#[tokio::main]
async fn main() -> Result<(), KumoError> {
    CrawlEngine::builder()
        .respect_robots_txt(false)
        .run(RequestSchedulingSpider)
        .await?;
    Ok(())
}
