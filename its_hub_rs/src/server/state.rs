use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::api::algorithm::ScalingAlgorithm;
use crate::core::cache::TokenCache;
use crate::core::lms::LmClient;

pub struct AlgorithmConfig {
    pub name: String,
    pub algorithm: Arc<dyn ScalingAlgorithm>,
}

impl Clone for AlgorithmConfig {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            algorithm: Arc::clone(&self.algorithm),
        }
    }
}

/// Groups the algorithm and model clients together so they are always
/// swapped atomically during reconfiguration. This prevents a concurrent
/// chat_completions request from seeing the new algorithm with the old client.
pub struct GatewayConfig {
    pub algorithm: AlgorithmConfig,
    pub clients: HashMap<String, Arc<LmClient>>,
}

pub struct AppState {
    /// Algorithm and clients, swapped atomically on reconfigure.
    pub gateway: RwLock<Option<GatewayConfig>>,
    /// When true, algorithm errors fall back to passthrough instead of 500.
    pub passthrough_on_error: RwLock<bool>,
    /// Optional token cache for response caching.
    pub cache: RwLock<Option<Arc<TokenCache>>>,
    /// Whether caching is enabled.
    pub cache_enabled: RwLock<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            gateway: RwLock::new(None),
            passthrough_on_error: RwLock::new(true), // default: safe for production
            cache: RwLock::new(None),
            cache_enabled: RwLock::new(false),
        }
    }
}
