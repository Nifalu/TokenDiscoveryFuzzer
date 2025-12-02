use serde::Deserialize;

use libafl::corpus::HasCurrentCorpusId;
use libafl::events::EventFirer;
use libafl::executors::{Executor, HasObservers};
use libafl::inputs::HasTargetBytes;
use libafl::observers::MapObserver;
use libafl::schedulers::TestcaseScore;
use libafl::stages::mutational::MutatedTransform;
use libafl::state::{HasCorpus, HasCurrentTestcase, HasExecutions, HasRand, MaybeHasClientPerfMonitor};
use libafl::{Evaluator, HasMetadata, HasNamedMetadata};
use libafl_bolts::tuples::{Handled, MatchNameRef, Handle};

pub mod sais;
mod mdelta;

pub use sais::SuffixArrayConfig;
pub use mdelta::MutationDeltaConfig;

/// Enum wrapper for config deserialization
/// Each variant holds a config that implements TokenDiscoveryStrategy
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Strategy {
    SuffixArray(SuffixArrayConfig),
    MutationDelta(MutationDeltaConfig),
    // Add new strategies here:
    // Ngram(NgramConfig),
}

impl Strategy {
    /// Dispatches to the actual strategy implementation
    pub fn discover_tokens<E, EM, I, S, F, C, Z, O>(
        &self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
        observer_handle: &Handle<C>
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
        F: TestcaseScore<I, S>,
        C: Handled + AsRef<O> + AsMut<O>,
        Z: Evaluator<E, EM, I, S>,
        O: MapObserver,
    {
        match self {
            Strategy::SuffixArray(cfg) => {
                SuffixArrayConfig::discover_tokens(cfg, state)
            }

            Strategy::MutationDelta(cfg) => {
                MutationDeltaConfig::discover_tokens::<E, EM, I, S, F, C, Z, O>(
                    cfg, fuzzer, executor, state, manager, observer_handle
                )
            }
            // Strategy::Ngram(cfg) => { ... }
        }
    }
}