use super::builder::LinkExtractor;
use super::domain::host_matches_domain;
use crate::extract::Response;

impl LinkExtractor {
    /// Extract all links from `response`, resolve relative URLs, apply filters,
    /// and deduplicate. Returns absolute URLs in document order.
    pub fn extract(&self, response: &Response) -> Vec<String> {
        let Some(html) = response.text() else {
            return vec![];
        };
        let document = scraper::Html::parse_document(html);

        let selector_str = self
            .tags
            .iter()
            .flat_map(|tag| self.attrs.iter().map(move |attr| format!("{tag}[{attr}]")))
            .collect::<Vec<_>>()
            .join(",");
        let href_sel = scraper::Selector::parse(&selector_str).unwrap();

        let hrefs = if let Some(ref scoped) = self.scope_html(&document) {
            let fragment = scraper::Html::parse_fragment(scoped);
            self.extract_from_html(&fragment, &href_sel, response)
        } else {
            self.extract_from_html(&document, &href_sel, response)
        };

        let mut seen = std::collections::HashSet::new();
        hrefs
            .into_iter()
            .map(|url| self.canonicalize_url(url))
            .filter(|url| self.allowed_by_filters(url))
            .filter(|url| seen.insert(url.clone()))
            .collect()
    }

    fn scope_html(&self, document: &scraper::Html) -> Option<String> {
        self.restrict_css.as_deref().and_then(|css| {
            scraper::Selector::parse(css)
                .ok()
                .and_then(|sel| document.select(&sel).next().map(|el| el.html()))
        })
    }

    fn extract_from_html(
        &self,
        html: &scraper::Html,
        selector: &scraper::Selector,
        response: &Response,
    ) -> Vec<String> {
        html.select(selector)
            .filter_map(|el| {
                self.attrs
                    .iter()
                    .find_map(|attr| el.value().attr(attr.as_str()))
            })
            .map(|href| response.urljoin(href))
            .collect()
    }

    fn canonicalize_url(&self, url: String) -> String {
        if self.canonicalize {
            url.find('#').map(|i| url[..i].to_string()).unwrap_or(url)
        } else {
            url
        }
    }

    fn allowed_by_filters(&self, url: &str) -> bool {
        let allow_ok = self.allow.is_empty() && self.allow_domains.is_empty()
            || self.allow.iter().any(|r| r.is_match(url))
            || self
                .allow_domains
                .iter()
                .any(|d| host_matches_domain(url, d));
        if !allow_ok {
            return false;
        }

        !self.deny.iter().any(|r| r.is_match(url))
            && !self
                .deny_domains
                .iter()
                .any(|d| host_matches_domain(url, d))
    }
}
