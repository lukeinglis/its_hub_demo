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

pub struct AppState {
    pub algorithm: RwLock<Option<AlgorithmConfig>>,
    pub clients: RwLock<HashMap<String, Arc<LmClient>>>,
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
            algorithm: RwLock::new(None),
            clients: RwLock::new(HashMap::new()),
            passthrough_on_error: RwLock::new(true), // default: safe for production
            cache: RwLock::new(None),
            cache_enabled: RwLock::new(false),
        }
    }
}
