use std::sync::Arc;

use crate::error::KumoError;

/// A reusable, precompiled CSS selector for hot extraction loops.
#[derive(Clone, Debug)]
pub struct CssSelector {
    inner: Arc<scraper::Selector>,
}

impl CssSelector {
    /// Compile a CSS selector once for reuse across responses and elements.
    pub fn parse(selector: &str) -> Result<Self, KumoError> {
        scraper::Selector::parse(selector)
            .map(|inner| Self {
                inner: Arc::new(inner),
            })
            .map_err(|error| {
                KumoError::parse_msg(format!("invalid CSS selector `{selector}`: {error}"))
            })
    }

    pub(crate) fn from_arc(inner: Arc<scraper::Selector>) -> Self {
        Self { inner }
    }

    pub(crate) fn as_scraper(&self) -> &scraper::Selector {
        &self.inner
    }
}
