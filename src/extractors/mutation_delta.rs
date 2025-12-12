use std::cmp::min;

use libafl::corpus::Corpus;
use libafl::events::EventFirer;
use libafl::executors::{Executor, HasObservers};
use libafl::inputs::HasTargetBytes;
use libafl::observers::MapObserver;
use libafl::state::{HasCorpus, HasCurrentTestcase, HasRand};
use libafl::Error;
use libafl_bolts::rands::Rand;
use libafl_bolts::tuples::{Handle, Handled, MatchNameRef};

use crate::config::config;

pub struct MutationDeltaExtractor<C> {
    observer_handle: Handle<C>,
}

impl<C> MutationDeltaExtractor<C> {
    pub fn new(observer_handle: Handle<C>) -> Self {
        Self { observer_handle }
    }

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
        let cfg = config();

        let (mutated_bytes, parent_id) = {
            let current_testcase = state.current_testcase().ok()?;
            let mutated_input = current_testcase.input().as_ref()?;
            let bytes = mutated_input.target_bytes().to_vec();
            let parent_id = current_testcase.parent_id()?;
            (bytes, parent_id)
        };

        let original_bytes = {
            let original_testcase = state.corpus().get(parent_id).ok()?.borrow();
            let original_input = original_testcase.input().as_ref()?;
            original_input.target_bytes().to_vec()
        };

        let mut test_vec = original_bytes.clone();

        let original_map = self.get_coverage(&original_bytes.clone().into(), fuzzer, executor, state, manager).ok()?;
        let mutated_map = self.get_coverage(&mutated_bytes.clone().into(), fuzzer, executor, state, manager).ok()?;

        let mut left_bound = 0;
        let mut right_bound = 0;

        // Find right bound
        for i in 0..mutated_bytes.len() {
            if i >= test_vec.len() {
                test_vec.push(mutated_bytes[i]);
            } else {
                test_vec[i] = mutated_bytes[i];
            }

            let coverage = self.get_coverage(&test_vec.clone().into(), fuzzer, executor, state, manager).ok()?;
            if coverage == mutated_map {
                left_bound = i;
                right_bound = i + 1;
                break;
            }
        }

        // Extend right bound
        for i in right_bound..min(mutated_bytes.len(), left_bound + cfg.max_token_length) {
            if i >= test_vec.len() {
                break;
            }

            let tmp = test_vec[i];
            test_vec[i] = state.rand_mut().next() as u8;
            let coverage = self.get_coverage(&test_vec.clone().into(), fuzzer, executor, state, manager).ok()?;
            test_vec[i] = tmp;

            if coverage == mutated_map {
                right_bound = i;
                break;
            }
        }

        // Find left bound
        for i in right_bound.saturating_sub(cfg.max_token_length)..right_bound {
            let tmp = test_vec[i];
            test_vec[i] = if i >= original_bytes.len() || test_vec[i] == original_bytes[i] {
                state.rand_mut().next() as u8
            } else {
                original_bytes[i]
            };

            let coverage = self.get_coverage(&test_vec.clone().into(), fuzzer, executor, state, manager).ok()?;
            test_vec[i] = tmp;

            if coverage == original_map {
                left_bound = i;
                break;
            }
        }

        let token_length = right_bound - left_bound;
        if token_length >= cfg.min_token_length {
            if !cfg.silent_run  {
                println!(
                    "[{}] Found token of length {} at position {}",
                    self.name(),
                    token_length,
                    left_bound
                );
            }
            Some(vec![mutated_bytes[left_bound..right_bound].to_vec()])
        } else {
            None
        }
    }

    fn get_coverage<E, EM, I, S, Z, O>(
        &self,
        input: &I,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
    ) -> Result<Vec<O::Entry>, Error>
    where
        E: Executor<EM, I, S, Z> + HasObservers,
        E::Observers: MatchNameRef,
        C: Handled + AsRef<O> + AsMut<O>,
        O: MapObserver,
    {
        {
            let mut observers = executor.observers_mut();
            let edge_observer = observers
                .get_mut(&self.observer_handle)
                .ok_or_else(|| Error::key_not_found("MapObserver not found".to_string()))?
                .as_mut();
            edge_observer.reset_map()?;
        }

        executor.run_target(fuzzer, state, manager, input)?;

        let observers = executor.observers();
        let edge_observer = observers
            .get(&self.observer_handle)
            .ok_or_else(|| Error::key_not_found("MapObserver not found".to_string()))?
            .as_ref();

        Ok(edge_observer.to_vec())
    }

    pub fn name(&self) -> &'static str { "mutation_delta" }
}