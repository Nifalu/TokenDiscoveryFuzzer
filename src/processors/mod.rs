pub mod sais;
mod filter_null_bytes;
mod strip_bytes;
mod remove_substrings;
mod remove_similar;
mod remove_repetitive;

pub use sais::{Sais, SelectionMode};
pub use filter_null_bytes::FilterNullBytes;
pub use strip_bytes::StripBytes;
pub use remove_substrings::RemoveSubstrings;
pub use remove_similar::{RemoveSimilar, KeepStrategy};
pub use remove_repetitive::RemoveRepetitive;

use crate::config::ProcessorConfig;

pub trait Processor: Send + Sync {
    fn process(&self, input: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>>;
    fn name(&self) -> &'static str;
}

pub fn build_pipeline(configs: &[ProcessorConfig]) -> Vec<Box<dyn Processor>> {
    configs.iter().map(|c| -> Box<dyn Processor> {
        match c {
            ProcessorConfig::Sais { min_len, max_len, threshold, token_count } => {
                let mode = match (threshold, token_count) {
                    (Some(t), _) => SelectionMode::Threshold(*t),
                    (_, Some(n)) => SelectionMode::MinTokenCount(*n),
                    _ => SelectionMode::Threshold(0.3),
                };
                Box::new(Sais { min_len: *min_len, max_len: *max_len, mode })
            }
            ProcessorConfig::FilterNullBytes { max_ratio } => {
                Box::new(FilterNullBytes { max_ratio: *max_ratio })
            }
            ProcessorConfig::StripBytes { bytes, min_length } => {
                Box::new(StripBytes { bytes_to_strip: bytes.clone(), min_length: *min_length })
            }
            ProcessorConfig::RemoveSubstrings => {
                Box::new(RemoveSubstrings)
            }
            ProcessorConfig::RemoveSimilar { threshold, keep_longer } => {
                let keep = if *keep_longer { KeepStrategy::Longer } else { KeepStrategy::Shorter };
                Box::new(RemoveSimilar { threshold: *threshold, keep })
            }
            ProcessorConfig::RemoveRepetitive { threshold } => {
                Box::new(RemoveRepetitive { threshold: *threshold })
            }
        }
    }).collect()
}