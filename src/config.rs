use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;
use crate::common_substring_discovery::TokenSelectionMode;

static CONFIG: OnceLock<TokenDiscoveryConfig> = OnceLock::new();

#[derive(Deserialize, Debug, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    #[default]
    Threshold,
    MinTokenCount,
}

#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct TokenDiscoveryConfig {
    pub selection_mode: SelectionMode,
    pub min_occurrence_ratio: f64,
    pub min_token_count: usize,
    pub min_corpus_size: usize,
    pub search_interval: u32,
    pub min_token_length: usize,
    pub max_token_length: usize,
    pub max_tokens: usize,
    pub recent_entries_count: usize,
    pub clean_tokens: bool,
}

impl Default for TokenDiscoveryConfig {
    fn default() -> Self {
        Self {
            selection_mode: SelectionMode::default(),
            min_occurrence_ratio: 0.25,
            min_token_count: 50,
            min_corpus_size: 100,
            search_interval: 1000,
            min_token_length: 3,
            max_token_length: 64,
            max_tokens: 100,
            recent_entries_count: 1000,
            clean_tokens: false,
        }
    }
}

impl TokenDiscoveryConfig {
    pub fn token_selection_mode(&self) -> TokenSelectionMode {
        match self.selection_mode {
            SelectionMode::Threshold => TokenSelectionMode::Threshold(self.min_occurrence_ratio),
            SelectionMode::MinTokenCount => TokenSelectionMode::MinTokenCount(self.min_token_count),
        }
    }
}

pub fn config() -> &'static TokenDiscoveryConfig {
    CONFIG.get_or_init(|| {
        let cfg = fs::read_to_string("token_config.yaml")
            .map_err(|e| eprintln!("Failed to load config file: {e}, using default."))
            .ok()
            .and_then(|s| {
                serde_yaml::from_str(&s)
                    .map_err(|e| eprintln!("Failed to parse config file: {e}, using default."))
                    .ok()
            })
            .unwrap_or_default();
        println!("Loaded config: {:?}", cfg);
        cfg
    })
}