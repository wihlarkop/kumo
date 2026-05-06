use serde::Serialize;

/// A single URL entry from a sitemap urlset.
#[derive(Debug, Clone, Serialize)]
pub struct SitemapEntry {
    pub loc: String,
    pub lastmod: Option<String>,
    pub changefreq: Option<String>,
    pub priority: Option<f32>,
}
