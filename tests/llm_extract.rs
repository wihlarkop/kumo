#![cfg(feature = "llm")]

use kumo::{
    error::KumoError,
    extract::Response,
    llm::{LlmClient, ResponseExtractExt, TokenUsage},
};
use schemars::JsonSchema;
use serde::Deserialize;

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
        _schema: &serde_json::Value,
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
    let client = FakeLlm::new(serde_json::json!({ "title": "hi", "count": 7 })).with_usage(100, 50);
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
