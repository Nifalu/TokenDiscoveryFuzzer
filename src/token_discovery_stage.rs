use std::{borrow::Cow, collections::HashSet, marker::PhantomData};
use libafl::{
    corpus::{Corpus, CorpusId, HasCurrentCorpusId},
    events::EventFirer,
    executors::{Executor, HasObservers},
    inputs::HasTargetBytes,
    mutators::{MutationResult, Mutator},
    observers::MapObserver,
    schedulers::TestcaseScore,
    stages::{mutational::{MutatedTransform, MutatedTransformPost},
             MutationalStage, Restartable, RetryCountRestartHelper, Stage},
    state::{HasCorpus, HasCurrentTestcase, HasExecutions, HasRand, MaybeHasClientPerfMonitor},
    Error,
    Evaluator,
    HasMetadata,
    HasNamedMetadata
};
use libafl_bolts::{tuples::{Handle, Handled, MatchNameRef}, Named};


use crate::smart_token_mutations::SmartTokens;
use crate::common_substring_discovery::find_common_substrings;
use crate::config::{config, TokenDiscoveryConfig};
pub const STAGE_NAME: &str = "TokenDiscoveryStage";

pub struct TokenDiscoveryStage<E, EM, I, S, M, F, C, Z, O>{
    name: Cow<'static, str>,
    mutator: M,
    stage_executions: u32, // how many times this stage has been called/executed
    _observer_handle: Handle<C>,
    phantom: PhantomData<(E, EM, I, S, F, Z, O)>
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

        self.stage_executions += 1;

        let input = {
            let mut testcase = state.current_testcase_mut()?;
            let input = match I::try_transform_from(&mut testcase, state).ok() {
                Some(i) => i,
                None => return Ok(()),
            };
            input
        };

        let num_iterations = self.iterations(state)?;

        let mut interesting_corpora: HashSet<CorpusId> = HashSet::new();
        for _ in 0..num_iterations {
            let corpus_id = self.mutate_and_evaluate(input.clone(), fuzzer, executor, state, manager)?;
            interesting_corpora.extend(corpus_id);
        }

        fn n_recent_corpus_entries<C, I>(corpus: &C, n: usize) -> Vec<Vec<u8>>
        where
            C: Corpus<I>,
            I: HasTargetBytes + Clone,
        {
            corpus
                .ids()
                .rev()
                .take(n)
                .filter_map(|id| {
                    corpus.cloned_input_for_id(id)
                        .ok()
                        .map(|input| input.target_bytes().to_vec())
                })
                .collect()
        }

        let cfg = config();
        if self.stage_executions % cfg.search_interval == 0 {
            if state.corpus().count() > cfg.min_corpus_size {
                let sequences = n_recent_corpus_entries(state.corpus(), cfg.recent_entries_count);

                let Some(tokens) = self.iterative_token_discovery(&sequences, &cfg) else {
                    return Ok(());
                };

                if tokens.is_empty() {
                    return Ok(());
                }

                if let Ok(token_meta) = state.metadata_mut::<SmartTokens>() {
                    for token in &tokens {
                        token_meta.add_token(token);
                    }
                }

                println!("Final: {} tokens after iterative discovery:", tokens.len());
                for token in tokens.iter().take(25) {
                    println!("  [{:2}] {:?} | {:02x?}", token.len(), String::from_utf8_lossy(token), token);
                }
                if tokens.len() > 25 {
                    println!("  ... and {} more", tokens.len() - 25);
                }
            }
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
            stage_executions: 0,
            _observer_handle: observer.handle(),
            phantom: PhantomData,
        }
    }


    pub fn iterative_token_discovery(
        &mut self,
        initial_corpus: &[Vec<u8>],
        cfg: &TokenDiscoveryConfig
    ) -> Option<Vec<Vec<u8>>> {
        if cfg.passes.is_empty() || initial_corpus.is_empty() {
            return None;
        }

        let mut current_data = initial_corpus.to_vec();

        for (i, pass) in cfg.passes.iter().enumerate() {
            let result = find_common_substrings(
                &current_data,
                pass.min_length,
                pass.max_length,
                pass.selection_mode(),
                &cfg.strip_bytes,
                cfg.max_null_ratio,
                cfg.remove_substrings
            )?;

            println!(
                "Pass {}: Refined from {} -> {} tokens (len {}-{}, threshold: {:.1}%, min {}/{} inputs)",
                i + 1,
                current_data.len(),
                result.tokens.len(),
                pass.min_length,
                pass.max_length,
                result.threshold_percentage * 100.0,
                result.threshold_absolute,
                current_data.len()
            );

            for token in result.tokens.iter().take(25) {
                //println!("  [{:2}] {:?} | {:02x?}", token.len(), String::from_utf8_lossy(token), token);
                println!("  [{:2}] {:?}", token.len(), String::from_utf8_lossy(token));
            }
            if result.tokens.len() > 25 {
                println!("  ... and {} more\n", result.tokens.len() - 25);
            } else {
                println!();
            }


            if result.tokens.is_empty() {
                return None;
            }

            current_data = result.tokens;
        }

        Some(current_data)
    }


    /**
    Mutate the input and run it once.
    If it was interesting, add it as new testcase to the corpus and return the id
    */
    fn mutate_and_evaluate(
        &mut self,
        mut input: I,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
    ) -> Result<Option<CorpusId>, Error> {

        // make the input mutable and mutate
        let mutation_result = self.mutator_mut().mutate(state, &mut input)?;

        if mutation_result == MutationResult::Skipped {
            return Ok(None);
        }

        // Pretty sure this does nothing when running with ByteInput...?
        let (untransformed, post) = input.try_transform_into(state)?;

        // check if mutated input is interesting
        let evaluation = fuzzer.evaluate_filtered(state, executor, manager, &untransformed)?;
        let (exec_result, corpus_id) = evaluation;

        if exec_result.is_solution() {
            println!("Found new solution persisting on disk");
        }

        // check for post process in the fuzzer
        self.mutator_mut().post_exec(state, corpus_id)?;
        post.post_exec(state, corpus_id)?;

        Ok(corpus_id)
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