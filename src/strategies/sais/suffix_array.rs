use serde::Deserialize;

use libafl::corpus::Corpus;
use libafl::inputs::HasTargetBytes;
use libafl::state::HasCorpus;


use crate::config::config;
use super::common_substring_discovery::{find_common_substrings, TokenSelectionMode};


#[derive(Deserialize, Debug, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    #[default]
    Threshold,
    MinTokenCount,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PatternPass {
    pub mode: SelectionMode,
    #[serde(default)]
    pub threshold: f64,
    #[serde(default)]
    pub token_count: usize,
    // Optional per-pass override
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
}

impl PatternPass {
    pub fn selection_mode(&self) -> TokenSelectionMode {
        match self.mode {
            SelectionMode::Threshold => TokenSelectionMode::Threshold(self.threshold),
            SelectionMode::MinTokenCount => TokenSelectionMode::MinTokenCount(self.token_count),
        }
    }

    pub fn min_length(&self, default: usize) -> usize {
        self.min_length.unwrap_or(default)
    }

    pub fn max_length(&self, default: usize) -> usize {
        self.max_length.unwrap_or(default)
    }
}

#[derive(Deserialize, Debug)]
pub struct SuffixArrayConfig {
    pub passes: Vec<PatternPass>,
}

impl SuffixArrayConfig<> {
    pub fn discover_tokens<I, S>(&self, state: &S) -> Option<Vec<Vec<u8>>>
    where
        I: HasTargetBytes + Clone,
        S: HasCorpus<I>,
    {
        let cfg = config();

        let corpus: Vec<Vec<u8>> = state
            .corpus()
            .ids()
            .rev()
            .take(cfg.search_pool_size)
            .filter_map(|id| {
                state
                    .corpus()
                    .cloned_input_for_id(id)
                    .ok()
                    .map(|input| input.target_bytes().to_vec())
            })
            .collect();

        if corpus.is_empty() {
            return None;
        }

        self.iterative_token_discovery(&corpus)
    }



    fn iterative_token_discovery(&self, corpus: &[Vec<u8>]) -> Option<Vec<Vec<u8>>> {
        let cfg = config();

        if self.passes.is_empty() || corpus.is_empty() {
            return None;
        }

        let mut current_data = corpus.to_vec();

        for (i, pass) in self.passes.iter().enumerate() {
            let min_len = pass.min_length(cfg.min_token_length);
            let max_len = pass.max_length(cfg.max_token_length);

            let result = find_common_substrings(
                &current_data,
                min_len,
                max_len,
                pass.selection_mode(),
                &cfg.strip_bytes,
                cfg.max_null_ratio,
                cfg.remove_substrings,
            )?;

            println!(
                "Pass {}: {} -> {} tokens (len {}-{}, threshold: {:.1}%, min {}/{})",
                i + 1,
                current_data.len(),
                result.tokens.len(),
                min_len,
                max_len,
                result.threshold_percentage * 100.0,
                result.threshold_absolute,
                current_data.len()
            );

            if result.tokens.is_empty() {
                return None;
            }

            current_data = result.tokens;
        }

        Some(current_data)
    }
}

