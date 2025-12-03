use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;

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

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExtractorConfig {
    Corpus,
    MutationDelta,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProcessorConfig {
    Sais {
        min_len: usize,
        max_len: usize,
        #[serde(default)]
        threshold: Option<f64>,
        #[serde(default)]
        token_count: Option<usize>,
    },
    FilterNullBytes {
        max_ratio: f64,
    },
    StripBytes {
        bytes: Vec<u8>,
        min_length: usize,
    },
    RemoveSubstrings,
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

    // Strategy config
    pub extractor: ExtractorConfig,
    pub pipeline: Vec<ProcessorConfig>,
}

impl TokenDiscoveryConfig {
    pub fn validate(&self) {
        // Pipeline is now flexible - validation could check for empty pipeline, etc.
        if self.pipeline.is_empty() && !matches!(self.fuzzer_preset, FuzzerPreset::Baseline) {
            println!("Warning: empty pipeline configured");
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
