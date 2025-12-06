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

fn default_curve() -> f64 { 1.0 } //default = linear

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThresholdFunction {
    Fixed { value: f64 },
    Interpolated {
        min_threshold: f64,  // at max_len
        max_threshold: f64,  // at min_len
        #[serde(default = "default_curve")]
        curve: f64,
    },
}

impl ThresholdFunction {
    pub fn compute(&self, token_len: usize, min_len: usize, max_len: usize) -> f64 {
        match self {
            Self::Fixed { value } => *value,
            Self::Interpolated { min_threshold, max_threshold, curve } => {
                let len = token_len.clamp(min_len, max_len);
                let t = (len - min_len) as f64 / (max_len - min_len) as f64;
                let curved_t = t.powf(*curve);
                max_threshold - curved_t * (max_threshold - min_threshold)
            }
        }
    }
}


#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProcessorConfig {
    FilterNullBytes {
        max_ratio: f64,
    },
    RemoveRepetitive {
        threshold: f64,
    },
    RemoveSimilar {
        threshold: f64,
        keep_longer: bool,
    },
    RemoveSubstrings,
    Sais {
        #[serde(default)]
        min_len: Option<usize>,
        #[serde(default)]
        max_len: Option<usize>,
        #[serde(default)]
        threshold: Option<f64>,
        #[serde(default)]
        token_count: Option<usize>,
        #[serde(default)]
        threshold_fn: Option<ThresholdFunction>,
    },
    SplitAt {
        delimiters: Vec<Vec<u8>>,
        #[serde(default)]
        min_length: Option<usize>,
    },
    StripBytes {
        bytes: Vec<u8>,
        #[serde(default)]
        min_length: Option<usize>,
    },
}

#[derive(Deserialize, Debug)]
pub struct TokenDiscoveryConfig {
    // runtime settings
    pub cores: String,

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
    pub displayed_tokens: usize,

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
