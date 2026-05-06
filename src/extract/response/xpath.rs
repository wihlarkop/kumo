use crate::extract::response::Response;

impl Response {
    /// Evaluate an XPath 1.0 expression against the response body.
    ///
    /// Returns string values of all matched nodes:
    /// - **Element nodes** -> outer HTML serialization
    /// - **Text nodes** -> the text content (use `text()` axis in your expression)
    /// - **Attribute nodes** -> the attribute value (use `@attr` in your expression)
    ///
    /// Returns an empty `Vec` on invalid expressions, no matches, or binary bodies.
    ///
    /// # Note
    ///
    /// The underlying HTML parser auto-inserts `<tbody>` inside `<table>` elements.
    /// Use `//table/tbody/tr/td` instead of `//table/tr/td`.
    ///
    /// Requires the `xpath` feature flag.
    ///
    /// # Examples
    /// ```rust,ignore
    /// res.xpath("//h1/text()")                         // all h1 text
    /// res.xpath("//a/@href")                           // all href values
    /// res.xpath(r#"//div[@class="price"]/text()"#)     // filtered elements
    /// res.xpath("//item/title/text()")                 // RSS feed titles
    /// ```
    pub fn xpath(&self, expr: &str) -> Vec<String> {
        let Some(text) = self.text() else {
            return vec![];
        };

        let package = sxd_html::parse_html(text);
        let document = package.as_document();

        let value = match sxd_xpath::evaluate_xpath(&document, expr) {
            Ok(v) => v,
            Err(_) => return vec![],
        };

        match value {
            sxd_xpath::Value::Nodeset(nodeset) => nodeset
                .document_order()
                .into_iter()
                .filter_map(xpath_node_to_string)
                .collect(),
            sxd_xpath::Value::String(s) => vec![s],
            sxd_xpath::Value::Number(n) => vec![n.to_string()],
            sxd_xpath::Value::Boolean(b) => vec![b.to_string()],
        }
    }

    /// Return the first XPath match as a string, or `None`.
    /// Requires the `xpath` feature flag.
    pub fn xpath_first(&self, expr: &str) -> Option<String> {
        self.xpath(expr).into_iter().next()
    }
}

fn xpath_node_to_string(node: sxd_xpath::nodeset::Node<'_>) -> Option<String> {
    use sxd_xpath::nodeset::Node;
    match node {
        Node::Text(t) => Some(t.text().to_string()),
        Node::Attribute(a) => Some(a.value().to_string()),
        Node::Element(e) => Some(xpath_element_to_html(e)),
        Node::Root(_) | Node::Comment(_) | Node::ProcessingInstruction(_) | Node::Namespace(_) => {
            None
        }
    }
}

fn xpath_element_to_html(el: sxd_document::dom::Element<'_>) -> String {
    let name = el.name().local_part();
    let attrs: String = el
        .attributes()
        .iter()
        .map(|a| format!(r#" {}="{}""#, a.name().local_part(), a.value()))
        .collect();
    let children: String = el
        .children()
        .iter()
        .filter_map(xpath_child_to_html)
        .collect();
    format!("<{name}{attrs}>{children}</{name}>")
}

fn xpath_child_to_html(child: &sxd_document::dom::ChildOfElement<'_>) -> Option<String> {
    use sxd_document::dom::ChildOfElement;
    match child {
        ChildOfElement::Element(e) => Some(xpath_element_to_html(*e)),
        ChildOfElement::Text(t) => Some(t.text().to_string()),
        ChildOfElement::Comment(_) | ChildOfElement::ProcessingInstruction(_) => None,
    }
}
