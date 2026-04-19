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
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Token usage returned by a single LLM extraction call.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

impl TokenUsage {
    pub(crate) fn from_rig(u: &rig::completion::Usage) -> Self {
        Self {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            total_tokens: u.total_tokens,
            cached_input_tokens: u.cached_input_tokens,
            cache_creation_input_tokens: u.cache_creation_input_tokens,
        }
    }
}

/// Shared atomic counters accumulated across all `extract_json` calls on a client.
pub(crate) struct UsageCounters {
    pub input: AtomicU64,
    pub output: AtomicU64,
    pub total: AtomicU64,
    pub cached_input: AtomicU64,
    pub cache_creation_input: AtomicU64,
}

impl UsageCounters {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            input: AtomicU64::new(0),
            output: AtomicU64::new(0),
            total: AtomicU64::new(0),
            cached_input: AtomicU64::new(0),
            cache_creation_input: AtomicU64::new(0),
        })
    }

    pub(crate) fn add(&self, usage: &TokenUsage) {
        self.input.fetch_add(usage.input_tokens, Ordering::Relaxed);
        self.output
            .fetch_add(usage.output_tokens, Ordering::Relaxed);
        self.total.fetch_add(usage.total_tokens, Ordering::Relaxed);
        self.cached_input
            .fetch_add(usage.cached_input_tokens, Ordering::Relaxed);
        self.cache_creation_input
            .fetch_add(usage.cache_creation_input_tokens, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(&self) -> TokenUsage {
        TokenUsage {
            input_tokens: self.input.load(Ordering::Relaxed),
            output_tokens: self.output.load(Ordering::Relaxed),
            total_tokens: self.total.load(Ordering::Relaxed),
            cached_input_tokens: self.cached_input.load(Ordering::Relaxed),
            cache_creation_input_tokens: self.cache_creation_input.load(Ordering::Relaxed),
        }
    }
}

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
///     async fn extract_json(
///         &self,
///         schema: &Schema,
///         html: &str,
///     ) -> Result<(serde_json::Value, TokenUsage), KumoError> {
///         // call your provider, return the JSON value and token usage
///         Ok((value, TokenUsage::default()))
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    /// Given a JSON schema and HTML, return the extracted data and token usage.
    async fn extract_json(
        &self,
        schema: &Schema,
        html: &str,
    ) -> Result<(serde_json::Value, TokenUsage), KumoError>;
}

/// Extension trait that adds `.extract::<T>()` and `.extract_with_usage::<T>()` to `Response`.
///
/// Imported via `use kumo::prelude::*` when the `llm` feature is enabled.
#[async_trait::async_trait]
pub trait ResponseExtractExt {
    /// Extract structured data of type `T` from this response using an LLM.
    ///
    /// `T` must derive both `serde::Deserialize` and `schemars::JsonSchema`.
    /// Doc comments on fields are included in the schema as extraction hints.
    ///
    /// Token usage is recorded on the client's internal counter (readable via
    /// `client.total_usage()`) but not returned here. Use [`extract_with_usage`]
    /// if you need per-call token counts.
    ///
    /// # Example
    /// ```rust,ignore
    /// let quotes: Vec<Quote> = res.extract(&claude_client).await?;
    /// ```
    async fn extract<T>(&self, client: &dyn LlmClient) -> Result<T, KumoError>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Send;

    /// Extract structured data and return the token usage for this call.
    ///
    /// # Example
    /// ```rust,ignore
    /// let (quotes, usage) = res.extract_with_usage::<Vec<Quote>>(&claude_client).await?;
    /// println!("tokens: {} in / {} out", usage.input_tokens, usage.output_tokens);
    /// ```
    async fn extract_with_usage<T>(
        &self,
        client: &dyn LlmClient,
    ) -> Result<(T, TokenUsage), KumoError>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Send;
}

#[async_trait::async_trait]
impl ResponseExtractExt for Response {
    async fn extract<T>(&self, client: &dyn LlmClient) -> Result<T, KumoError>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Send,
    {
        let (value, _usage) = self.extract_with_usage::<T>(client).await?;
        Ok(value)
    }

    async fn extract_with_usage<T>(
        &self,
        client: &dyn LlmClient,
    ) -> Result<(T, TokenUsage), KumoError>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Send,
    {
        let schema = schemars::schema_for!(T);
        let body_text = self.text().unwrap_or("");
        let (json, usage) = client.extract_json(&schema, body_text).await?;
        let value = serde_json::from_value(json)
            .map_err(|e| KumoError::Llm(format!("schema mismatch: {e}")))?;
        Ok((value, usage))
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
        Response::from_parts("http://example.com", 200, body)
    }

    struct FakeLlm {
        returns: serde_json::Value,
        usage: TokenUsage,
    }

    impl FakeLlm {
        fn new(returns: serde_json::Value) -> Self {
            Self {
                returns,
                usage: TokenUsage::default(),
            }
        }

        fn with_usage(mut self, input: u64, output: u64) -> Self {
            self.usage = TokenUsage {
                input_tokens: input,
                output_tokens: output,
                total_tokens: input + output,
                ..Default::default()
            };
            self
        }
    }

    #[async_trait::async_trait]
    impl LlmClient for FakeLlm {
        async fn extract_json(
            &self,
            _schema: &Schema,
            _html: &str,
        ) -> Result<(serde_json::Value, TokenUsage), KumoError> {
            Ok((self.returns.clone(), self.usage))
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
        let client = FakeLlm::new(serde_json::json!({ "title": "hello", "count": 42 }));
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
        let client = FakeLlm::new(serde_json::json!([
            { "title": "a", "count": 1 },
            { "title": "b", "count": 2 }
        ]));
        let resp = make_response("<html>irrelevant</html>");
        let items: Vec<TestItem> = resp.extract(&client).await.unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "a");
    }

    #[tokio::test]
    async fn extract_schema_mismatch_returns_llm_error() {
        let client = FakeLlm::new(serde_json::json!({ "wrong_field": true }));
        let resp = make_response("<html></html>");
        let result: Result<TestItem, _> = resp.extract(&client).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("llm error"), "expected Llm error, got: {err}");
    }

    #[tokio::test]
    async fn extract_with_usage_returns_both() {
        let client =
            FakeLlm::new(serde_json::json!({ "title": "hi", "count": 7 })).with_usage(100, 50);
        let resp = make_response("<html>irrelevant</html>");
        let (item, usage) = resp.extract_with_usage::<TestItem>(&client).await.unwrap();
        assert_eq!(
            item,
            TestItem {
                title: "hi".into(),
                count: 7
            }
        );
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
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
