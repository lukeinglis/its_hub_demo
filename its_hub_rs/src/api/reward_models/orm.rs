//! OutcomeRewardModel trait.
//!
//! Mirrors Python's `AbstractOutcomeRewardModel` from `its_hub/api/reward_models/orm.py`.

use async_trait::async_trait;

use crate::api::types::ChatMessage;

#[async_trait]
pub trait OutcomeRewardModel: Send + Sync {
    async fn score_batch(
        &self,
        prompt_messages: &[ChatMessage],
        responses: &[String],
    ) -> Result<Vec<f64>, anyhow::Error>;
}
