use crate::error::KumoError;
use rig::OneOrMany;
use rig::completion::{AssistantContent, ToolDefinition};
use serde_json::Value;

pub(super) struct ExtractArgs {
    pub user_content: String,
    pub system: String,
    pub tool: ToolDefinition,
}

pub(super) fn build_extract_args(
    schema: &Value,
    html: &str,
    system_prompt: Option<&str>,
    prompt_template: Option<&str>,
    strip_scripts: bool,
) -> ExtractArgs {
    let html = if strip_scripts {
        super::prompt::strip_scripts_and_styles(html)
    } else {
        html.to_string()
    };

    let user_template = prompt_template.unwrap_or(super::prompt::DEFAULT_USER_PROMPT);
    let user_content = super::prompt::render_user_prompt(user_template, &html);
    let system = system_prompt
        .unwrap_or(super::prompt::DEFAULT_SYSTEM_PROMPT)
        .to_string();

    let tool = ToolDefinition {
        name: "extract".to_string(),
        description: "Extract structured data from the provided HTML.".to_string(),
        parameters: schema.clone(),
    };

    ExtractArgs {
        user_content,
        system,
        tool,
    }
}

pub(super) fn extract_tool_input(
    choice: OneOrMany<AssistantContent>,
    tool_name: &str,
) -> Result<Value, KumoError> {
    for content in choice {
        if let AssistantContent::ToolCall(tc) = content
            && tc.function.name == tool_name
        {
            return Ok(tc.function.arguments);
        }
    }
    Err(KumoError::Llm(format!(
        "no '{tool_name}' tool_use block in LLM response"
    )))
}

pub(super) fn llm_err(msg: impl std::fmt::Display) -> KumoError {
    KumoError::Llm(msg.to_string())
}
