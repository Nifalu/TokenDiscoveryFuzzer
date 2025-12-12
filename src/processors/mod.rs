pub mod sais;
mod filter_null_bytes;
mod strip_bytes;
mod remove_substrings;
mod remove_similar;
mod remove_repetitive;
mod split_at;

pub use sais::{Sais, SelectionMode};
pub use filter_null_bytes::FilterNullBytes;
pub use strip_bytes::StripBytes;
pub use remove_substrings::RemoveSubstrings;
pub use remove_similar::{RemoveSimilar, KeepStrategy};
pub use remove_repetitive::RemoveRepetitive;
pub use split_at::SplitAt;

use crate::config::{config, ProcessorConfig};

pub trait Processor: Send + Sync {
    fn process(&self, input: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>>;
    fn name(&self) -> &'static str;
}

pub fn build_pipeline(configs: &[ProcessorConfig]) -> Vec<Box<dyn Processor>> {
    configs.iter().map(|c| -> Box<dyn Processor> {
        match c {
            ProcessorConfig::FilterNullBytes { max_ratio } => {
                Box::new(FilterNullBytes { max_ratio: *max_ratio })
            }
            ProcessorConfig::RemoveRepetitive { threshold } => {
                Box::new(RemoveRepetitive { threshold: *threshold })
            }
            ProcessorConfig::RemoveSimilar { threshold, keep_longer } => {
                let keep = if *keep_longer { KeepStrategy::Longer } else { KeepStrategy::Shorter };
                Box::new(RemoveSimilar { threshold: *threshold, keep })
            }
            ProcessorConfig::RemoveSubstrings => {
                Box::new(RemoveSubstrings)
            }
            ProcessorConfig::Sais { min_len, max_len, threshold, token_count, threshold_fn } => {
                let cfg = config();
                let min = min_len.unwrap_or(cfg.min_token_length);
                let max = max_len.unwrap_or(cfg.max_token_length);
                let mode = match (threshold_fn, threshold, token_count) {
                    (Some(f), _, _) => SelectionMode::ThresholdFn(f.clone()),
                    (_, Some(t), _) => SelectionMode::Threshold(*t),
                    (_, _, Some(n)) => SelectionMode::MinTokenCount(*n),
                    _ => SelectionMode::Threshold(0.3),
                };
                Box::new(Sais { min_len: min, max_len: max, mode })
            }
            ProcessorConfig::SplitAt { delimiters, min_length } => {
                Box::new(SplitAt {
                    delimiters: delimiters.clone(),
                    min_length: min_length.unwrap_or(config().min_token_length)
                })
            }
            ProcessorConfig::StripBytes { bytes, min_length } => {
                Box::new(StripBytes {
                    bytes_to_strip: bytes.clone(),
                    min_length: min_length.unwrap_or(config().min_token_length)
                })
            }
        }
    }).collect()
}