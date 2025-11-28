use std::{borrow::Cow, marker::PhantomData};
use libafl::{
    corpus::HasCurrentCorpusId,
    events::EventFirer,
    executors::{Executor, HasObservers},
    inputs::HasTargetBytes,
    mutators::{Mutator},
    observers::MapObserver,
    schedulers::TestcaseScore,
    stages::{mutational::{MutatedTransform},
             MutationalStage, Restartable, RetryCountRestartHelper, Stage},
    state::{HasCorpus, HasCurrentTestcase, HasExecutions, HasRand, MaybeHasClientPerfMonitor},
    Error,
    Evaluator,
    HasMetadata,
    HasNamedMetadata
};
use libafl_bolts::{tuples::{Handle, Handled, MatchNameRef}, Named};


use crate::smart_token_mutations::SmartTokens;
use crate::config::{config, TokenDiscoveryConfig};
pub const STAGE_NAME: &str = "TokenDiscoveryStage";

pub struct TokenDiscoveryStage<E, EM, I, S, M, F, C, Z, O>{
    name: Cow<'static, str>,
    mutator: M,
    _observer_handle: Handle<C>,
    phantom: PhantomData<(E, EM, I, S, F, Z, O)>,
    stage_executions: u32, // how many times this stage has been called/executed
    cfg: &'static TokenDiscoveryConfig,
}

impl<E, EM, I, S, M, F, C, Z, O> Stage<E, EM, S, Z> for TokenDiscoveryStage<E, EM, I, S, M, F, C, Z, O>
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
    M:  Mutator<I, S>,
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

        // Only perform the stage every N executions
        self.stage_executions += 1;
        if self.stage_executions % self.cfg.search_interval != 0 {
            return Ok(())
        }

        // Discovery Tokens depending on the configured strategy
        // This allows to easily switch between strategies without recompiling
        let tokens = self.cfg.strategy.discover_tokens::<E, EM, I, S, M, F, C, Z, O>(
            fuzzer, executor, state, manager,
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
impl <E, EM, I, S, M, F, C, Z, O> TokenDiscoveryStage<E, EM, I, S, M, F, C, Z, O>
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
    M:  Mutator<I, S>,
    F:  TestcaseScore<I, S>,
    C:  Handled + AsRef<O> + AsMut<O>,
    Z:  Evaluator<E, EM, I, S>,
    O:  MapObserver,
{

    pub fn new(mutator: M, observer: &C) -> Self {
        Self {
            mutator,
            name: Cow::Owned(STAGE_NAME.to_owned()),
            _observer_handle: observer.handle(),
            phantom: PhantomData,
            stage_executions: 0,
            cfg: config()
        }
    }
}

/*
=========================================================================================================
*/
impl<E, EM, I, S, M, F, C, Z, O> Restartable<S> for TokenDiscoveryStage<E, EM, I, S, M, F, C, Z, O>
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

impl <E, EM, I, S, M, F, C, Z, O> MutationalStage<S> for TokenDiscoveryStage<E, EM, I, S, M, F, C, Z, O>
where
    S: HasCurrentTestcase<I>,
    F: TestcaseScore<I, S>
{
    type Mutator = M;

    fn mutator(&self) -> &Self::Mutator {
        &self.mutator
    }

    fn mutator_mut(&mut self) -> &mut Self::Mutator {
        &mut self.mutator
    }

    /**
    Calculates the score of the current testcase which determines how many times we should
    iterate/mutate this testcase. (higher scores mean more mutations will be done)
    */
    fn iterations(&self, state: &mut S) -> Result<usize, Error> {
        // Gets the current Testcase we are fuzzing (mut)
        let mut testcase = state.current_testcase_mut()?;
        // Computes the favor factor of a Testcase. Higher is better.
        let score = F::compute(state, &mut testcase)? as usize;

        Ok(score)
    }
}

impl<E, EM, I, S, M, F, C, Z, O> Named for TokenDiscoveryStage<E, EM, I, S, M, F, C, Z, O> {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}