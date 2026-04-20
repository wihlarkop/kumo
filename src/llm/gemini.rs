use super::{TokenUsage, UsageCounters, prompt, shared};
use crate::error::KumoError;
use async_trait::async_trait;
use rig::client::CompletionClient;
use rig::completion::{CompletionModel, ToolDefinition};
use rig::providers::gemini;
use serde_json::Value;
use std::sync::Arc;

pub mod models {
    pub use crate::llm::models::gemini::*;
}

/// LLM client for Google Gemini, powered by rig-core.
///
/// # Example
/// ```rust,ignore
/// use kumo::llm::gemini::models;
///
/// let client = GeminiClient::new(std::env::var("GEMINI_API_KEY")?)
///     .model(models::GEMINI_2_0_FLASH);
///
/// let items: Vec<Item> = res.extract(&client).await?;
/// ```
pub struct GeminiClient {
    inner: gemini::Client,
    model: String,
    system_prompt: Option<String>,
    prompt_template: Option<String>,
    strip_scripts: bool,
    max_tokens: u64,
    usage: Arc<UsageCounters>,
}

impl GeminiClient {
    /// Create a new Gemini client.
    ///
    /// Default model: `gemini-2.0-flash`. Override with `.model()` or use the
    /// constants in [`models`].
    pub fn new(api_key: impl Into<String>) -> Self {
        let key = api_key.into();
        Self {
            inner: gemini::Client::new(key).expect("failed to build Gemini client"),
            model: models::GEMINI_2_5_FLASH.into(),
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
impl super::LlmClient for GeminiClient {
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
            .map_err(|e| shared::llm_err(format!("Gemini API error — {e}")))?;

        let usage = TokenUsage::from_rig(&resp.usage);
        let value = shared::extract_tool_input(resp.choice, "extract")?;
        self.usage.add(&usage);
        Ok((value, usage))
    }
}
