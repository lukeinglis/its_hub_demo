//! Inference-time scaling gateway for LLMs.
//!
//! `its_hub_rs` is a lean gateway binary that receives inference requests,
//! fans out to vLLM, runs ITS algorithms, and returns results. An Axum HTTP
//! server exposes an OpenAI-compatible chat completions API so any existing
//! client can benefit from inference-time scaling with a single `budget`
//! parameter.

/// Public API interfaces: traits, types, errors.
pub mod api;
/// Core implementations: algorithms, LM client, reward models.
pub mod core;
/// Axum HTTP server with configure, chat completions, models, and health endpoints.
pub mod server;

// Public re-exports for the gateway binary
pub use core::algorithms::beam_search::BeamSearch;
pub use core::algorithms::best_of_n::{BestOfN, HttpOrmClient};
pub use api::reward_models::orm::OutcomeRewardModel;
pub use core::algorithms::particle_gibbs::{
    EntropicParticleFiltering, ParticleFiltering, ParticleGibbs,
};
pub use core::algorithms::planning_wrapper::PlanningWrapper;
pub use core::algorithms::self_consistency::{create_regex_projection_function, SelfConsistency};
pub use api::{AlgorithmOutput, ProcessRewardModel, ScalingAlgorithm};
pub use core::cache::{CacheKey, CacheStats, TokenCache};
pub use core::lms::LmClient;
pub use core::reward_models::{HttpProcessRewardModel, JudgeMode, LlmJudgeRewardModel};
