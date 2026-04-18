use super::{prompt, shared, TokenUsage, UsageCounters};
use crate::error::KumoError;
use async_trait::async_trait;
use rig::client::{CompletionClient, Nothing};
use rig::completion::{CompletionModel, ToolDefinition};
use rig::providers::ollama;
use schemars::Schema;
use serde_json::Value;
use std::sync::Arc;

/// LLM client for local Ollama, powered by rig-core.
///
/// Uses `OLLAMA_API_BASE_URL` env var (default: `http://localhost:11434`).
/// For a custom URL, use [`OllamaClient::with_url`].
///
/// # Example
/// ```rust,ignore
/// let client = OllamaClient::new().model("llama3.2");
///
/// let items: Vec<Item> = res.extract(&client).await?;
/// ```
pub struct OllamaClient {
    inner: ollama::Client,
    model: String,
    system_prompt: Option<String>,
    prompt_template: Option<String>,
    strip_scripts: bool,
    max_tokens: u64,
    usage: Arc<UsageCounters>,
}

impl OllamaClient {
    /// Create a client using the default Ollama URL (`http://localhost:11434`).
    pub fn new() -> Self {
        let inner = ollama::Client::builder()
            .api_key(Nothing)
            .build()
            .expect("failed to build Ollama client");
        Self {
            inner,
            model: "llama3.2".into(),
            system_prompt: None,
            prompt_template: None,
            strip_scripts: false,
            max_tokens: 4096,
            usage: UsageCounters::new(),
        }
    }

    /// Create a client pointing at a custom Ollama base URL.
    pub fn with_url(base_url: impl AsRef<str>) -> Self {
        let inner = ollama::Client::builder()
            .api_key(Nothing)
            .base_url(base_url.as_ref())
            .build()
            .expect("failed to build Ollama client");
        Self {
            inner,
            model: "llama3.2".into(),
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

impl Default for OllamaClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::LlmClient for OllamaClient {
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
            .map_err(|e| shared::llm_err(format!("Ollama error — {e}")))?;

        let usage = TokenUsage::from_rig(&resp.usage);
        let value = shared::extract_tool_input(resp.choice, "extract")?;
        self.usage.add(&usage);
        Ok((value, usage))
    }
}
