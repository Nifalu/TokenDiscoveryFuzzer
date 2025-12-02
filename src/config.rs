use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;

use crate::strategies::Strategy;

static CONFIG: OnceLock<TokenDiscoveryConfig> = OnceLock::new();

#[derive(Deserialize, Debug, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum FuzzerPreset {
    #[default]
    Baseline,
    StandardTokens,
    PreservingTokens,
}

#[derive(Deserialize, Debug, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerPreset {
    #[default]
    Fast,
    Explore,
    Exploit,
    Coe,
    Lin,
    Quad,
}

#[derive(Deserialize, Debug)]
pub struct TokenDiscoveryConfig {
    // Main preset
    pub fuzzer_preset: FuzzerPreset,
    pub scheduler_preset: SchedulerPreset,

    // Paths
    pub corpus_dir: String,
    pub crashes_dir: String,

    // Fuzzer settings
    pub timeout_secs: u64,
    pub fuzz_loop_for: u64,

    // Token discovery settings
    pub min_corpus_size: usize,
    pub search_interval: u32,
    pub max_tokens: usize,
    pub max_token_length: usize,
    pub min_token_length: usize,
    pub search_pool_size: usize,

    // SAIS settings
    pub strip_bytes: Vec<u8>,
    pub max_null_ratio: Option<f64>,
    pub remove_substrings: bool,

    // Strategy config
    pub strategy: Strategy,
}

impl TokenDiscoveryConfig {
    pub fn validate(&self) {
        // Tokens1 and Tokens2 have no separate mutational stage
        // suffix_array strategy doesn't do mutations itself
        // This combination is invalid
        if matches!(self.fuzzer_preset, FuzzerPreset::StandardTokens | FuzzerPreset::PreservingTokens) {
            if matches!(self.strategy, Strategy::SuffixArray(_)) {
                panic!(
                    "Invalid config: fuzzer_preset '{}' has no separate mutational stage, \
                     but strategy 'suffix_array' does not perform mutations.\n\n\
                     Fix: Either change 'fuzzer_preset' to 'tokens1_plus' or 'tokens2_plus',\n\
                     or change 'strategy.type' to 'mutation_delta'.",
                    match self.fuzzer_preset {
                        FuzzerPreset::StandardTokens => "standard_token",
                        FuzzerPreset::PreservingTokens => "preserving_token",
                        _ => unreachable!(),
                    }
                );
            }
        }
    }
}

pub fn config() -> &'static TokenDiscoveryConfig {
    CONFIG.get_or_init(|| {
        let config_path = std::env::args()
            .nth(1)
            .unwrap_or_else(|| "token_config.yaml".to_string());

        let cfg: TokenDiscoveryConfig = fs::read_to_string(&config_path)
            .map_err(|e| panic!("Failed to load {}: {e}", config_path))
            .and_then(|s| {
                serde_yaml::from_str(&s)
                    .map_err(|e| panic!("Failed to parse {}: {e}", config_path))
            })
            .unwrap();

        cfg.validate();
        cfg
    })
}
