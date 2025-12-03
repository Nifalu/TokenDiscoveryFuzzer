mod corpus;
mod mutation_delta;

pub use corpus::CorpusExtractor;
pub use mutation_delta::MutationDeltaExtractor;

use libafl::events::EventFirer;
use libafl::executors::{Executor, HasObservers};
use libafl::inputs::HasTargetBytes;
use libafl::observers::MapObserver;
use libafl::state::{HasCorpus, HasCurrentTestcase, HasRand};
use libafl_bolts::tuples::{Handled, MatchNameRef};

pub enum Extractor<C> {
    Corpus(CorpusExtractor),
    MutationDelta(MutationDeltaExtractor<C>),
}

impl<C> Extractor<C> {

    #[inline(always)]
    pub fn extract<E, EM, I, S, Z, O>(
        &self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
    ) -> Option<Vec<Vec<u8>>>
    where
        E: Executor<EM, I, S, Z> + HasObservers,
        E::Observers: MatchNameRef,
        EM: EventFirer<I, S>,
        I: Clone + From<Vec<u8>> + HasTargetBytes,
        S: HasCorpus<I> + HasCurrentTestcase<I> + HasRand,
        C: Handled + AsRef<O> + AsMut<O>,
        O: MapObserver,
    {
        match self {
            Extractor::Corpus(e) => e.extract::<I, S>(state),
            Extractor::MutationDelta(e) => e.extract::<E, EM, I, S, Z, O>(fuzzer, executor, state, manager),
        }
    }

    #[inline(always)]
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            Extractor::Corpus(e) => e.name(),
            Extractor::MutationDelta(e) => e.name(),
        }
    }
}