use super::{SitemapEntry, SitemapSpider};
use crate::{
    error::KumoError,
    extract::Response,
    spider::{Output, Spider},
};

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

        if is_robots_txt(body) {
            for url in sitemap_urls_from_robots(body) {
                output = output.follow(url);
            }
            return Ok(output);
        }

        if body.contains("<sitemapindex") {
            for url in Self::extract_locs(body) {
                output = output.follow(url);
            }
        } else {
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

fn is_robots_txt(body: &str) -> bool {
    body.lines()
        .any(|l| l.starts_with("User-agent:") || l.starts_with("Sitemap:"))
}

fn sitemap_urls_from_robots(body: &str) -> impl Iterator<Item = String> + '_ {
    body.lines().filter_map(|line| {
        let trimmed = line.trim();
        let url = trimmed
            .strip_prefix("Sitemap:")
            .or_else(|| trimmed.strip_prefix("sitemap:"))?
            .trim();
        (!url.is_empty()).then(|| url.to_string())
    })
}
