use async_trait::async_trait;
use serde_json::Value;
use crate::api::errors::LmClientError;
use crate::api::types::ChatMessage;
#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait AbstractLanguageModel: Send + Sync {
    async fn agenerate_single(&self, messages: &[ChatMessage], stop: Option<&str>, max_tokens: Option<u32>, temperature: Option<f64>, include_stop_str_in_output: Option<bool>, tools: Option<&Value>, tool_choice: Option<&Value>) -> Result<Value, LmClientError>;
    async fn fan_out(&self, messages: &[ChatMessage], budget: u32, temperature: Option<f64>, max_tokens: Option<u32>, tools: Option<&Value>, tool_choice: Option<&Value>) -> Vec<Result<Value, LmClientError>> {
        let mut r = Vec::with_capacity(budget as usize);
        for _ in 0..budget { r.push(self.agenerate_single(messages, None, max_tokens, temperature, None, tools, tool_choice).await); }
        r
    }
    fn model_name(&self) -> &str;
}
