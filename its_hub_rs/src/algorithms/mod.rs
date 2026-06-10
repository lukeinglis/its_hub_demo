// Re-export shim: algorithms now live in crate::core::algorithms
// Traits live in crate::api

pub use crate::api::algorithm::{AlgorithmOutput, ScalingAlgorithm};
pub use crate::api::reward_models::prm::ProcessRewardModel;

pub mod beam_search {
    pub use crate::core::algorithms::beam_search::*;
}
pub mod best_of_n {
    pub use crate::core::algorithms::best_of_n::*;
    pub use crate::api::reward_models::orm::OutcomeRewardModel;
}
pub mod self_consistency {
    pub use crate::core::algorithms::self_consistency::*;
}
pub mod particle_gibbs {
    pub use crate::core::algorithms::particle_gibbs::*;
}
pub mod planning_wrapper {
    pub use crate::core::algorithms::planning_wrapper::*;
}
pub mod metropolis_hastings {
    pub use crate::core::algorithms::metropolis_hastings::*;
}
