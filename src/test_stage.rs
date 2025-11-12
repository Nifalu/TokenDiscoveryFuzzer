use std::{borrow::Cow, collections::HashSet, marker::PhantomData};

use libafl::{
    corpus::{Corpus, CorpusId, HasCurrentCorpusId},
    events::EventFirer,
    executors::{Executor, HasObservers},
    inputs::HasTargetBytes,
    mutators::{MutationResult, Mutator, Tokens},
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

pub const STAGE_NAME: &str = "Test Stage";
pub struct TestStage<E, EM, I, S, M, F, C, Z, O>{
    name: Cow<'static, str>,
    mutator: M,
    stage_executions: u32, // how many times this stage has been called/executed
    observer_handle: Handle<C>,
    phantom: PhantomData<(E, EM, I, S, F, Z, O)>
}

impl<E, EM, I, S, M, F, C, Z, O> Stage<E, EM, S, Z> for TestStage<E, EM, I, S, M, F, C, Z, O>
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


        /* Remove duplicate tokens and get the current testcase (input) */
        self.stage_executions += 1;
        self.clean_tokens(state);

        let Some(input) = self.current_testcase_as_input(state)? else {
            return Ok(());
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

        /* Search for new tokens */
        for id in interesting_corpora {
            self.search_tokens(&input, id, fuzzer, executor, state, manager)?;
        }

        /* print new tokens every N executions of this stage */
        if self.stage_executions % 1000 == 0 {
            let token_data = state.metadata_mut::<Tokens>()?;
            for token in token_data.iter() {
                println!("Token of length {}B found:", token.len());
                println!("  Decimal: {:?}", token);
                println!("  Hex:     {:02x?}", token);
                let ascii = String::from_utf8_lossy(token);
                println!("  ASCII:   {}", ascii);
            }

            /* Delete all tokens to make room for new ones */
            println!("\n == CLEARED {} TOKENS ==", token_data.len());
            let empty: Vec<Vec<u8>> = Vec::new();
            state.add_metadata(Tokens::from(empty));
        }
        Ok(())
    }
}

/*
=========================================================================================================
*/
impl <E, EM, I, S, M, F, C, Z, O> TestStage <E, EM, I, S, M, F, C, Z, O>
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

    pub  fn new(mutator: M, observer: &C) -> Self {
        Self {
            mutator,
            name: Cow::Owned(STAGE_NAME.to_owned()),
            stage_executions: 0,
            observer_handle: observer.handle(),
            phantom: PhantomData,
        }
    }

    pub fn search_tokens(
        &mut self,
        original: &I,
        corpus_id: CorpusId,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
    ) -> Result<(), Error>{

        /* get the mutated input */
        let Some(mutated) = self.mutated_testcase_as_input(state, corpus_id)? else {
            return Err(Error::empty("The mutated input could not be found"));
        };

        let input_bytes = original.target_bytes().clone();

        /* Find out which bits have changed during the mutation steps above */
        let diff_indices = self.search_diff_index(&original, &mutated);
        if !diff_indices.is_empty() {
            let mut seen_indices: HashSet<usize> = HashSet::new();

            /* Go through the indices that have changed*/
            for index in diff_indices {
                let token_data = state.metadata_mut::<Tokens>()?;

                /* if we found too many tokens we don't look for more? */
                if token_data.len() >= 100 {
                    return Ok(());
                }

                if index < 1 || seen_indices.contains(&index) {
                    continue;
                }

                seen_indices.insert(index);

                /* place the changed bit into the input byte vector */
                let mut raw_bytes = input_bytes.to_vec();
                let changed_byte = mutated.target_bytes()[index];
                raw_bytes[index] = changed_byte;

                /* get the coverage of the original input with just that one changed bit*/
                let raw_coverage = self.get_input_coverage(
                    &raw_bytes.clone().into(),
                    fuzzer,
                    executor,
                    state,
                    manager
                )?;

                /* prepare indices*/
                let mut analyze_bytes = input_bytes.to_vec();
                let mut left_index = index -1;
                let mut right_index = index + 1;

                /* iteratively move the left index to the left and compare the coverage */
                loop {

                    if left_index <= 0 || index - left_index + 1 >= 2 {
                        break;
                    }

                    seen_indices.insert(left_index);
                    let original_byte = analyze_bytes[left_index];
                    analyze_bytes[left_index] = changed_byte;
                    let left_coverage = self.get_input_coverage(
                        &analyze_bytes.clone().into(),
                        fuzzer,
                        executor,
                        state,
                        manager
                    )?;

                    if raw_coverage != left_coverage {
                        break;
                    }

                    analyze_bytes[left_index] = original_byte;
                    left_index -= 1;
                }

                /* iteratively move the right index to the right and compare the coverage */
                loop {

                    if right_index >= input_bytes.len() || right_index - index -1 >= 2{
                        break;
                    }

                    seen_indices.insert(right_index);
                    let original_byte = analyze_bytes[right_index];
                    analyze_bytes[right_index] = changed_byte;
                    let right_coverage = self.get_input_coverage(
                        &analyze_bytes.clone().into(),
                        fuzzer,
                        executor,
                        state,
                        manager
                    )?;

                    if raw_coverage != right_coverage {
                        break;
                    }

                    analyze_bytes[right_index] = original_byte;
                    right_index += 1;
                }

                /* extract the values at the indices of the discovered token and store it */
                let token = &input_bytes.clone()[left_index..right_index].to_vec();
                let token_data = state.metadata_mut::<Tokens>()?;
                if !token_data.contains(token) {
                    token_data.add_token(token);
                }
            }

        }
        Ok(())
    }

    /**
    Determine the indices of the bytes that have been mutated
    */
    pub fn search_diff_index(&self, original: &I, mutated: &I) -> Vec<usize> {
        // find diff between original and mutated
        let mut diffs = Vec::<usize>::new();
        let origin_bytes = original.target_bytes();
        let mutated_bytes = mutated.target_bytes();
        for (i, bytes) in origin_bytes.iter().zip(mutated_bytes.iter()).enumerate() {
            let (origin, mutated) = bytes;
            if origin != mutated {
                diffs.push(i);
            }
        }
        diffs
    }

    /**
    Get rid of duplicate tokens and tokens which are fully contained inside another token
    */
    pub fn clean_tokens(&self, state: &mut S) {
        let tokens_clone = state.metadata::<Tokens>().unwrap().clone();
        let mut unique: Vec<Vec<u8>> = Vec::new();

        'outer: for token in tokens_clone.iter() {
            for other in tokens_clone.iter() {
                if token == other {
                    continue;
                }

                // Only discard `token` if it is shorter and fully inside `other`
                if token.len() < other.len() &&
                    other.windows(token.len()).any(|w| w == token) {
                    continue 'outer; // discard token
                }
            }

            unique.push(token.clone());
        }

        state.add_metadata(Tokens::from(unique.into_boxed_slice()));
    }

    /**
    Get the current testcase from the corpus if transformation to <I> was successful.
    */
    fn current_testcase_as_input(&self, state: &mut S) -> Result<Option<I>, Error> {
        // transform Testcase<I> containing the input to I
        let mut testcase = state.current_testcase_mut()?;
        let Ok(input) = I::try_transform_from(&mut testcase, state) else {
            return Ok(None);
        };
        drop(testcase);
        Ok(Some(input))
    }


    fn mutated_testcase_as_input(
        &self,
        state: &mut S,
        corpus_id: CorpusId
    ) -> Result<Option<I>, Error> {
        let mut mutated_testcase = state.corpus().get(corpus_id)?.borrow_mut();
        Ok(I::try_transform_from(&mut mutated_testcase, state).ok())
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
    ) -> Result<Option<CorpusId>, Error>{

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

    fn get_input_coverage(
        &mut self,
        input: &I,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
    ) -> Result<Vec<O::Entry>, Error>{

        // enclose the reset in own scope for borrow checking
        {
            let mut observers = executor.observers_mut();
            let edge_observer = observers
                .get_mut(&self.observer_handle)
                .ok_or_else(|| Error::key_not_found("invariant: MapObserver not found".to_string()))?
                .as_mut();

            // reset to analyze trace between inputs
            edge_observer.reset_map()?;
        }

        // feedbacks not need analyzing traces
        let (_, _) = fuzzer.evaluate_filtered(state, executor, manager, input)?;
        let coverage_map: Vec<_>;
        {
            let mut observers = executor.observers_mut();
            let edge_observer = observers
                .get_mut(&self.observer_handle)
                .ok_or_else(|| Error::key_not_found("invariant: MapObserver not found".to_string()))?
                .as_mut();

            coverage_map = edge_observer.to_vec().clone();
        }

        Ok(coverage_map)
    }

}

/*
=========================================================================================================
*/
impl<E, EM, I, S, M, F, C, Z, O> Restartable<S> for TestStage<E, EM, I, S, M, F, C, Z, O>
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

impl <E, EM, I, S, M, F, C, Z, O> MutationalStage<S> for TestStage<E, EM, I, S, M, F, C, Z, O>
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

impl<E, EM, I, S, M, F, C, Z, O> Named for TestStage<E, EM, I, S, M, F, C, Z, O> {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}