use std::collections::HashMap;
use reqwest::header::{HeaderName, HeaderValue};
use crate::error::KumoError;
use super::{Middleware, Request};

/// Injects a fixed set of HTTP headers into every request.
///
/// # Example
/// ```rust,ignore
/// DefaultHeaders::new().user_agent("kumo/0.1 (+https://github.com/you/kumo)")
/// ```
pub struct DefaultHeaders {
    headers: HashMap<String, String>,
}

impl DefaultHeaders {
    pub fn new() -> Self {
        Self {
            headers: HashMap::new(),
        }
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn user_agent(self, ua: impl Into<String>) -> Self {
        self.header("User-Agent", ua)
    }
}

impl Default for DefaultHeaders {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Middleware for DefaultHeaders {
    async fn before_request(&self, request: &mut Request) -> Result<(), KumoError> {
        for (name, value) in &self.headers {
            if let (Ok(n), Ok(v)) = (
                HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_str(value),
            ) {
                request.headers.insert(n, v);
            }
        }
        Ok(())
    }
}
