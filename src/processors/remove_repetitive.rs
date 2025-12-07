// src/processors/remove_repetitive.rs

use crate::config::config;
use crate::print_stats;
use super::Processor;

pub struct RemoveRepetitive {
    pub threshold: f64,  // e.g., 0.8 = remove if >80% is same byte
}

impl Processor for RemoveRepetitive {
    fn process(&self, inputs: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>> {
        let original_count = inputs.len();

        let result: Vec<Vec<u8>> = inputs.into_iter()
            .filter(|token| {
                if token.is_empty() { return false; }
                let mut counts = [0usize; 256];
                for &b in token { counts[b as usize] += 1; }
                let max_count = *counts.iter().max().unwrap();
                (max_count as f64 / token.len() as f64) < self.threshold
            })
            .collect();

        if !config().silent_run {
            print_stats!(self.name(), "Removed {} repetitive tokens (threshold {:.0}%).",
                original_count - result.len(),
                self.threshold * 100.0
            );
        }

        if result.is_empty() { None } else { Some(result) }
    }

    fn name(&self) -> &'static str { "remove_repetitive" }
}