use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;

use crate::strategies::Strategy;

static CONFIG: OnceLock<TokenDiscoveryConfig> = OnceLock::new();

#[derive(Deserialize, Debug, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum StagePreset {
    #[default]
    Baseline,
    SuffixArray,
    MutationDelta,
}

#[derive(Deserialize, Debug, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum MutatorPreset {
    #[default]
    Standard,
    TokenPreserving,
}

#[derive(Deserialize, Debug, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum MutationsPreset {
    #[default]
    Havoc,
    HavocWithTokens,
    HavocWithSmartTokens,
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

#[derive(Deserialize, Debug, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum CorpusPreset {
    #[default]
    InMemory,
    OnDisk,
    InMemoryOnDisk,
    Cached,
}

#[derive(Deserialize, Debug)]
pub struct TokenDiscoveryConfig {
    // Presets
    pub stage_preset: StagePreset,
    pub mutator_preset: MutatorPreset,
    pub mutations_preset: MutationsPreset,
    pub scheduler_preset: SchedulerPreset,
    pub corpus_preset: CorpusPreset,

    // Objective
    pub enable_timeout_objective: bool,

    // Paths
    pub corpus_dir: String,
    pub crashes_dir: String,

    // Fuzzer settings
    pub broker_port: u16,
    pub timeout_secs: u64,
    pub iterations: u64,

    // Token discovery settings
    pub min_corpus_size: usize,
    pub search_interval: u32,
    pub max_tokens: usize,
    pub max_token_length: usize,
    pub min_token_length: usize,
    pub search_pool_size: usize,

    // Strategy config
    pub strategy: Strategy,
}

impl TokenDiscoveryConfig {
    pub fn validate(&self) {
        // TokenPreserving requires token mutations
        if matches!(self.mutator_preset, MutatorPreset::TokenPreserving)
            && matches!(self.mutations_preset, MutationsPreset::Havoc)
        {
            panic!(
                "Invalid config: mutator_preset: token_preserving requires token mutations.\n\
                 Fix: Change 'mutations_preset: havoc' to 'mutations_preset: havoc_with_tokens' or 'mutations_preset: havoc_with_smart_tokens'"
            );
        }

        // Token discovery stages require token mutations
        if matches!(self.stage_preset, StagePreset::SuffixArray | StagePreset::MutationDelta)
            && matches!(self.mutations_preset, MutationsPreset::Havoc)
        {
            panic!(
                "Invalid config: stage_preset: {:?} requires token mutations.\n\
                 Fix: Change 'mutations_preset: havoc' to 'mutations_preset: havoc_with_tokens' or 'mutations_preset: havoc_with_smart_tokens'",
                self.stage_preset
            );
        }

        // Standard mutator with SmartTokens is wasteful (warning only)
        if matches!(self.mutator_preset, MutatorPreset::Standard)
            && matches!(self.mutations_preset, MutationsPreset::HavocWithSmartTokens)
        {
            panic!(
                "Warning: mutations_preset: havoc_with_smart_tokens works better with mutator_preset: token_preserving.\n\
                 Consider: Change 'mutator_preset: standard' to 'mutator_preset: token_preserving'"
            );
        }
    }
}

pub fn config() -> &'static TokenDiscoveryConfig {
    CONFIG.get_or_init(|| {
        let cfg: TokenDiscoveryConfig = fs::read_to_string("token_config.yaml")
            .map_err(|e| panic!("Failed to load token_config.yaml: {e}"))
            .and_then(|s| {
                serde_yaml::from_str(&s)
                    .map_err(|e| panic!("Failed to parse token_config.yaml: {e}"))
            })
            .unwrap();

        cfg.validate();
        cfg
    })
}