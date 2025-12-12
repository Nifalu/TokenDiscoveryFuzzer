// src/processors/similarity_filter.rs

use crate::config::config;
use crate::print_stats;
use super::Processor;

#[derive(Debug, Clone, Copy)]
pub enum KeepStrategy {
    Longer,
    Shorter,
}

pub struct RemoveSimilar {
    pub threshold: f64,      // e.g., 0.9 = 90% similar triggers removal
    pub keep: KeepStrategy,
}

impl RemoveSimilar {
    fn levenshtein(a: &[u8], b: &[u8]) -> usize {
        let mut prev: Vec<usize> = (0..=b.len()).collect();
        let mut curr = vec![0; b.len() + 1];

        for (i, &ca) in a.iter().enumerate() {
            curr[0] = i + 1;
            for (j, &cb) in b.iter().enumerate() {
                curr[j + 1] = if ca == cb {
                    prev[j]
                } else {
                    1 + prev[j].min(prev[j + 1]).min(curr[j])
                };
            }
            std::mem::swap(&mut prev, &mut curr);
        }
        prev[b.len()]
    }

    fn similarity(a: &[u8], b: &[u8]) -> f64 {
        let max_len = a.len().max(b.len());
        if max_len == 0 {
            return 1.0;
        }
        let dist = Self::levenshtein(a, b);
        1.0 - (dist as f64 / max_len as f64)
    }
}

impl Processor for RemoveSimilar {
    fn process(&self, inputs: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>> {
        let original_count = inputs.len();

        // Sort by length (descending for Longer, ascending for Shorter)
        let mut sorted = inputs;
        match self.keep {
            KeepStrategy::Longer => sorted.sort_by(|a, b| b.len().cmp(&a.len())),
            KeepStrategy::Shorter => sorted.sort_by(|a, b| a.len().cmp(&b.len())),
        }

        let mut result: Vec<Vec<u8>> = Vec::new();

        for token in sorted {
            let dominated = result.iter().any(|existing| {
                Self::similarity(&token, existing) >= self.threshold
            });

            if !dominated {
                result.push(token);
            }
        }
        if !config().silent_run {
            print_stats!(self.name(), "Removed {} similar tokens (threshold {:.0}%).",
                original_count - result.len(),
                self.threshold * 100.0
            );
        }

        if result.is_empty() { None } else { Some(result) }
    }

    fn name(&self) -> &'static str { "similarity_filter" }
}