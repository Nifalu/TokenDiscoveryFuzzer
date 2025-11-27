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
use crate::config::config;
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

        /* get the current testcase and extract appended tokens if there are any */
        //TODO: We share tokens with testcases but we dont retrieve them here.
        let input = {
            let mut testcase = state.current_testcase_mut()?;
            let input = match I::try_transform_from(&mut testcase, state).ok() {
                Some(i) => i,
                None => return Ok(()),
            };
            input
        };

        /* The more interesting the current testcase is, the more often it should be mutated */
        let num_iterations = self.iterations(state)?;

        /* Apply mutations to the input and extend the corpus with interesting mutations */
        let mut interesting_corpora: HashSet<CorpusId> = HashSet::new();
        for _ in 0..num_iterations {
            /* Each mutation is run against the fuzzing target */
            let corpus_id = self.mutate_and_evaluate(input.clone(), fuzzer, executor, state, manager)?;
            interesting_corpora.extend(corpus_id); // Iterator won't push None values
        }

        fn n_recent_corpus_entries<C, I>(corpus: &C, n: usize) -> Vec<Vec<u8>>
        where
            C: Corpus<I>,
            I: HasTargetBytes + Clone,
        {
            corpus
                .ids()
                .rev()  // Start from last (most recent)
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

                let result = find_common_substrings(
                    &sequences,
                    cfg.min_token_length,
                    cfg.max_token_length,
                    cfg.token_selection_mode(),
                );

                if let Some(result) = result {
                    if !result.tokens.is_empty() {
                        // 1. Strip control characters
                        let cleaned: Vec<Vec<u8>> = result.tokens
                            .into_iter()
                            .map(|t| {
                                if cfg.clean_tokens {
                                    self.clean_tokens(&t)
                                } else {
                                    t
                                }
                            })
                            .filter(|t| t.len() >= cfg.min_token_length)
                            .collect();

                        // 2. Remove substrings (keeps longest)
                        let mut deduped = self.remove_substrings(cleaned);

                        // 3. Sort by length (longest first) for display
                        deduped.sort_by(|a, b| b.len().cmp(&a.len()));

                        // 4. Add tokens
                        if let Ok(token_meta) = state.metadata_mut::<SmartTokens>() {
                            for token in &deduped {
                                token_meta.add_token(token);
                            }
                        }

                        println!(
                            "Discovered {} tokens (threshold: {:.1}%, {} of {} inputs):",
                            deduped.len(),
                            result.threshold_percentage * 100.0,
                            result.threshold_absolute,
                            sequences.len()
                        );
                        for token in deduped.iter().take(25) {
                            println!("  [{:2}] {:?} | {:02x?}", token.len(), String::from_utf8_lossy(token), token);
                        }
                        if deduped.len() > 25 {
                            println!("  ... and {} more", deduped.len() - 25);
                        }
                    }
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

    fn clean_tokens(
        &mut self,
        token: &[u8]) -> Vec<u8>
    {
        let is_control = |b: &u8| *b < 0x20 || *b == 0x7F;

        let start = token.iter().position(|b| !is_control(b)).unwrap_or(token.len());
        let end = token.iter().rposition(|b| !is_control(b)).map(|i| i + 1).unwrap_or(0);

        if start < end {
            token[start..end].to_vec()
        } else {
            Vec::new()
        }
    }

    fn remove_substrings(
        &mut self,
        tokens: Vec<Vec<u8>>) -> Vec<Vec<u8>>
    {
        let mut sorted = tokens;
        sorted.sort_by(|a, b| b.len().cmp(&a.len())); // longest first

        let mut result: Vec<Vec<u8>> = Vec::new();
        for token in sorted {
            let is_substring = result.iter().any(|existing|
                existing.windows(token.len()).any(|w| w == token.as_slice())
            );
            if !is_substring {
                result.push(token);
            }
        }
        result
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