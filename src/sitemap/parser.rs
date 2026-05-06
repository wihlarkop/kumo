use super::{SitemapEntry, SitemapSpider};

impl SitemapSpider {
    /// Extract all `<loc>` URLs from any sitemap body (urlset or sitemapindex).
    pub(super) fn extract_locs(body: &str) -> Vec<String> {
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
        let changefreq_re = regex::Regex::new(r"<changefreq>\s*([^\s<]+)\s*</changefreq>").unwrap();

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
