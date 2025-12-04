use std::{borrow::Cow, marker::PhantomData};
use libafl::{
    corpus::{Corpus, HasCurrentCorpusId},
    events::EventFirer,
    executors::{Executor, HasObservers},
    inputs::HasTargetBytes,
    observers::MapObserver,
    stages::{mutational::MutatedTransform, Restartable, RetryCountRestartHelper, Stage},
    state::{HasCorpus, HasCurrentTestcase, HasRand, MaybeHasClientPerfMonitor},
    Error,
    HasMetadata,
    HasNamedMetadata
};
use libafl_bolts::{tuples::{Handled, MatchNameRef}, Named};

use crate::config::config;
use crate::extractors::Extractor;
use crate::processors::Processor;
use crate::smart_token_mutations::SmartTokens;

pub const STAGE_NAME: &str = "TokenDiscoveryStage";

pub struct TokenDiscoveryStage<E, EM, I, S, Z, C, O> {
    name: Cow<'static, str>,
    last_corpus_size: usize,
    extractor: Extractor<C>,
    processors: Vec<Box<dyn Processor>>,
    stage_calls: u32,
    phantom: PhantomData<(E, EM, I, S, Z, O)>,
}

impl<E, EM, I, S, Z, C, O> Stage<E, EM, S, Z> for TokenDiscoveryStage<E, EM, I, S, Z, C, O>
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
    + HasNamedMetadata
    + HasCurrentCorpusId,
    C: Handled + AsRef<O> + AsMut<O>,
    O: MapObserver,
{
    fn perform(
        &mut self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
    ) -> Result<(), Error> {
        let cfg = config();

        self.stage_calls += 1;
        let current_corpus_size = state.corpus().count();


        // Don't run this stage if:
        // - Corpus size has changed since last execution
        // - Not enough executions have passed since last run
        // - Corpus size is below minimum threshold
        if self.last_corpus_size != current_corpus_size
            || self.stage_calls % cfg.search_interval != 0
            || cfg.min_corpus_size > current_corpus_size
        {
            self.last_corpus_size = current_corpus_size;
            return Ok(());
        }

        // 1. Extract initial data
        let mut data = match self.extractor.extract::<E, EM, I, S, Z, O>(fuzzer, executor, state, manager) {
            Some(d) => d,
            None => return Ok(()),
        };

        // 2. Run through pipeline
        for proc in &self.processors {
            data = match proc.process(data) {
                Some(d) => d,
                None => return Ok(()),
            };
        }

        // 3. Add to SmartTokens
        if let Ok(token_meta) = state.metadata_mut::<SmartTokens>() {
            token_meta.add_tokens(&data);
        }

        Ok(())
    }
}

impl<E, EM, I, S, Z, C, O> TokenDiscoveryStage<E, EM, I, S, Z, C, O> {
    pub fn new(extractor: Extractor<C>, processors: Vec<Box<dyn Processor>>) -> Self {
        Self {
            name: Cow::Borrowed(STAGE_NAME),
            last_corpus_size: 0,
            extractor,
            processors,
            stage_calls: 0,
            phantom: PhantomData,
        }
    }
}

impl<E, EM, I, S, Z, C, O> Restartable<S> for TokenDiscoveryStage<E, EM, I, S, Z, C, O>
where
    S: HasMetadata + HasNamedMetadata + HasCurrentCorpusId,
{
    fn should_restart(&mut self, state: &mut S) -> Result<bool, Error> {
        RetryCountRestartHelper::should_restart(state, &self.name, 3)
    }

    fn clear_progress(&mut self, state: &mut S) -> Result<(), Error> {
        RetryCountRestartHelper::clear_progress(state, &self.name)
    }
}

impl<E, EM, I, S, Z, C, O> Named for TokenDiscoveryStage<E, EM, I, S, Z, C, O> {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}