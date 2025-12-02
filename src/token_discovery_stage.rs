use std::{borrow::Cow, marker::PhantomData};
use libafl::{
    corpus::HasCurrentCorpusId,
    events::EventFirer,
    executors::{Executor, HasObservers},
    inputs::HasTargetBytes,
    observers::MapObserver,
    schedulers::TestcaseScore,
    stages::{mutational::MutatedTransform,
             Restartable, RetryCountRestartHelper, Stage},
    state::{HasCorpus, HasCurrentTestcase, HasExecutions, HasRand, MaybeHasClientPerfMonitor},
    Error,
    Evaluator,
    HasMetadata,
    HasNamedMetadata
};
use libafl::corpus::Corpus;
use libafl_bolts::{tuples::{Handle, Handled, MatchNameRef}, Named};


use crate::smart_token_mutations::SmartTokens;
use crate::config::config;
pub const STAGE_NAME: &str = "TokenDiscoveryStage";

pub struct TokenDiscoveryStage<E, EM, I, S, F, C, Z, O>{
    name: Cow<'static, str>,
    observer_handle: Handle<C>,
    phantom: PhantomData<(E, EM, I, S, F, Z, O)>,
    stage_executions: u32,
}

impl<E, EM, I, S, F, C, Z, O> Stage<E, EM, S, Z> for TokenDiscoveryStage<E, EM, I, S, F, C, Z, O>
where
    E:  Executor<EM, I, S, Z>
    +HasObservers,
    E::Observers: MatchNameRef,
    EM: EventFirer<I, S>,
    I:  MutatedTransform<I, S>
    +Clone
    +From<Vec<u8>>
    +HasTargetBytes,
    S:  HasCorpus<I>
    +HasMetadata
    +MaybeHasClientPerfMonitor
    +HasCurrentTestcase<I>
    +HasRand
    +HasExecutions
    +HasNamedMetadata
    +HasCurrentCorpusId,
    F:  TestcaseScore<I, S>,
    C:  Handled + AsRef<O> + AsMut<O>,
    Z:  Evaluator<E, EM, I, S>,
    O:  MapObserver,
{
    fn perform(
        &mut self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
    ) -> Result<(), Error> {

        let cfg = config();
        // Only perform the stage every N executions and ensure minimum corpus size
        self.stage_executions += 1;
        if self.stage_executions % cfg.search_interval != 0
            || cfg.min_corpus_size > state.corpus().count() {
            return Ok(())
        }
        // Discovery Tokens depending on the configured strategy
        // This allows to easily switch between strategies without recompiling
        let tokens = cfg.strategy.discover_tokens::<E, EM, I, S, F, C, Z, O>(
            fuzzer, executor, state, manager, &self.observer_handle
        );

        // Add discovered tokens to the SmartTokens metadata
        match tokens {
            Some(t) => {
                if let Ok(token_meta) = state.metadata_mut::<SmartTokens>() {
                    token_meta.add_tokens(&t);
                }
            },
            None => return Ok(())
        }

        Ok(())
    }
}

/*
=========================================================================================================
*/
impl <E, EM, I, S, F, C, Z, O> TokenDiscoveryStage<E, EM, I, S, F, C, Z, O>
where
    E:  Executor<EM, I, S, Z>
    +HasObservers,
    E::Observers: MatchNameRef,
    EM: EventFirer<I, S>,
    I:  MutatedTransform<I, S>
    +Clone
    +From<Vec<u8>>
    +HasTargetBytes,
    S:  HasCorpus<I>
    +HasMetadata
    +MaybeHasClientPerfMonitor
    +HasCurrentTestcase<I>
    +HasRand
    +HasExecutions
    +HasNamedMetadata
    +HasCurrentCorpusId,
    F:  TestcaseScore<I, S>,
    C:  Handled + AsRef<O> + AsMut<O>,
    Z:  Evaluator<E, EM, I, S>,
    O:  MapObserver,
{

    pub fn new(observer_handle: Handle<C>) -> Self {
        Self {
            name: Cow::Owned(STAGE_NAME.to_owned()),
            observer_handle,
            phantom: PhantomData,
            stage_executions: 0,
        }
    }
}

/*
=========================================================================================================
*/
impl<E, EM, I, S, F, C, Z, O> Restartable<S> for TokenDiscoveryStage<E, EM, I, S, F, C, Z, O>
where
    S: HasMetadata + HasNamedMetadata + HasCurrentCorpusId,
{
    fn should_restart(&mut self, state: &mut S) -> Result<bool, Error> {
        // Make sure we don't get stuck crashing on a single testcase
        RetryCountRestartHelper::should_restart(state, &self.name, 3)
    }

    fn clear_progress(&mut self, state: &mut S) -> Result<(), Error> {
        RetryCountRestartHelper::clear_progress(state, &self.name)
    }
}



impl<E, EM, I, S, F, C, Z, O> Named for TokenDiscoveryStage<E, EM, I, S, F, C, Z, O> {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}