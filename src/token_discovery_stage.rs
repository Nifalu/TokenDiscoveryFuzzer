use std::{borrow::Cow, collections::HashSet, marker::PhantomData};
use std::cmp::{max, min};
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

pub const STAGE_NAME: &str = "TokenDiscoveryStage";
const MAXTOKENLENGTH: usize = 6; // in bytes
const MINTOKENLENGTH: usize = 2; // in bytes

pub struct TokenDiscoveryStage<E, EM, I, S, M, F, C, Z, O>{
    name: Cow<'static, str>,
    mutator: M,
    stage_executions: u32, // how many times this stage has been called/executed
    observer_handle: Handle<C>,
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

        /* Remove duplicate tokens and get the current testcase (input) */
        self.stage_executions += 1;

        println!("called perform!");

        self.clean_tokens(state);

        let input = {
            let mut testcase = state.current_testcase_mut()?;
            match I::try_transform_from(&mut testcase, state).ok() {
                Some(i) => i,
                None => return Ok(()),
            }
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
            observer_handle: observer.handle(),
            phantom: PhantomData,
        }
    }

    /**
    Search for Tokens within the mutated input.

    When a mutation leads to more coverage, the fuzzer will treat it as interesting and add it
    to the corpus. This algorithm works on the assumption that the mutated_input has a different
    coverage than the original input. It tries to figure out, what parts of the mutated_input
    are responsible for the change.

    General idea of the algorithm:
    Add individual bytes from the mutated input one by one to the original input (test sequence).
    1. At some point the coverage map should change, which means whatever caused the change
    is now inside our test sequence.
    As soon as this is the case, set that position as upper_bound.
    2. Now undo the mutations or change bytes from the left again in order to catch bytes which
    are not necessary for the coverage map to change. (adjust lower bound)
    3. The lower bound should now start at the point where the relevant subsequence starts.
    But the upper bound is at the position of the last relevant mutation, the relevant section
    could be further to the right tough. So we have to verify some bytes to the right.
     */
    pub fn search_tokens(
        &mut self,
        original_input: &I,
        corpus_id: CorpusId,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
    ) -> Result<(), Error> {
        /* Get the mutated testcase */
        let Ok(mutated_input) = I::try_transform_from(
            &mut *state.corpus().get(corpus_id)?.borrow_mut(),
            state
        ) else {
            return Ok(());  // Skip gracefully if transformation fails
        };

        /* convert the inputs into bytes and calculate indices of differences */
        let original_seq = original_input.target_bytes().to_vec();
        let mutated_seq = mutated_input.target_bytes().to_vec();
        let mut test_seq = original_seq.clone();

        /* Implicitly there must be a difference because the mutator will always mutate something.
        if original_bytes.as_ref() == mutated_bytes.as_ref() {
            return Ok(()) // Can't find a token without a mutation
        }
        */

        let baseline_coverage = self.get_input_coverage(
            original_input, fuzzer, executor, state, manager)?;
        let mut lower_bound : usize= 0;
        let mut upper_bound : usize = 0;


        /* add mutations until we get more coverage (move upper bound to the right) */
        //TODO how do we handle the case when the mutator increased the length of the vector?
        //Dirty fix:
        if test_seq.len() < mutated_seq.len() {
            println!("mutated sequence is longer than test sequence");
            return Ok(())
        }

        for i in 0..mutated_seq.len() {
            test_seq[i] = mutated_seq[i];

            let coverage = self.get_input_coverage(
                &test_seq.clone().into(),
                fuzzer, executor, state, manager)?;

            if coverage != baseline_coverage {
                upper_bound = i;
                break;
            }
        }

        /* remove mutations and check non-mutations from the left (move lower bound to the right) */
        for i in max(0, upper_bound-MAXTOKENLENGTH)..upper_bound {
            let tmp = test_seq[i]; // cache current value
            if test_seq[i] == original_seq[i] {
                test_seq[i] = 255 // put a random u8 here.
            } else {
                test_seq[i] = original_seq[i]
            }
            let coverage = self.get_input_coverage(
                &test_seq.clone().into(),
                fuzzer, executor, state, manager)?;

            if coverage != baseline_coverage {
                lower_bound = i;
                test_seq[i] = tmp; // restore previous value
                break;
            }
        }

        for i in upper_bound+1..min(mutated_seq.len(), lower_bound+MAXTOKENLENGTH) {
            let tmp = test_seq[i];
            test_seq[i] = 255; // put a random u8 here

            let coverage = self.get_input_coverage(
                &test_seq.clone().into(),
                fuzzer, executor, state, manager)?;

            if coverage != baseline_coverage {
                upper_bound = i;
                test_seq[i] = tmp;
                break;
            }
        }

        /* Add the token to the fuzzer */
        let token_length = upper_bound - lower_bound;
        if token_length >= MINTOKENLENGTH {
            let token = test_seq[lower_bound..upper_bound].to_vec();
            let token_data = state.metadata_mut::<Tokens>()?;
            if !token_data.contains(&token) {
                token_data.add_token(&token);
                println!("Token of length {}B added:", token.len());
                println!("  Decimal: {:?}", token);
                println!("  Hex:     {:02x?}", token);
                let ascii = String::from_utf8_lossy(&token);
                println!("  ASCII:   {}", ascii);
            }
        }

        Ok(())

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

    fn get_input_coverage(
        &mut self,
        input: &I,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
    ) -> Result<Vec<O::Entry>, Error> {

        /* reset the observer map */
        let mut observers = executor.observers_mut();
        let edge_observer = observers
            .get_mut(&self.observer_handle)
            .ok_or_else(|| Error::key_not_found("MapObserver not found".to_string()))?
            .as_mut();
        edge_observer.reset_map()?;

        /* run input target */
        executor.run_target(fuzzer, state, manager, input)?;

        /* observe */
        let coverage = {
            let observers = executor.observers();
            let edge_observer = observers
                .get(&self.observer_handle)
                .ok_or_else(|| Error::key_not_found("MapObserver not found".to_string()))?
                .as_ref();
            edge_observer.to_vec()
        };

        Ok(coverage)

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