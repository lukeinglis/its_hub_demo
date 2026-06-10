//! ScalingAlgorithm trait and AlgorithmOutput enum.
//!
//! Extracted from the old algorithms/mod.rs to match v1's api/algorithm.py.

use async_trait::async_trait;
use serde_json::Value;

use crate::api::types::ChatMessage;
use crate::api::lm::AbstractLanguageModel;

#[derive(Debug, Clone)]
pub enum AlgorithmOutput {
    ResponseOnly(Value),
    Full { selected: Value, metadata: Value },
}

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait ScalingAlgorithm: Send + Sync {
    async fn infer(
        &self,
        client: &dyn AbstractLanguageModel,
        messages: &[ChatMessage],
        budget: u32,
        return_response_only: bool,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
        tools: Option<&Value>,
        tool_choice: Option<&Value>,
    ) -> Result<AlgorithmOutput, anyhow::Error>;
}
