pub mod client;
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

pub use client::{LlmClient, TokenUsage};

#[cfg(feature = "llm")]
use std::sync::Arc;
#[cfg(feature = "llm")]
use std::sync::atomic::{AtomicU64, Ordering};

/// Shared atomic counters accumulated across all `extract_json` calls on a client.
#[cfg(feature = "llm")]
pub(crate) struct UsageCounters {
    pub input: AtomicU64,
    pub output: AtomicU64,
    pub total: AtomicU64,
    pub cached_input: AtomicU64,
    pub cache_creation_input: AtomicU64,
}

#[cfg(feature = "llm")]
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

#[cfg(feature = "llm")]
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

/// Extension trait that adds `.extract::<T>()` and `.extract_with_usage::<T>()` to `Response`.
///
/// Imported via `use kumo::prelude::*` when the `llm` feature is enabled.
#[cfg(feature = "llm")]
#[async_trait::async_trait]
pub trait ResponseExtractExt {
    /// Extract structured data of type `T` from this response using an LLM.
    ///
    /// `T` must derive both `serde::Deserialize` and `schemars::JsonSchema`.
    async fn extract<T>(&self, client: &dyn LlmClient) -> Result<T, crate::error::KumoError>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Send;

    /// Extract structured data and return the token usage for this call.
    async fn extract_with_usage<T>(
        &self,
        client: &dyn LlmClient,
    ) -> Result<(T, TokenUsage), crate::error::KumoError>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Send;
}

#[cfg(feature = "llm")]
#[async_trait::async_trait]
impl ResponseExtractExt for crate::extract::Response {
    async fn extract<T>(&self, client: &dyn LlmClient) -> Result<T, crate::error::KumoError>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Send,
    {
        let (value, _usage) = self.extract_with_usage::<T>(client).await?;
        Ok(value)
    }

    async fn extract_with_usage<T>(
        &self,
        client: &dyn LlmClient,
    ) -> Result<(T, TokenUsage), crate::error::KumoError>
    where
        T: serde::de::DeserializeOwned + schemars::JsonSchema + Send,
    {
        let schema = schemars::schema_for!(T);
        let schema_json = serde_json::to_value(&schema)
            .map_err(|e| crate::error::KumoError::Llm(format!("schema serialization: {e}")))?;
        let body_text = self.text().unwrap_or("");
        let (json, usage) = client.extract_json(&schema_json, body_text).await?;
        let value = serde_json::from_value(json)
            .map_err(|e| crate::error::KumoError::Llm(format!("schema mismatch: {e}")))?;
        Ok((value, usage))
    }
}
