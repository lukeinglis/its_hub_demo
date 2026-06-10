//! Inference-time scaling library for LLMs.
//!
//! `its_hub_rs` provides a collection of algorithms that improve LLM
//! response quality by spending additional compute at inference time.
//! Algorithms include majority-vote self-consistency, best-of-N scoring,
//! beam search, particle filtering, and more.  An Axum HTTP server
//! exposes an OpenAI-compatible chat completions API so any existing
//! client can benefit from inference-time scaling with a single
//! `budget` parameter.
//!
//! # Module structure (mirrors v1 Python architecture)
//!
//! - `api` - Public interfaces: traits, types, errors
//! - `core` - Implementation: algorithms, LM clients, reward models, orchestrator
//! - `server` - Axum HTTP server

/// Public API interfaces: traits, types, and error definitions.
pub mod api;
/// Core implementations: algorithms, LM clients, reward models, orchestrator.
pub mod core;
/// Axum HTTP server with configure, chat completions, models, and health endpoints.
pub mod server;

// Backward-compatible re-export shims (delegate to api/core)
/// Scaling algorithm implementations (re-exports from core::algorithms).
pub mod algorithms;
/// Prompt/message abstraction (re-exports from api::types).
pub mod chat_messages;
/// LM backend clients (re-exports from core::lms).
pub mod client;
/// Reward model integrations (re-exports from core::reward_models).
pub mod integration;
/// Incremental text generation (re-exports from core::lms::step_generation).
pub mod step_generation;
/// Shared request/response types (re-exports from api::types + api::errors).
pub mod types;
/// Utility constants and helpers (re-exports from core::utils).
pub mod utils;

// Public re-exports for external consumers
pub use core::algorithms::beam_search::BeamSearch;
pub use core::algorithms::best_of_n::{BestOfN, HttpOrmClient};
pub use api::reward_models::orm::OutcomeRewardModel;
pub use core::algorithms::metropolis_hastings::MetropolisHastings;
pub use core::algorithms::particle_gibbs::{
    EntropicParticleFiltering, ParticleFiltering, ParticleGibbs,
};
pub use core::algorithms::planning_wrapper::PlanningWrapper;
pub use core::algorithms::self_consistency::{create_regex_projection_function, SelfConsistency};
pub use api::{AbstractLanguageModel, AlgorithmOutput, ProcessRewardModel, ScalingAlgorithm};
pub use core::lms::litellm::LiteLLMClient;
pub use core::lms::LmBackend;
pub use core::reward_models::{HttpProcessRewardModel, JudgeMode, LlmJudgeRewardModel};
