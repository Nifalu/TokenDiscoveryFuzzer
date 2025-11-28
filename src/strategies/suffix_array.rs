use serde::Deserialize;

use libafl::corpus::{Corpus, HasCurrentCorpusId};
use libafl::events::EventFirer;
use libafl::executors::{Executor, HasObservers};
use libafl::inputs::HasTargetBytes;
use libafl::mutators::Mutator;
use libafl::observers::MapObserver;
use libafl::schedulers::TestcaseScore;
use libafl::stages::mutational::MutatedTransform;
use libafl::state::{HasCorpus, HasCurrentTestcase, HasExecutions, HasRand, MaybeHasClientPerfMonitor};
use libafl::{Evaluator, HasMetadata, HasNamedMetadata};
use libafl_bolts::tuples::{Handled, MatchNameRef};

use super::TokenDiscoveryStrategy;
use crate::config::config;
use crate::common_substring_discovery::{find_common_substrings, TokenSelectionMode};

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
pub struct SuffixArrayStrategy {
    pub passes: Vec<PatternPass>,
    #[serde(default)]
    pub strip_bytes: Vec<u8>,
    pub max_null_ratio: Option<f64>,
    pub remove_substrings: bool,
}

impl SuffixArrayStrategy {
    fn iterative_token_discovery(&self, corpus: &[Vec<u8>]) -> Option<Vec<Vec<u8>>> {
        if self.passes.is_empty() || corpus.is_empty() {
            return None;
        }

        let mut current_data = corpus.to_vec();

        for (i, pass) in self.passes.iter().enumerate() {
            let result = find_common_substrings(
                &current_data,
                pass.min_length,
                pass.max_length,
                pass.selection_mode(),
                &self.strip_bytes,        // from self
                self.max_null_ratio,      // from self
                self.remove_substrings,   // from self
            )?;

            println!(
                "Pass {}: {} -> {} tokens (len {}-{}, threshold: {:.1}%, min {}/{})",
                i + 1,
                current_data.len(),
                result.tokens.len(),
                pass.min_length,
                pass.max_length,
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

impl TokenDiscoveryStrategy for SuffixArrayStrategy {
    fn discover_tokens<E, EM, I, S, M, F, C, Z, O>(
        &self,
        _fuzzer: &mut Z,
        _executor: &mut E,
        state: &mut S,
        _manager: &mut EM,
    ) -> Option<Vec<Vec<u8>>>
    where
        E: Executor<EM, I, S, Z> + HasObservers,
        E::Observers: MatchNameRef,
        EM: EventFirer<I, S>,
        I: MutatedTransform<I, S> + Clone + From<Vec<u8>> + HasTargetBytes,
        S: HasCorpus<I>
        + HasMetadata
        + MaybeHasClientPerfMonitor
        + HasCurrentTestcase<I>
        + HasRand
        + HasExecutions
        + HasNamedMetadata
        + HasCurrentCorpusId,
        M: Mutator<I, S>,
        F: TestcaseScore<I, S>,
        C: Handled + AsRef<O> + AsMut<O>,
        Z: Evaluator<E, EM, I, S>,
        O: MapObserver,
    {
        let cfg = config();

        let corpus: Vec<Vec<u8>> = state
            .corpus()
            .ids()
            .rev()
            .take(cfg.recent_entries_count)
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
}

