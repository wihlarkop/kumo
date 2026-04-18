pub mod models;
pub mod prompt;

#[cfg(feature = "llm")]
mod shared;

#[cfg(feature = "claude")]
pub mod anthropic;
#[cfg(feature = "gemini")]
pub mod gemini;
#[cfg(feature = "ollama")]
pub mod ollama;
#[cfg(feature = "openai")]
pub mod openai;

#[cfg(feature = "claude")]
pub use anthropic::AnthropicClient;
#[cfg(feature = "gemini")]
pub use gemini::GeminiClient;
#[cfg(feature = "ollama")]
pub use ollama::OllamaClient;
#[cfg(feature = "openai")]
pub use openai::OpenAiClient;

use crate::{error::KumoError, extract::Response};
use schemars::Schema;

/// Provider-agnostic LLM client for structured data extraction.
///
/// Implement this trait to plug in any LLM provider not shipped with kumo.
///
/// # Example (custom provider)
/// ```rust,ignore
/// struct MyLlm;
///
/// #[async_trait::async_trait]
/// impl LlmClient for MyLlm {
///     async fn extract_json(&self, schema: &Schema, html: &str) -> Result<Value, KumoError> {
///         // call your provider, return the JSON value
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    /// Given a JSON schema and HTML, return the extracted data as a JSON value.
    async fn extract_json(
        &self,
        schema: &Schema,
        html: &str,
    ) -> Result<serde_json::Value, KumoError>;
}

/// Extension trait that adds `.extract::<T>()` to `Response`.
///
/// Imported via `use kumo::prelude::*` when the `llm` feature is enabled.
#[async_trait::async_trait]
pub trait ResponseExtractExt {
    /// Extract structured data of type `T` from this response using an LLM.
    ///
    /// `T` must derive both `serde::Deserialize` and `schemars::JsonSchema`.
    /// Doc comments on fields are included in the schema as extraction hints.
    ///
    /// # Example
    /// ```rust,ignore
    /// #[derive(Deserialize, JsonSchema)]
    /// struct Quote {
    ///     /// Full quote text including punctuation
    ///     text: String,
    ///     author: String,
    /// }
    ///
    /// let quotes: Vec<Quote> = res.extract(&claude_client).await?;
    /// ```
    async fn extract<T>(&self, client: &dyn LlmClient) -> Result<T, KumoError>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Send;
}

#[async_trait::async_trait]
impl ResponseExtractExt for Response {
    async fn extract<T>(&self, client: &dyn LlmClient) -> Result<T, KumoError>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Send,
    {
        let schema = schemars::schema_for!(T);
        let json = client.extract_json(&schema, self.text()).await?;
        serde_json::from_value(json).map_err(|e| KumoError::Llm(format!("schema mismatch: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::Response;
    use reqwest::header::HeaderMap;
    use schemars::JsonSchema;
    use serde::Deserialize;
    use std::time::Duration;

    fn make_response(body: &str) -> Response {
        Response {
            url: "http://example.com".into(),
            status: 200,
            headers: HeaderMap::new(),
            elapsed: Duration::ZERO,
            body: body.to_string(),
        }
    }

    struct FakeLlm {
        returns: serde_json::Value,
    }

    #[async_trait::async_trait]
    impl LlmClient for FakeLlm {
        async fn extract_json(
            &self,
            _schema: &Schema,
            _html: &str,
        ) -> Result<serde_json::Value, KumoError> {
            Ok(self.returns.clone())
        }
    }

    #[derive(Debug, Deserialize, JsonSchema, PartialEq)]
    struct TestItem {
        /// The item title
        title: String,
        count: u32,
    }

    #[tokio::test]
    async fn extract_deserializes_llm_json() {
        let client = FakeLlm {
            returns: serde_json::json!({ "title": "hello", "count": 42 }),
        };
        let resp = make_response("<html>irrelevant</html>");
        let item: TestItem = resp.extract(&client).await.unwrap();
        assert_eq!(
            item,
            TestItem {
                title: "hello".into(),
                count: 42
            }
        );
    }

    #[tokio::test]
    async fn extract_vec_deserializes_llm_json() {
        let client = FakeLlm {
            returns: serde_json::json!([
                { "title": "a", "count": 1 },
                { "title": "b", "count": 2 }
            ]),
        };
        let resp = make_response("<html>irrelevant</html>");
        let items: Vec<TestItem> = resp.extract(&client).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "a");
    }

    #[tokio::test]
    async fn extract_schema_mismatch_returns_llm_error() {
        let client = FakeLlm {
            returns: serde_json::json!({ "wrong_field": true }),
        };
        let resp = make_response("<html></html>");
        let result: Result<TestItem, _> = resp.extract(&client).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("llm error"), "expected Llm error, got: {err}");
    }

    #[test]
    fn doc_comments_appear_in_schema() {
        let schema = schemars::schema_for!(TestItem);
        let json = serde_json::to_value(&schema).unwrap();
        let title_desc = json
            .pointer("/properties/title/description")
            .and_then(|v| v.as_str());
        assert_eq!(title_desc, Some("The item title"));
    }
}
