use std::sync::{Arc, OnceLock};

use ego_tree::NodeId;
use scraper::ElementRef;

use super::{ElementList, SharedDocument, get_selector, re_matches};

/// A single CSS-matched HTML element.
///
/// Keeps the parsed document alive so cloned elements remain independently usable.
#[derive(Clone)]
pub struct Element {
    document: SharedDocument,
    node_id: NodeId,
    outer_html: Arc<OnceLock<String>>,
}

impl std::fmt::Debug for Element {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Element")
            .field("outer_html", &self.outer_html())
            .finish()
    }
}

impl Element {
    pub(crate) fn new(document: SharedDocument, node_id: NodeId) -> Self {
        Self {
            document,
            node_id,
            outer_html: Arc::new(OnceLock::new()),
        }
    }

    fn with_element<R>(&self, operation: impl FnOnce(ElementRef<'_>) -> R) -> R {
        let document = self
            .document
            .lock()
            .expect("parsed document lock should not be poisoned");
        let element = document
            .tree
            .get(self.node_id)
            .and_then(ElementRef::wrap)
            .expect("Element node must remain valid while its document is alive");
        operation(element)
    }

    /// Get the concatenated text content of this element and all its descendants.
    pub fn text(&self) -> String {
        self.with_element(|element| element.text().collect())
    }

    /// Get the value of an attribute by name.
    pub fn attr(&self, name: &str) -> Option<String> {
        self.with_element(|element| element.attr(name).map(String::from))
    }

    /// Select child elements via a CSS selector.
    pub fn css(&self, selector: &str) -> ElementList {
        let Some(sel) = get_selector(selector) else {
            return ElementList { elements: vec![] };
        };
        let node_ids = self.with_element(|element| {
            element
                .select(&sel)
                .map(|child| child.id())
                .collect::<Vec<_>>()
        });
        let elements = node_ids
            .into_iter()
            .map(|node_id| Self::new(self.document.clone(), node_id))
            .collect();
        ElementList { elements }
    }

    /// Apply a regex pattern to this element's text content and return all matches.
    ///
    /// If the pattern contains capture group 1, returns group-1 matches.
    /// Otherwise returns the full match. Returns an empty Vec on invalid pattern.
    pub fn re(&self, pattern: &str) -> Vec<String> {
        re_matches(&self.text(), pattern)
    }

    /// Return the first regex match in this element's text, or `None`.
    pub fn re_first(&self, pattern: &str) -> Option<String> {
        self.re(pattern).into_iter().next()
    }

    /// Get the outer HTML of this element (the element itself and its children).
    pub fn outer_html(&self) -> &str {
        self.outer_html
            .get_or_init(|| self.with_element(|element| element.html()))
            .as_str()
    }

    /// Get the inner HTML of this element (children only, no outer tag).
    pub fn inner_html(&self) -> String {
        self.with_element(|element| element.inner_html())
    }
}
