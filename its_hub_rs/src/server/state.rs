use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::algorithms::ScalingAlgorithm;
use crate::client::LmClient;

pub struct AlgorithmConfig {
    pub name: String,
    pub algorithm: Box<dyn ScalingAlgorithm>,
}

pub struct AppState {
    pub algorithm: RwLock<Option<AlgorithmConfig>>,
    pub clients: RwLock<HashMap<String, Arc<LmClient>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            algorithm: RwLock::new(None),
            clients: RwLock::new(HashMap::new()),
        }
    }
}
