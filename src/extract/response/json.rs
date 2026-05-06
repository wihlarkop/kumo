use crate::{error::KumoError, extract::response::Response};

impl Response {
    /// Deserialize the response body as JSON. Works for both text and binary bodies.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, KumoError> {
        serde_json::from_slice(self.bytes())
            .map_err(|e| KumoError::parse("json deserialization", e))
    }

    /// Evaluate a JSONPath expression against the response body parsed as JSON.
    #[cfg(feature = "jsonpath")]
    pub fn jsonpath(&self, expr: &str) -> Result<Vec<serde_json::Value>, KumoError> {
        use jsonpath_rust::JsonPath;

        let value: serde_json::Value =
            serde_json::from_slice(self.bytes()).map_err(|e| KumoError::parse("json body", e))?;
        let results = value
            .query(expr)
            .map_err(|e| KumoError::parse(format!("jsonpath '{expr}'"), e))?;
        Ok(results.into_iter().cloned().collect())
    }
}
