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

#[derive(Deserialize, Debug, Clone)]
pub struct PatternPass {
    pub min_length: usize,
    pub max_length: usize,
    pub mode: SelectionMode,
    #[serde(default)]
    pub threshold: f64,
    #[serde(default)]
    pub token_count: usize,
}

impl PatternPass {
    pub fn selection_mode(&self) -> TokenSelectionMode {
        match self.mode {
            SelectionMode::Threshold => TokenSelectionMode::Threshold(self.threshold),
            SelectionMode::MinTokenCount => TokenSelectionMode::MinTokenCount(self.token_count),
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct TokenDiscoveryConfig {
    pub passes: Vec<PatternPass>,
    pub min_corpus_size: usize,
    pub search_interval: u32,
    pub max_tokens: usize,
    pub recent_entries_count: usize,
    pub strip_bytes: Vec<u8>,
    pub max_null_ratio: Option<f64>,
    pub remove_substrings: bool,
}

impl Default for TokenDiscoveryConfig {
    fn default() -> Self {
        Self {
            passes: vec![
                PatternPass {
                    min_length: 32,
                    max_length: 64,
                    mode: SelectionMode::MinTokenCount,
                    threshold: 0.0,
                    token_count: 50,
                },
                PatternPass {
                    min_length: 16,
                    max_length: 64,
                    mode: SelectionMode::Threshold,
                    threshold: 0.4,
                    token_count: 0,
                },
                PatternPass {
                    min_length: 4,
                    max_length: 64,
                    mode: SelectionMode::Threshold,
                    threshold: 0.3,
                    token_count: 0,
                },
            ],
            min_corpus_size: 250,
            search_interval: 2000,
            max_tokens: 2500,
            recent_entries_count: 2500,
            strip_bytes: (0x00..=0x1F).chain(std::iter::once(0x7F)).collect(),  // control chars
            max_null_ratio: Some(0.1),
            remove_substrings: true,
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