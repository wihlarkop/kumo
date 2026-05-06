use crate::extract::{
    response::Response,
    selector::{Element, ElementList},
};

impl Response {
    /// Select elements in this page via a CSS selector.
    /// Returns an empty list if the body is binary.
    pub fn css(&self, selector: &str) -> ElementList {
        let Some(text) = self.text() else {
            return ElementList { elements: vec![] };
        };
        let Some(sel) = crate::extract::selector::get_selector(selector) else {
            return ElementList { elements: vec![] };
        };
        let document = scraper::Html::parse_document(text);
        let elements = document
            .select(&sel)
            .map(|el| Element {
                outer_html: el.html(),
            })
            .collect();
        ElementList { elements }
    }
}
