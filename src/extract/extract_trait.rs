use super::selector::Element;
use crate::error::KumoError;
use crate::llm::client::LlmClient;

/// Implemented by types that can be extracted from a single HTML [`Element`].
///
/// Use `#[derive(Extract)]` (requires the `derive` feature) to generate this
/// implementation automatically from `#[extract(...)]` field annotations.
///
/// The `llm` parameter allows CSS-selector extraction to fall back to an LLM
/// when a field's selector returns no match. Pass `None` to disable fallback.
#[async_trait::async_trait]
pub trait Extract: Sized {
    async fn extract_from(
        element: &Element,
        llm: Option<&dyn LlmClient>,
    ) -> Result<Self, KumoError>;
}
