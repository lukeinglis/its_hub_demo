use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::algorithms::ScalingAlgorithm;
use crate::client::LmBackend;

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
    pub clients: RwLock<HashMap<String, Arc<LmBackend>>>,
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
        }
    }
}
