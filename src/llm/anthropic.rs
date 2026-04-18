use super::{prompt, shared, TokenUsage, UsageCounters};
use crate::error::KumoError;
use async_trait::async_trait;
use rig::client::CompletionClient;
use rig::completion::{CompletionModel, ToolDefinition};
use rig::providers::anthropic;
use schemars::Schema;
use serde_json::Value;
use std::sync::Arc;

pub mod models {
    pub use crate::llm::models::anthropic::*;
}

/// LLM client for Anthropic Claude, powered by rig-core.
///
/// Uses Claude's tool-use API to guarantee structured JSON output that matches
/// your schema — no fragile markdown parsing.
///
/// # Example
/// ```rust,ignore
/// use kumo::llm::anthropic::models;
///
/// let client = AnthropicClient::new(std::env::var("ANTHROPIC_API_KEY")?)
///     .model(models::CLAUDE_HAIKU_4_5)
///     .system_prompt("This page lists book products. Prices are in GBP.");
///
/// let books: Vec<Book> = res.extract(&client).await?;
/// ```
pub struct AnthropicClient {
    inner: anthropic::Client,
    model: String,
    system_prompt: Option<String>,
    prompt_template: Option<String>,
    strip_scripts: bool,
    max_tokens: u64,
    usage: Arc<UsageCounters>,
}

impl AnthropicClient {
    /// Create a new Anthropic Claude client.
    ///
    /// Default model: `claude-sonnet-4-6`. Override with `.model()` or use the
    /// constants in [`models`].
    pub fn new(api_key: impl Into<String>) -> Self {
        let key = api_key.into();
        Self {
            inner: anthropic::Client::builder()
                .api_key(key)
                .build()
                .expect("failed to build Anthropic client"),
            model: models::CLAUDE_SONNET_4_6.into(),
            system_prompt: None,
            prompt_template: None,
            strip_scripts: false,
            max_tokens: 4096,
            usage: UsageCounters::new(),
        }
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn prompt_template(mut self, template: impl Into<String>) -> Self {
        self.prompt_template = Some(template.into());
        self
    }

    pub fn strip_scripts_and_styles(mut self, yes: bool) -> Self {
        self.strip_scripts = yes;
        self
    }

    pub fn max_tokens(mut self, n: u64) -> Self {
        self.max_tokens = n;
        self
    }

    /// Returns the cumulative token usage across all `extract` calls on this client.
    pub fn total_usage(&self) -> TokenUsage {
        self.usage.snapshot()
    }
}

#[async_trait]
impl super::LlmClient for AnthropicClient {
    async fn extract_json(
        &self,
        schema: &Schema,
        html: &str,
    ) -> Result<(Value, TokenUsage), KumoError> {
        let html = if self.strip_scripts {
            prompt::strip_scripts_and_styles(html)
        } else {
            html.to_string()
        };

        let user_template = self
            .prompt_template
            .as_deref()
            .unwrap_or(prompt::DEFAULT_USER_PROMPT);
        let user_content = prompt::render_user_prompt(user_template, &html);
        let system = self
            .system_prompt
            .as_deref()
            .unwrap_or(prompt::DEFAULT_SYSTEM_PROMPT);

        let tool = ToolDefinition {
            name: "extract".to_string(),
            description: "Extract structured data from the provided HTML.".to_string(),
            parameters: schema.as_value().clone(),
        };

        let model = self.inner.completion_model(&self.model);
        let request = model
            .completion_request(user_content)
            .preamble(system.to_string())
            .tool(tool)
            .max_tokens(self.max_tokens)
            .build();

        let resp = model
            .completion(request)
            .await
            .map_err(|e| shared::llm_err(format!("Anthropic API error — {e}")))?;

        let usage = TokenUsage::from_rig(&resp.usage);
        let value = shared::extract_tool_input(resp.choice, "extract")?;
        self.usage.add(&usage);
        Ok((value, usage))
    }
}
