use libafl::{
    mutators::{MutationResult, Mutator, MutatorsTuple, ScheduledMutator, ComposedByMutations, MutationId},
    state::HasRand,
    corpus::CorpusId,
    Error,
};
use libafl_bolts::{Named, HasLen, tuples::NamedTuple};
use std::borrow::Cow;
use std::num::NonZero;
use libafl_bolts::rands::Rand;


/// A scheduled mutator that preserves token mutations by applying them last
pub struct TokenPreservingScheduledMutator<MT> {
    name: Cow<'static, str>,
    mutations: MT,
    max_stack_pow: usize,
    token_indices: Vec<usize>,  // Indices of token mutations in the tuple
    last_token_used: Option<usize>,  // Track which token mutation was used (raw index)
}

impl<MT> TokenPreservingScheduledMutator<MT>
where
    MT: NamedTuple + HasLen,
{
    pub fn new(mutations: MT) -> Self {
        // Identify which mutations are token mutations at construction time
        let token_indices = Self::identify_token_mutations(&mutations);

        Self {
            name: Cow::from(format!(
                "TokenPreservingScheduledMutator[{}]",
                mutations.names().join(", ")
            )),
            mutations,
            max_stack_pow: 7,
            token_indices,
            last_token_used: None,
        }
    }

    /// Identify token mutations by their name
    fn identify_token_mutations(mutations: &MT) -> Vec<usize> {
        let mut indices = Vec::new();
        for (i, name) in mutations.names().iter().enumerate() {
            // Check for both SmartToken and regular Token mutations
            if name.contains("Token") {
                indices.push(i);
            }
        }
        indices
    }
}

impl<MT> TokenPreservingScheduledMutator<MT>
where
    MT: HasLen,
{
    fn is_token_mutation(&self, idx: usize) -> bool {
        self.token_indices.contains(&idx)
    }

    /// Schedule a non-token mutation
    fn schedule_non_token<S: HasRand>(&self, state: &mut S) -> MutationId {
        let total_len = self.mutations.len();
        let non_token_count = total_len - self.token_indices.len();
        if non_token_count == 0 {
            // Only token mutations available
            return self.schedule_token(state).into();
        }

        loop {
            let idx = state.rand_mut().below(unsafe { NonZero::new(total_len).unwrap_unchecked() });
            if !self.is_token_mutation(idx) {
                return idx.into();
            }
        }
    }

    /// Schedule a token mutation (returns raw index, not MutationId)
    fn schedule_token<S: HasRand>(&self, state: &mut S) -> usize {
        if self.token_indices.is_empty() {
            panic!("No token mutations available");
        }

        let idx = state.rand_mut().below(unsafe { NonZero::new(self.token_indices.len()).unwrap_unchecked() });
        self.token_indices[idx]  // Return the actual mutation index
    }
}

impl<MT> Named for TokenPreservingScheduledMutator<MT> {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<MT> ComposedByMutations for TokenPreservingScheduledMutator<MT> {
    type Mutations = MT;

    fn mutations(&self) -> &MT {
        &self.mutations
    }

    fn mutations_mut(&mut self) -> &mut MT {
        &mut self.mutations
    }
}

impl<I, MT, S> Mutator<I, S> for TokenPreservingScheduledMutator<MT>
where
    MT: MutatorsTuple<I, S> + HasLen,
    S: HasRand,
{
    fn mutate(&mut self, state: &mut S, input: &mut I) -> Result<MutationResult, Error> {
        let mut r = MutationResult::Skipped;
        let base_iterations = self.iterations(state, input);
        self.last_token_used = None;

        // Decide upfront if we'll use a token mutation
        let use_token = !self.token_indices.is_empty()
            && state.rand_mut().below(NonZero::new(100).unwrap()) < 30; // 30% chance

        let iterations = if use_token {
            // If using token, apply fewer stacked mutations to preserve it
            (base_iterations / 2).max(1)
        } else {
            base_iterations
        };

        // Apply non-token mutations
        for _ in 0..iterations {
            let idx = if use_token {
                self.schedule_non_token(state)
            } else {
                self.schedule(state, input)  // Use regular scheduling when no token
            };

            let outcome = self.mutations_mut().get_and_mutate(idx, state, input)?;
            if outcome == MutationResult::Mutated {
                r = MutationResult::Mutated;
            }
        }

        // Apply token mutation last (if we decided to use one)
        if use_token {
            let token_idx = self.schedule_token(state);  // Returns usize
            let outcome = self.mutations_mut().get_and_mutate(token_idx.into(), state, input)?;
            if outcome == MutationResult::Mutated {
                r = MutationResult::Mutated;
                self.last_token_used = Some(token_idx);  // Store the raw index
            }
        }

        Ok(r)
    }

    fn post_exec(&mut self, state: &mut S, corpus_id: Option<CorpusId>) -> Result<(), Error> {
        // Only call post_exec if we used a token mutation
        if let Some(idx) = self.last_token_used {
            self.mutations_mut().get_and_post_exec(idx, state, corpus_id)?;
            self.last_token_used = None;
        }
        Ok(())
    }
}

impl<I, MT, S> ScheduledMutator<I, S> for TokenPreservingScheduledMutator<MT>
where
    MT: MutatorsTuple<I, S> + HasLen,
    S: HasRand,
{
    fn iterations(&self, state: &mut S, _: &I) -> u64 {
        1 << (1 + state.rand_mut().below_or_zero(self.max_stack_pow))
    }

    fn schedule(&self, state: &mut S, _: &I) -> MutationId {
        debug_assert_ne!(self.mutations.len(), 0);
        state
            .rand_mut()
            .below(unsafe { NonZero::new(self.mutations.len()).unwrap_unchecked() })
            .into()
    }
}