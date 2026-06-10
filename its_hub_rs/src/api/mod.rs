//! Public API interfaces for inference-time scaling.
//!
//! Mirrors Python's `its_hub/api/` module: traits, types, and error definitions.

pub mod algorithm;
pub mod errors;
pub mod lm;
pub mod reward_models;
pub mod types;

// Re-export key items for convenience
pub use algorithm::{AlgorithmOutput, ScalingAlgorithm};
pub use errors::LmClientError;
pub use lm::AbstractLanguageModel;
pub use reward_models::{OutcomeRewardModel, ProcessRewardModel};
pub use types::{
    ChatCompletionChoice, ChatCompletionRequest, ChatCompletionResponse, ChatCompletionUsage,
    ChatMessage, ChatMessages, ConfigRequest, ConfigResponse, Content, ContentPart,
    HealthResponse, ModelInfo, ModelsResponse, ToolCall, ToolFunction,
    extract_content_from_lm_response,
};
