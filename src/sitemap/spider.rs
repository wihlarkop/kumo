type UrlFilter = Box<dyn Fn(&str) -> bool + Send + Sync>;

/// A spider that discovers URLs from a sitemap and crawls each one.
///
/// Fetches `/sitemap.xml` by default and supports:
/// - Standard urlset sitemaps - emits [`SitemapEntry`] items per URL
/// - Sitemap index files - follows child sitemaps automatically
/// - Robots.txt autodiscovery via [`SitemapSpider::from_robots`]
///
/// [`SitemapEntry`]: crate::sitemap::SitemapEntry
///
/// # Example
/// ```rust,ignore
/// // Crawl sitemap.xml - emits SitemapEntry items with metadata
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
    pub(super) sitemap_url: String,
    pub(super) filter_url: Option<UrlFilter>,
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
    pub fn with_sitemap(sitemap_url: impl Into<String>) -> Self {
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
}
