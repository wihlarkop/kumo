use super::{TokenUsage, UsageCounters, prompt, shared};
use crate::error::KumoError;
use async_trait::async_trait;
use rig::client::CompletionClient;
use rig::completion::{CompletionModel, ToolDefinition};
use rig::providers::openai;
use serde_json::Value;
use std::sync::Arc;

pub mod models {
    pub use crate::llm::models::openai::*;
}

/// LLM client for OpenAI, powered by rig-core.
///
/// # Example
/// ```rust,ignore
/// use kumo::llm::openai::models;
///
/// let client = OpenAiClient::new(std::env::var("OPENAI_API_KEY")?)
///     .model(models::GPT_4O_MINI)
///     .system_prompt("This page lists book products. Prices are in GBP.");
///
/// let books: Vec<Book> = res.extract(&client).await?;
/// ```
pub struct OpenAiClient {
    inner: openai::Client,
    model: String,
    system_prompt: Option<String>,
    prompt_template: Option<String>,
    strip_scripts: bool,
    max_tokens: u64,
    usage: Arc<UsageCounters>,
}

impl OpenAiClient {
    /// Create a new OpenAI client.
    ///
    /// Default model: `gpt-4o`. Override with `.model()` or use the constants
    /// in [`models`].
    pub fn new(api_key: impl Into<String>) -> Self {
        let key = api_key.into();
        Self {
            inner: openai::Client::builder()
                .api_key(key)
                .build()
                .expect("failed to build OpenAI client"),
            model: models::GPT_5.into(),
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
impl super::LlmClient for OpenAiClient {
    async fn extract_json(
        &self,
        schema: &Value,
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
            parameters: schema.clone(),
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
            .map_err(|e| shared::llm_err(format!("OpenAI API error — {e}")))?;

        let usage = TokenUsage::from_rig(&resp.usage);
        let value = shared::extract_tool_input(resp.choice, "extract")?;
        self.usage.add(&usage);
        Ok((value, usage))
    }
}
