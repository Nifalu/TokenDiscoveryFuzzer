use libafl::{mutators::{Mutator, MutationResult}, state::{HasRand, HasMaxSize}, inputs::{HasMutatorBytes, ResizableMutator}, corpus::CorpusId, Error, HasMetadata};
use libafl_bolts::{Named};
use serde::{Serialize, Deserialize};
use std::borrow::Cow;
use std::collections::HashSet;
use std::num::NonZero;
use std::slice::Iter;
use libafl_bolts::rands::Rand;

#[expect(clippy::unsafe_derive_deserialize)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartTokens {
    tokens_vec: Vec<Vec<u8>>,       // fast access
    tokens_set: HashSet<Vec<u8>>,   // fast deduplication
    stats: Vec<TokenStat>,
    max_tokens: usize,
    protected_idx: Option<usize>,  // index currently in use
}

libafl_bolts::impl_serdeany!(SmartTokens);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStat {
    pub uses: u64,
    pub successes: u64,
}

/// The metadata used for SmartToken mutators
impl SmartTokens {
    /// limit how many tokens we can have
    const DEFAULT_MAX_TOKENS: usize = 100;

    /// Creates a new SmartTokens metadata with default capacity
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_MAX_TOKENS)
    }

    /// Creates a new SmartTokens metadata with custom capacity
    #[must_use]
    pub fn with_capacity(max_tokens: usize) -> Self {
        Self {
            tokens_vec: Vec::with_capacity(max_tokens),
            tokens_set: HashSet::with_capacity(max_tokens),
            stats: Vec::with_capacity(max_tokens),
            max_tokens,
            protected_idx: None
        }
    }

    /// protect the token currently in use from being replaced
    pub fn protect_index(&mut self, idx: usize) {
        self.protected_idx = Some(idx);
    }

    /// unprotect a token if its no longer in use
    pub fn unprotect(&mut self) {
        self.protected_idx = None;
    }

    /// Adds a token to a dictionary, checking it is not a duplicate
    /// Returns `false` if the token was already present and did not get added.
    #[expect(clippy::ptr_arg)]
    pub fn add_token(&mut self, token: &Vec<u8>) -> Option<usize> {
        if self.tokens_set.contains(token) {
            return None;
        }
        if self.tokens_vec.len() < self.max_tokens {
            self.tokens_vec.push(token.clone());
            self.tokens_set.insert(token.clone());
            self.stats.push(TokenStat::default());
            Some(self.tokens_vec.len() - 1)
        } else {
            match self.find_eviction_index() {
                Some(idx) => {
                    self.tokens_set.remove(&self.tokens_vec[idx]);
                    self.tokens_set.insert(token.clone());
                    self.tokens_vec[idx] = token.clone(); // replace old token
                    self.stats[idx] = TokenStat::default();

                    Some(idx)
                },
                None => None // reject new token
            }
        }
    }

    /// Determine which tokens to drop whenever the limit is reached.
    fn find_eviction_index(&self) -> Option<usize> {
        // first try to sort out unuseful ones
        let mut worst_idx: usize = 0;
        let mut worst_rate = f64::MAX;

        for (i, stat) in self.stats.iter().enumerate() {
            if Some(i) == self.protected_idx {
                continue // don't remove the index we just used.
            }

            if stat.uses > 0 {
                let rate = stat.successes as f64 / stat.uses as f64;
                if rate < worst_rate {
                    worst_rate = rate;
                    worst_idx = i;
                }
            }
        }

        // Don't return protected index
        if Some(worst_idx) == self.protected_idx {
            return None;
        }

        if worst_rate > 1.0 {
            return None  // No token has been used yet
        }

        Some(worst_idx)
    }

    /// record the use of a token
    #[inline]
    pub fn update_stats(&mut self, idx: usize, success: bool) {
        if let Some(stat) = self.stats.get_mut(idx) {
            stat.uses += 1;
            if success {
                stat.successes += 1;
            }
        }
    }

    /// Gets the tokens stored in this db
    #[inline]
    pub fn tokens(&self) -> &[Vec<u8>] {
        &self.tokens_vec
    }

    /// Returns an iterator over the tokens.
    #[inline]
    pub fn iter(&self) -> Iter<'_, Vec<u8>> {
        self.tokens_vec.iter()
    }
}

#[derive(Debug, Default)]
pub struct SmartToken {
    last_token_idx: Option<usize>,
}

impl SmartToken {
    pub fn record_token_use<S>(&mut self, idx: usize, state: &mut S) -> Result<(), Error>
    where
        S: HasMetadata,
    {
        // Protect this token from eviction during execution
        if let Some(smart_tokens) = state.metadata_map_mut().get_mut::<SmartTokens>() {
            smart_tokens.protect_index(idx);
        }
        self.last_token_idx = Some(idx);
        Ok(())
    }

    pub fn post_exec<S>(&mut self, state: &mut S, corpus_id: Option<CorpusId>) -> Result<(), Error>
    where
        S: HasMetadata,
    {
        if let Some(idx) = self.last_token_idx {
            let smart_tokens = state.metadata_map_mut().get_mut::<SmartTokens>().unwrap();

            // Unprotect and record use
            smart_tokens.unprotect();
            smart_tokens.update_stats(idx, corpus_id.is_some());

            self.last_token_idx = None;
        }
        Ok(())
    }
}



/// Inserts a random token at a random position in the `Input`.
#[derive(Debug, Default)]
pub struct SmartTokenInsert {
    smart_token: SmartToken
}

impl<I, S> Mutator<I, S> for SmartTokenInsert
where
    S: HasMetadata + HasRand + HasMaxSize,
    I: ResizableMutator<u8> + HasMutatorBytes,
{
    fn mutate(&mut self, state: &mut S, input: &mut I) -> Result<MutationResult, Error> {
        let max_size = state.max_size();
        let tokens_len = {
            let Some(meta) = state.metadata_map().get::<SmartTokens>() else {
                return Ok(MutationResult::Skipped);
            };
            if let Some(tokens_len) = NonZero::new(meta.tokens().len()) {
                tokens_len
            } else {
                return Ok(MutationResult::Skipped);
            }
        };

        let token_idx = state.rand_mut().below(tokens_len);
        let size = input.mutator_bytes().len();

        // # Safety
        // after saturating add it's always above 0
        let off = state
            .rand_mut()
            .below(unsafe { NonZero::new(size.saturating_add(1)).unwrap_unchecked()});

        let meta = state.metadata_map().get::<SmartTokens>().unwrap();
        let token = &meta.tokens()[token_idx];
        let mut len = token.len();

        if size + len > max_size {
            if max_size > size {
                len = max_size - size;
            } else {
                return Ok(MutationResult::Skipped);
            }
        }

        input.resize(size + len, 0);
        unsafe {
            buffer_self_copy(input.mutator_bytes_mut(), off, off+len, size - off);
            buffer_copy(input.mutator_bytes_mut(), token, 0, off, len);
        }

        // Track that we used this token
        self.smart_token.record_token_use(token_idx, state)?;

        Ok(MutationResult::Mutated)
    }

    fn post_exec(&mut self, state: &mut S, corpus_id: Option<CorpusId>) -> Result<(), Error> {
        self.smart_token.post_exec(state, corpus_id)
    }
}

impl Named for SmartTokenInsert {
    fn name(&self) -> &Cow<'static, str> {
        static NAME: Cow<'static, str> = Cow::Borrowed("SmartTokenInsert");
        &NAME
    }
}

impl SmartTokenInsert {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}


/// A `TokenReplace` [`Mutator`] replaces a random part of the input with one of a range of tokens.
/// From AFL terms, this is called as `Dictionary` mutation (which doesn't really make sense ;) ).
#[derive(Debug, Default)]
pub struct SmartTokenReplace {
    smart_token: SmartToken,
}

impl<I, S> Mutator<I, S> for SmartTokenReplace
where
    S: HasMetadata + HasRand + HasMaxSize,
    I: ResizableMutator<u8> + HasMutatorBytes,
{
    fn mutate(&mut self, state: &mut S, input: &mut I) -> Result<MutationResult, Error> {
        let size = input.mutator_bytes().len();
        let off = if let Some(nz) = NonZero::new(size) {
            state.rand_mut().below(nz)
        } else {
            return Ok(MutationResult::Skipped);
        };

        let tokens_len = {
            let Some(meta) = state.metadata_map().get::<SmartTokens>() else {
                return Ok(MutationResult::Skipped);
            };
            if let Some(tokens_len) = NonZero::new(meta.tokens().len()) {
                tokens_len
            } else {
                return Ok(MutationResult::Skipped);
            }
        };
        let token_idx = state.rand_mut().below(tokens_len);

        let meta = state.metadata_map().get::<SmartTokens>().unwrap();
        let token = &meta.tokens()[token_idx];
        let mut len = token.len();
        if off + len > size {
            len = size - off;
        }

        unsafe {
            buffer_copy(input.mutator_bytes_mut(), token, 0, off, len);
        }

        // Track that we used this token
        self.smart_token.record_token_use(token_idx, state)?;

        Ok(MutationResult::Mutated)
    }

    fn post_exec(&mut self, state: &mut S, corpus_id: Option<CorpusId>) -> Result<(), Error> {
        self.smart_token.post_exec(state, corpus_id)
    }
}

impl Named for SmartTokenReplace {
    fn name(&self) -> &Cow<'static, str> {
        static NAME: Cow<'static, str> = Cow::Borrowed("SmartTokenReplace");
        &NAME
    }
}

impl SmartTokenReplace {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for SmartTokens {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for TokenStat {
    fn default() -> Self {
        Self {
            uses: 1,        // Start at 1
            successes: 1,   // Start at 1 (100% initial success rate)
        }
    }
}

// Allow sharing of discovered tokens between different clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredTokens {
    pub tokens: Vec<Vec<u8>>,
}
libafl_bolts::impl_serdeany!(DiscoveredTokens);



// ------------- Utilities copied from libafl mutations.rs (private) ------------- //

/// Mem move in the own vec
#[inline]
unsafe fn buffer_self_copy<T>(data: &mut [T], from: usize, to: usize, len: usize) {
    debug_assert!(!data.is_empty());
    debug_assert!(from + len <= data.len());
    debug_assert!(to + len <= data.len());
    if len != 0 && from != to {
        let ptr = data.as_mut_ptr();
        unsafe {
            core::ptr::copy(ptr.add(from), ptr.add(to), len);
        }
    }
}

/// Mem move between vecs
#[inline]
unsafe fn buffer_copy<T>(dst: &mut [T], src: &[T], from: usize, to: usize, len: usize) {
    debug_assert!(!dst.is_empty());
    debug_assert!(!src.is_empty());
    debug_assert!(from + len <= src.len());
    debug_assert!(to + len <= dst.len());
    let dst_ptr = dst.as_mut_ptr();
    let src_ptr = src.as_ptr();
    if len != 0 {
        unsafe {
            core::ptr::copy(src_ptr.add(from), dst_ptr.add(to), len);
        }
    }
}




