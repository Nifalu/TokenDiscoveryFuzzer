use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedToken {
    pub bytes: Vec<u8>,
    pub initial_weight: f64,  // from corpus analysis (0.0-1.0)
    pub uses: u64,
    pub successes: u64,
}

impl WeightedToken {
    pub fn effective_weight(&self) -> f64 {
        // Each success is valuable, uses barely matter
        let success_bonus = (self.successes as f64).ln_1p() * 0.5;  // diminishing returns
        self.initial_weight + success_bonus
    }
}

