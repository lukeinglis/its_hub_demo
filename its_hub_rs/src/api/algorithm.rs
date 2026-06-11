//! ScalingAlgorithm trait and AlgorithmOutput enum.

use async_trait::async_trait;
use serde_json::Value;

use crate::api::types::{ChatCompletionUsage, ChatMessage};
use crate::core::lms::LmClient;

#[derive(Debug, Clone)]
pub enum AlgorithmOutput {
    ResponseOnly {
        message: Value,
        usage: Option<ChatCompletionUsage>,
    },
    Full {
        selected: Value,
        metadata: Value,
        usage: Option<ChatCompletionUsage>,
    },
}

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait ScalingAlgorithm: Send + Sync {
    async fn infer(
        &self,
        client: &LmClient,
        messages: &[ChatMessage],
        budget: u32,
        return_response_only: bool,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<AlgorithmOutput, anyhow::Error>;
}
