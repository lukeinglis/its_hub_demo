// Re-export shim: client is now in crate::core::lms
pub use crate::core::lms::{LmBackend, LmClient, EndpointType};

pub mod litellm {
    pub use crate::core::lms::litellm::*;
}

pub mod error {
    pub use crate::api::errors::LmClientError;
}
