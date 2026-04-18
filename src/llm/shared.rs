use crate::error::KumoError;
use rig::completion::AssistantContent;
use rig::OneOrMany;
use serde_json::Value;

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
