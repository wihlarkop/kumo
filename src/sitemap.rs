use crate::{
    error::KumoError,
    extract::Response,
    spider::{Output, Spider},
};

/// A spider that discovers URLs from a sitemap and crawls each one.
///
/// Fetches `/sitemap.xml` by default and supports:
/// - Standard urlset sitemaps (`<url><loc>…</loc></url>`)
/// - Sitemap index files (`<sitemap><loc>…</loc></sitemap>`) — the child
///   sitemaps are followed automatically via the engine's normal queue.
///
/// # Example
/// ```rust,ignore
/// CrawlEngine::builder()
///     .run(SitemapSpider::new("https://example.com"))
///     .await?;
/// ```
pub struct SitemapSpider {
    sitemap_url: String,
}

impl SitemapSpider {
    /// Create a spider that fetches `{base_url}/sitemap.xml`.
    pub fn new(base_url: impl Into<String>) -> Self {
        let base = base_url.into();
        let sitemap = format!("{}/sitemap.xml", base.trim_end_matches('/'));
        Self { sitemap_url: sitemap }
    }

    /// Create a spider with a custom sitemap URL.
    pub fn with_sitemap(_base_url: impl Into<String>, sitemap_url: impl Into<String>) -> Self {
        Self { sitemap_url: sitemap_url.into() }
    }

    fn extract_locs(body: &str) -> Vec<String> {
        let re = regex::Regex::new(r"<loc>\s*(https?://[^\s<]+)\s*</loc>").unwrap();
        re.captures_iter(body).map(|c| c[1].to_string()).collect()
    }
}

#[async_trait::async_trait]
impl Spider for SitemapSpider {
    type Item = serde_json::Value;

    fn name(&self) -> &str {
        "sitemap"
    }

    fn start_urls(&self) -> Vec<String> {
        vec![self.sitemap_url.clone()]
    }

    async fn parse(&self, response: &Response) -> Result<Output<Self::Item>, KumoError> {
        let Some(body) = response.text() else {
            return Ok(Output::new());
        };

        let urls = Self::extract_locs(body);
        let mut output = Output::new();

        if body.contains("<sitemapindex") {
            // Sitemap index: follow the child sitemaps, which will be parsed as sitemaps too.
            for url in urls {
                output = output.follow(url);
            }
        } else {
            // Standard urlset: enqueue every <loc> for crawling.
            for url in urls {
                // Skip the sitemap URL itself to avoid re-parsing it.
                if url != self.sitemap_url {
                    output = output.follow(url);
                }
            }
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_locs_from_urlset() {
        let xml = r#"<?xml version="1.0"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/page1</loc></url>
  <url><loc>https://example.com/page2</loc></url>
</urlset>"#;
        let locs = SitemapSpider::extract_locs(xml);
        assert_eq!(locs, vec!["https://example.com/page1", "https://example.com/page2"]);
    }

    #[test]
    fn extract_locs_from_index() {
        let xml = r#"<?xml version="1.0"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap><loc>https://example.com/sitemap-1.xml</loc></sitemap>
  <sitemap><loc>https://example.com/sitemap-2.xml</loc></sitemap>
</sitemapindex>"#;
        let locs = SitemapSpider::extract_locs(xml);
        assert_eq!(locs.len(), 2);
        assert!(locs[0].contains("sitemap-1"));
    }

    #[test]
    fn new_sets_default_sitemap_url() {
        let spider = SitemapSpider::new("https://example.com");
        assert_eq!(spider.sitemap_url, "https://example.com/sitemap.xml");
    }

    #[test]
    fn new_trims_trailing_slash() {
        let spider = SitemapSpider::new("https://example.com/");
        assert_eq!(spider.sitemap_url, "https://example.com/sitemap.xml");
    }
}
