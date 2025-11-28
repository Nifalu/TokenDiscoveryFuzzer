use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;

use crate::strategies::Strategy;

static CONFIG: OnceLock<TokenDiscoveryConfig> = OnceLock::new();

#[derive(Deserialize, Debug)]
pub struct TokenDiscoveryConfig {
    pub min_corpus_size: usize,
    pub search_interval: u32,
    pub max_tokens: usize,
    pub recent_entries_count: usize,
    pub strategy: Strategy,  // <-- holds the selected strategy
}

pub fn config() -> &'static TokenDiscoveryConfig {
    CONFIG.get_or_init(|| {
        fs::read_to_string("token_config.yaml")
            .map_err(|e| panic!("Failed to load token_config.yaml: {e}"))
            .and_then(|s| {
                serde_yaml::from_str(&s)
                    .map_err(|e| panic!("Failed to parse token_config.yaml: {e}"))
            })
            .unwrap()
    })
}