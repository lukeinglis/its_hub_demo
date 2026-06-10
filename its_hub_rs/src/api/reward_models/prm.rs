//! ProcessRewardModel trait for scoring intermediate reasoning steps.

use async_trait::async_trait;

use crate::api::types::ChatMessage;

#[async_trait]
pub trait ProcessRewardModel: Send + Sync {
    /// Score each step in a partial response.
    ///
    /// Returns a vector of scores, one per step in the response prefix.
    async fn score(
        &self,
        prompt_messages: &[ChatMessage],
        steps: &[String],
    ) -> Result<Vec<f64>, anyhow::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::Content;

    struct MockPRM {
        fixed_scores: Vec<f64>,
    }

    #[async_trait]
    impl ProcessRewardModel for MockPRM {
        async fn score(
            &self,
            _prompt_messages: &[ChatMessage],
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
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Text("What is 2+2?".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];
        let steps = vec!["Step 1: Add 2+2".to_string(), "Step 2: = 4".to_string()];

        let scores = prm.score(&messages, &steps).await.unwrap();
        assert_eq!(scores.len(), 2);
        assert!((scores[0] - 0.9).abs() < f64::EPSILON);
        assert!((scores[1] - 0.8).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_process_reward_model_empty_steps() {
        let prm = MockPRM {
            fixed_scores: vec![],
        };
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(Content::Text("prompt".to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];
        let steps: Vec<String> = vec![];

        let scores = prm.score(&messages, &steps).await.unwrap();
        assert!(scores.is_empty());
    }
}
