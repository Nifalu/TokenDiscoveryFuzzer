use libafl::corpus::Corpus;
use libafl::inputs::HasTargetBytes;
use libafl::state::HasCorpus;

use crate::config::config;

pub struct CorpusExtractor;

impl CorpusExtractor {
    pub fn extract<I, S>(&self, state: &S) -> Option<Vec<Vec<u8>>>
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

        if corpus.is_empty() { None } else { Some(corpus) }
    }

    pub fn name(&self) -> &'static str { "corpus" }
}