use serde::Deserialize;
use std::{fs, process};
use std::sync::OnceLock;
use serde_json::Value;

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
    // System settings
    pub cores: String,
    pub broker_port: u16,
    pub prometheus_host: String,
    pub prometheus_port: u16,
    pub corpus_dir: String,
    pub crashes_dir: String,

    // Main preset
    pub fuzzer_preset: FuzzerPreset,
    pub scheduler_preset: SchedulerPreset,
    pub silent_run: bool,
    pub disable_multimonitor: bool,

    // Fuzzer settings
    pub timeout_secs: u64,

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

fn merge_json(base: &mut Value, override_val: &Value) {
    if let (Value::Object(base_map), Value::Object(override_map)) = (base, override_val) {
        for (key, value) in override_map {
            if let Some(base_value) = base_map.get_mut(key) {
                if base_value.is_object() && value.is_object() {
                    merge_json(base_value, value);
                } else {
                    *base_value = value.clone();
                }
            } else {
                base_map.insert(key.clone(), value.clone());
            }
        }
    }
}

fn find_default_config() -> Result<String, String> {
    let dir = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {e}"))?;

    // Check current dir
    let path = dir.join("default_config.json");
    if path.exists() {
        return fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()));
    }

    // Check parent dir
    if let Some(parent) = dir.parent() {
        let path = parent.join("default_config.json");
        if path.exists() {
            return fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {e}", path.display()));
        }
    }

    Err("default_config.json not found in current or parent directory".to_string())
}

fn exit_with_error(msg: &str) -> ! {
    eprintln!("Error: {msg}");
    process::exit(1);
}

pub fn config() -> &'static TokenDiscoveryConfig {
    CONFIG.get_or_init(|| {
        let config_path = std::env::args()
            .nth(1)
            .unwrap_or_else(|| exit_with_error("Usage: fuzzer <config.json>"));

        // Load default config (required)
        let default_str = find_default_config()
            .unwrap_or_else(|e| exit_with_error(&e));

        let mut base: Value = serde_json::from_str(&default_str)
            .unwrap_or_else(|e| exit_with_error(&format!("Invalid default_config.json: {e}")));

        // Load user config
        let user_str = fs::read_to_string(&config_path)
            .unwrap_or_else(|e| exit_with_error(&format!("Failed to load {config_path}: {e}")));

        let user_config: Value = serde_json::from_str(&user_str)
            .unwrap_or_else(|e| exit_with_error(&format!("Invalid JSON in {config_path}: {e}")));

        // Merge and deserialize
        merge_json(&mut base, &user_config);

        let cfg: TokenDiscoveryConfig = serde_json::from_value(base)
            .unwrap_or_else(|e| exit_with_error(&format!("Config error: {e}")));

        cfg.validate();
        cfg
    })
}
