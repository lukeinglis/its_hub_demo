use async_trait::async_trait;
use serde_json::Value;

use crate::api::{AbstractLanguageModel, AlgorithmOutput, ScalingAlgorithm};
use crate::api::types::ChatMessage;

/// Placeholder for a Metropolis-Hastings sampling algorithm.
///
/// Not yet implemented; calling `infer()` returns an error.
pub struct MetropolisHastings;

impl MetropolisHastings {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MetropolisHastings {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScalingAlgorithm for MetropolisHastings {
    async fn infer(
        &self,
        _client: &dyn AbstractLanguageModel,
        _messages: &[ChatMessage],
        _budget: u32,
        _return_response_only: bool,
        _temperature: Option<f64>,
        _max_tokens: Option<u32>,
        _tools: Option<&Value>,
        _tool_choice: Option<&Value>,
    ) -> Result<AlgorithmOutput, anyhow::Error> {
        Err(anyhow::anyhow!("MetropolisHastings not yet implemented"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metropolis_hastings_default() {
        let _mh = MetropolisHastings::default();
    }
}
