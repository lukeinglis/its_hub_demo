pub mod best_of_n;
pub mod self_consistency;

use async_trait::async_trait;
use serde_json::Value;

use crate::chat_messages::ChatMessages;
use crate::client::LmClient;
use crate::types::ChatMessage;

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

/// Process reward model trait for scoring intermediate reasoning steps.
///
/// Used by beam search and particle filtering algorithms to evaluate
/// partial responses step by step.
///
/// Mirrors Python's `AbstractProcessRewardModel` from `its_hub/base.py`.
#[async_trait]
pub trait ProcessRewardModel: Send + Sync {
    /// Score each step in a partial response.
    ///
    /// Returns a vector of scores, one per step in the response prefix.
    async fn score(
        &self,
        prompt_or_messages: &ChatMessages,
        steps: &[String],
    ) -> Result<Vec<f64>, anyhow::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPRM {
        fixed_scores: Vec<f64>,
    }

    #[async_trait]
    impl ProcessRewardModel for MockPRM {
        async fn score(
            &self,
            _prompt_or_messages: &ChatMessages,
            steps: &[String],
        ) -> Result<Vec<f64>, anyhow::Error> {
            Ok(self.fixed_scores[..steps.len()].to_vec())
        }
    }

    #[tokio::test]
    async fn test_process_reward_model_trait() {
        let prm = MockPRM {
            fixed_scores: vec![0.9, 0.8, 0.7],
        };
        let cm = ChatMessages::from_string("What is 2+2?");
        let steps = vec!["Step 1: Add 2+2".to_string(), "Step 2: = 4".to_string()];

        let scores = prm.score(&cm, &steps).await.unwrap();
        assert_eq!(scores.len(), 2);
        assert!((scores[0] - 0.9).abs() < f64::EPSILON);
        assert!((scores[1] - 0.8).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_process_reward_model_with_messages() {
        let prm = MockPRM {
            fixed_scores: vec![0.5, 0.6, 0.7],
        };
        let cm = ChatMessages::from_messages(vec![crate::types::ChatMessage {
            role: "user".to_string(),
            content: Some(crate::types::Content::Text("Solve x+1=3".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }]);
        let steps = vec!["x = 3 - 1".to_string(), "x = 2".to_string()];

        let scores = prm.score(&cm, &steps).await.unwrap();
        assert_eq!(scores, vec![0.5, 0.6]);
    }

    #[tokio::test]
    async fn test_process_reward_model_empty_steps() {
        let prm = MockPRM {
            fixed_scores: vec![],
        };
        let cm = ChatMessages::from_string("prompt");
        let steps: Vec<String> = vec![];

        let scores = prm.score(&cm, &steps).await.unwrap();
        assert!(scores.is_empty());
    }
}
