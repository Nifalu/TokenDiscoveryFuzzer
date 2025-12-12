use crate::config::config;
use crate::print_stats;
use super::Processor;
pub struct RemoveSubstrings;

impl Processor for RemoveSubstrings {
    fn process(&self, inputs: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>> {
        let token_len = inputs.len();
        if inputs.is_empty() {
            return None;
        }

        let mut sorted = inputs;
        sorted.sort_by(|a, b| b.len().cmp(&a.len()));

        let mut result: Vec<Vec<u8>> = Vec::new();
        for token in sorted {
            let is_substring = result.iter()
                .any(|existing| existing.windows(token.len()).any(|w| w == token.as_slice()));
            if !is_substring {
                result.push(token);
            }
        }
        if !config().silent_run {
            print_stats!(self.name(), "Removed {} substrings from tokens.", token_len - result.len());
        }

        if result.is_empty() { None } else { Some(result) }
    }

    fn name(&self) -> &'static str { "remove_substrings" }
}