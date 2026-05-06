mod entry;
mod parser;
mod runtime;
mod spider;

pub use entry::SitemapEntry;
pub use spider::SitemapSpider;

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
