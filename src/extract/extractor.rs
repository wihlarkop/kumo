use crate::error::KumoError;

/// A single node extracted from an HTML document.
///
/// Kept separate from `Element` to allow future extractors (XPath, LLM)
/// to return richer data without touching the CSS-specific types.
pub struct ExtractedNode {
    pub outer_html: String,
}

impl ExtractedNode {
    /// Get the concatenated text content of this node and all its descendants.
    pub fn text(&self) -> String {
        let fragment = scraper::Html::parse_fragment(&self.outer_html);
        fragment.root_element().text().collect::<Vec<_>>().join("")
    }

    /// Get the value of an attribute by name.
    pub fn attr(&self, name: &str) -> Option<String> {
        let fragment = scraper::Html::parse_fragment(&self.outer_html);
        let sel = scraper::Selector::parse("*").unwrap();
        fragment
            .select(&sel)
            .find(|el| !matches!(el.value().name(), "html" | "body"))
            .and_then(|el| el.value().attr(name))
            .map(String::from)
    }
}

/// Extension point for pluggable extraction strategies.
///
/// The default implementation (`CssExtractor`) uses CSS selectors via `scraper`.
/// Future crates can implement XPath or LLM-based extraction without touching
/// core kumo code.
pub trait Extractor: Send + Sync {
    fn extract(
        &self,
        html: &str,
        selector: &str,
    ) -> Result<Vec<ExtractedNode>, KumoError>;
}

/// Default CSS-selector extractor backed by the `scraper` crate.
pub struct CssExtractor;

impl Extractor for CssExtractor {
    fn extract(
        &self,
        html: &str,
        selector: &str,
    ) -> Result<Vec<ExtractedNode>, KumoError> {
        let document = scraper::Html::parse_document(html);
        let sel = scraper::Selector::parse(selector)
            .map_err(|e| KumoError::Parse(format!("invalid CSS selector '{}': {:?}", selector, e)))?;
        let nodes = document
            .select(&sel)
            .map(|el| ExtractedNode { outer_html: el.html() })
            .collect();
        Ok(nodes)
    }
}
