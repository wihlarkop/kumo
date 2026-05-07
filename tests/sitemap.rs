use kumo::{
    extract::Response,
    sitemap::SitemapSpider,
    spider::{Output, Spider},
};

#[test]
fn new_sets_default_sitemap_url() {
    let spider = SitemapSpider::new("https://example.com");
    assert_eq!(spider.start_urls(), vec!["https://example.com/sitemap.xml"]);
}

#[test]
fn new_trims_trailing_slash() {
    let spider = SitemapSpider::new("https://example.com/");
    assert_eq!(spider.start_urls(), vec!["https://example.com/sitemap.xml"]);
}

#[test]
fn from_robots_sets_robots_url() {
    let spider = SitemapSpider::from_robots("https://example.com");
    assert_eq!(spider.start_urls(), vec!["https://example.com/robots.txt"]);
}

#[tokio::test]
async fn sitemap_index_follows_child_sitemaps() {
    let xml = r#"<?xml version="1.0"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap><loc>https://example.com/sitemap-1.xml</loc></sitemap>
  <sitemap><loc>https://example.com/sitemap-2.xml</loc></sitemap>
</sitemapindex>"#;
    let response = Response::from_parts("https://example.com/sitemap.xml", 200, xml);
    let output = SitemapSpider::new("https://example.com")
        .parse(&response)
        .await
        .unwrap();
    assert_eq!(
        output.follow,
        vec![
            "https://example.com/sitemap-1.xml",
            "https://example.com/sitemap-2.xml",
        ]
    );
}

#[tokio::test]
async fn urlset_entries_are_enqueued() {
    let xml = r#"<?xml version="1.0"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/page1</loc></url>
  <url><loc>https://example.com/page2</loc></url>
</urlset>"#;
    let response = Response::from_parts("https://example.com/sitemap.xml", 200, xml);
    let output = SitemapSpider::new("https://example.com")
        .parse(&response)
        .await
        .unwrap();
    assert_eq!(
        output.follow,
        vec!["https://example.com/page1", "https://example.com/page2"]
    );
}

#[tokio::test]
async fn filter_url_limits_urlset_follow_links() {
    let xml = r#"<?xml version="1.0"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/blog/1</loc></url>
  <url><loc>https://example.com/about</loc></url>
</urlset>"#;
    let response = Response::from_parts("https://example.com/sitemap.xml", 200, xml);
    let output = SitemapSpider::new("https://example.com")
        .filter_url(|url| url.contains("/blog/"))
        .parse(&response)
        .await
        .unwrap();
    assert_eq!(output.follow, vec!["https://example.com/blog/1"]);
}

#[tokio::test]
async fn robots_body_follows_sitemap_directives() {
    let robots = "User-agent: *\nDisallow:\nSitemap: https://example.com/sitemap.xml\n";
    let response = Response::from_parts("https://example.com/robots.txt", 200, robots);
    let output: Output<_> = SitemapSpider::from_robots("https://example.com")
        .parse(&response)
        .await
        .unwrap();
    assert_eq!(output.follow, vec!["https://example.com/sitemap.xml"]);
}
