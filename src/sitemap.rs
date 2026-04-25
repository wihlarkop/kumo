use serde::Serialize;

use crate::{
    error::KumoError,
    extract::Response,
    spider::{Output, Spider},
};

/// A single URL entry from a sitemap urlset.
#[derive(Debug, Clone, Serialize)]
pub struct SitemapEntry {
    pub loc: String,
    pub lastmod: Option<String>,
    pub changefreq: Option<String>,
    pub priority: Option<f32>,
}

/// A spider that discovers URLs from a sitemap and crawls each one.
///
/// Fetches `/sitemap.xml` by default and supports:
/// - Standard urlset sitemaps — emits [`SitemapEntry`] items per URL
/// - Sitemap index files — follows child sitemaps automatically
/// - Robots.txt autodiscovery via [`SitemapSpider::from_robots`]
///
/// # Example
/// ```rust,ignore
/// // Crawl sitemap.xml — emits SitemapEntry items with metadata
/// CrawlEngine::builder()
///     .run(SitemapSpider::new("https://example.com"))
///     .await?;
///
/// // Discover sitemaps from robots.txt first
/// CrawlEngine::builder()
///     .run(SitemapSpider::from_robots("https://example.com"))
///     .await?;
///
/// // Only follow blog URLs
/// CrawlEngine::builder()
///     .run(
///         SitemapSpider::new("https://example.com")
///             .filter_url(|url| url.contains("/blog/")),
///     )
///     .await?;
/// ```
pub struct SitemapSpider {
    sitemap_url: String,
    filter_url: Option<Box<dyn Fn(&str) -> bool + Send + Sync>>,
}

impl SitemapSpider {
    /// Create a spider that fetches `{base_url}/sitemap.xml`.
    pub fn new(base_url: impl Into<String>) -> Self {
        let base = base_url.into();
        let sitemap = format!("{}/sitemap.xml", base.trim_end_matches('/'));
        Self {
            sitemap_url: sitemap,
            filter_url: None,
        }
    }

    /// Create a spider with a custom sitemap URL.
    pub fn with_sitemap(_base_url: impl Into<String>, sitemap_url: impl Into<String>) -> Self {
        Self {
            sitemap_url: sitemap_url.into(),
            filter_url: None,
        }
    }

    /// Discover sitemaps from `{base_url}/robots.txt` first.
    ///
    /// Fetches robots.txt, extracts all `Sitemap:` directives,
    /// and follows them as sitemaps. Falls back gracefully if
    /// no `Sitemap:` directive is present.
    pub fn from_robots(base_url: impl Into<String>) -> Self {
        let base = base_url.into();
        let robots_url = format!("{}/robots.txt", base.trim_end_matches('/'));
        Self {
            sitemap_url: robots_url,
            filter_url: None,
        }
    }

    /// Only enqueue URLs for which `f` returns `true`.
    /// Applied to every `<loc>` discovered in urlset sitemaps.
    pub fn filter_url(mut self, f: impl Fn(&str) -> bool + Send + Sync + 'static) -> Self {
        self.filter_url = Some(Box::new(f));
        self
    }

    /// Extract all `<loc>` URLs from any sitemap body (urlset or sitemapindex).
    fn extract_locs(body: &str) -> Vec<String> {
        let re = regex::Regex::new(r"<loc>\s*(https?://[^\s<]+)\s*</loc>").unwrap();
        re.captures_iter(body).map(|c| c[1].to_string()).collect()
    }

    /// Parse a urlset sitemap into full `SitemapEntry` records.
    /// Returns an empty Vec for sitemapindex documents.
    pub(crate) fn parse_urlset_entries(body: &str) -> Vec<SitemapEntry> {
        if body.contains("<sitemapindex") {
            return vec![];
        }
        let url_re = regex::Regex::new(r"(?s)<url>(.*?)</url>").unwrap();
        let loc_re = regex::Regex::new(r"<loc>\s*(https?://[^\s<]+)\s*</loc>").unwrap();
        let lastmod_re = regex::Regex::new(r"<lastmod>\s*([^\s<]+)\s*</lastmod>").unwrap();
        let priority_re = regex::Regex::new(r"<priority>\s*([0-9.]+)\s*</priority>").unwrap();
        let changefreq_re =
            regex::Regex::new(r"<changefreq>\s*([^\s<]+)\s*</changefreq>").unwrap();

        url_re
            .captures_iter(body)
            .filter_map(|cap| {
                let block = cap.get(1)?.as_str();
                let loc = loc_re.captures(block)?.get(1)?.as_str().to_string();
                Some(SitemapEntry {
                    loc,
                    lastmod: lastmod_re
                        .captures(block)
                        .and_then(|c| c.get(1))
                        .map(|m| m.as_str().to_string()),
                    changefreq: changefreq_re
                        .captures(block)
                        .and_then(|c| c.get(1))
                        .map(|m| m.as_str().to_string()),
                    priority: priority_re
                        .captures(block)
                        .and_then(|c| c.get(1))
                        .and_then(|m| m.as_str().parse().ok()),
                })
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl Spider for SitemapSpider {
    type Item = SitemapEntry;

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

        let mut output = Output::new();

        // robots.txt autodiscovery: extract Sitemap: directives and follow them.
        if body
            .lines()
            .any(|l| l.starts_with("User-agent:") || l.starts_with("Sitemap:"))
        {
            for line in body.lines() {
                let trimmed = line.trim();
                if let Some(url) = trimmed
                    .strip_prefix("Sitemap:")
                    .or_else(|| trimmed.strip_prefix("sitemap:"))
                {
                    let url = url.trim().to_string();
                    if !url.is_empty() {
                        output = output.follow(url);
                    }
                }
            }
            return Ok(output);
        }

        if body.contains("<sitemapindex") {
            // Sitemap index: follow child sitemaps.
            for url in Self::extract_locs(body) {
                output = output.follow(url);
            }
        } else {
            // Standard urlset: emit entries and enqueue each loc for crawling.
            for entry in Self::parse_urlset_entries(body) {
                let passes = self
                    .filter_url
                    .as_ref()
                    .map(|f| f(&entry.loc))
                    .unwrap_or(true);
                if passes && entry.loc != self.sitemap_url {
                    output = output.follow(entry.loc.clone()).item(entry);
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
        assert_eq!(
            locs,
            vec!["https://example.com/page1", "https://example.com/page2"]
        );
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

    #[test]
    fn from_robots_sets_robots_url() {
        let spider = SitemapSpider::from_robots("https://example.com");
        assert_eq!(spider.sitemap_url, "https://example.com/robots.txt");
    }

    #[test]
    fn parse_urlset_entries_extracts_full_metadata() {
        let xml = r#"<?xml version="1.0"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>https://example.com/page1</loc>
    <lastmod>2024-01-15</lastmod>
    <changefreq>weekly</changefreq>
    <priority>0.8</priority>
  </url>
  <url>
    <loc>https://example.com/page2</loc>
  </url>
</urlset>"#;
        let entries = SitemapSpider::parse_urlset_entries(xml);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].loc, "https://example.com/page1");
        assert_eq!(entries[0].lastmod.as_deref(), Some("2024-01-15"));
        assert_eq!(entries[0].changefreq.as_deref(), Some("weekly"));
        assert!((entries[0].priority.unwrap() - 0.8).abs() < 0.001);
        assert_eq!(entries[1].loc, "https://example.com/page2");
        assert!(entries[1].lastmod.is_none());
        assert!(entries[1].priority.is_none());
    }

    #[test]
    fn parse_urlset_entries_empty_on_sitemapindex() {
        let xml = r#"<sitemapindex><sitemap><loc>https://example.com/s.xml</loc></sitemap></sitemapindex>"#;
        let entries = SitemapSpider::parse_urlset_entries(xml);
        assert!(entries.is_empty());
    }
}
