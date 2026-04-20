use crate::error::KumoError;

/// Token usage returned by a single LLM extraction call.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

/// Provider-agnostic LLM client for structured data extraction.
///
/// Implement this trait to plug in any LLM provider not shipped with kumo.
///
/// The `schema` parameter is a JSON Schema object (`{"type":"object","properties":{...}}`)
/// describing the fields to extract. Each provider converts it into the appropriate
/// tool-use / structured-output call for their API.
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    /// Given a JSON schema (as a `serde_json::Value`) and HTML, return the
    /// extracted data and token usage.
    async fn extract_json(
        &self,
        schema: &serde_json::Value,
        html: &str,
    ) -> Result<(serde_json::Value, TokenUsage), KumoError>;
}
