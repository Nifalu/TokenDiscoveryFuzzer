use std::cmp::min;
use serde::Deserialize;

use libafl::corpus::HasCurrentCorpusId;
use libafl::events::EventFirer;
use libafl::executors::{Executor, HasObservers};
use libafl::inputs::HasTargetBytes;
use libafl::mutators::{MutationResult, Mutator};
use libafl::observers::MapObserver;
use libafl::schedulers::TestcaseScore;
use libafl::stages::mutational::MutatedTransform;
use libafl::state::{HasCorpus, HasCurrentTestcase, HasExecutions, HasRand, MaybeHasClientPerfMonitor};
use libafl::{Error, Evaluator, HasMetadata, HasNamedMetadata};
use libafl_bolts::rands::Rand;
use libafl_bolts::tuples::{Handle, Handled, MatchNameRef};

use crate::config::config;

#[derive(Deserialize, Debug)]
pub struct MutationDeltaConfig {}

impl MutationDeltaConfig {
    pub fn discover_tokens<E, EM, I, S, M, F, C, Z, O>(
        &self,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
        mutator: &mut M,
        observer_handle: &Handle<C>,
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
        M: Mutator<I, S>,
        F: TestcaseScore<I, S>,
        C: Handled + AsRef<O> + AsMut<O>,
        Z: Evaluator<E, EM, I, S>,
        O: MapObserver,
    {
        let cfg = config();
        let mut original_testcase = state.current_testcase_mut().ok()?.clone();
        let original = original_testcase.input().clone()?;
        let mut input = original.clone();
        let score = F::compute(state, &mut original_testcase).ok()? as usize;

        for _ in 0..score {
            if let Ok(MutationResult::Mutated) = mutator.mutate(state, &mut input) {
                if let Ok((_, Some(_corpus_id))) = fuzzer.evaluate_filtered(state, executor, manager, &input) {
                    // Found interesting mutation - extract delta
                    return self.extract_delta(
                        &original, &input, cfg, fuzzer, executor, state, manager, observer_handle
                    );
                }
            }
        }
        None
    }

    fn extract_delta<E, EM, I, S, C, Z, O>(
        &self,
        original: &I,
        mutated: &I,
        cfg: &crate::config::TokenDiscoveryConfig,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
        observer_handle: &Handle<C>,
    ) -> Option<Vec<Vec<u8>>>
    where
        E: Executor<EM, I, S, Z> + HasObservers,
        E::Observers: MatchNameRef,
        I: Clone + From<Vec<u8>> + HasTargetBytes,
        S: HasRand,
        C: Handled + AsRef<O>,
        O: MapObserver,
    {
        let original_hash = self.get_coverage(original, fuzzer, executor, state, manager, observer_handle).ok()?;
        let mutated_hash = self.get_coverage(mutated, fuzzer, executor, state, manager, observer_handle).ok()?;

        let original_vec = original.target_bytes().to_vec();
        let mutated_vec = mutated.target_bytes().to_vec();
        let mut test_vec = original_vec.clone();

        let mut left_bound = 0;
        let mut right_bound = 0;

        // Find right bound
        for i in 0..mutated_vec.len() {
            if i >= test_vec.len() {
                test_vec.push(mutated_vec[i]);
            } else {
                test_vec[i] = mutated_vec[i];
            }

            let hash = self.get_coverage(&test_vec.clone().into(), fuzzer, executor, state, manager, observer_handle).ok()?;
            if hash == mutated_hash {
                left_bound = i;
                right_bound = i + 1;
                break;
            }
        }

        // Extend right bound
        for i in right_bound..min(mutated_vec.len(), left_bound + cfg.max_token_length) {
            if i >= test_vec.len() {
                break;
            }

            let tmp = test_vec[i];
            test_vec[i] = state.rand_mut().next() as u8;

            let hash = self.get_coverage(&test_vec.clone().into(), fuzzer, executor, state, manager, observer_handle).ok()?;
            if hash == mutated_hash {
                right_bound = i;
                test_vec[i] = tmp;
                break;
            }
        }

        // Find left bound
        for i in right_bound.saturating_sub(cfg.max_token_length)..right_bound {
            let tmp = test_vec[i];
            test_vec[i] = if i >= original_vec.len() || test_vec[i] == original_vec[i] {
                state.rand_mut().next() as u8
            } else {
                original_vec[i]
            };

            let hash = self.get_coverage(&test_vec.clone().into(), fuzzer, executor, state, manager, observer_handle).ok()?;
            if hash == original_hash {
                left_bound = i;
                test_vec[i] = tmp;
                break;
            }
        }

        let token_length = right_bound - left_bound;
        if token_length >= cfg.min_token_length {
            Some(vec![mutated_vec[left_bound..right_bound].to_vec()])
        } else {
            None
        }
    }

    fn get_coverage<E, EM, I, S, Z, C, O>(
        &self,
        input: &I,
        fuzzer: &mut Z,
        executor: &mut E,
        state: &mut S,
        manager: &mut EM,
        observer_handle: &Handle<C>,
    ) -> Result<Vec<O::Entry>, Error>
    where
        E: Executor<EM, I, S, Z> + HasObservers,
        E::Observers: MatchNameRef,
        C: Handled + AsRef<O>,
        O: MapObserver,
    {
        executor.run_target(fuzzer, state, manager, input)?;

        let observers = executor.observers();
        let edge_observer = observers
            .get(observer_handle)
            .ok_or_else(|| Error::key_not_found("MapObserver not found".to_string()))?
            .as_ref();

        Ok(edge_observer.to_vec())
    }
}