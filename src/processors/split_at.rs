use crate::config::config;
use crate::print_stats;
use super::Processor;

pub struct SplitAt {
    pub delimiters: Vec<Vec<u8>>,
    pub min_length: usize,
}

impl Processor for SplitAt {
    fn process(&self, inputs: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>> {
        let original_count = inputs.len();

        let result: Vec<Vec<u8>> = inputs.into_iter()
            .flat_map(|token| self.split_token(&token))
            .filter(|t| t.len() >= self.min_length)
            .collect();

        if !config().silent_run {
            print_stats!(self.name(), "Split {} inputs into {} parts using {} delimiter(s).",
            original_count,
            result.len(),
            self.delimiters.len());
        }

        if result.is_empty() { None } else { Some(result) }
    }

    fn name(&self) -> &'static str { "split_at" }
}

impl SplitAt {
    fn split_token(&self, token: &[u8]) -> Vec<Vec<u8>> {
        let mut result = vec![token.to_vec()];

        for delim in &self.delimiters {
            result = result.into_iter()
                .flat_map(|chunk| self.split_by_delimiter(&chunk, delim))
                .collect();
        }

        result
    }

    fn split_by_delimiter(&self, data: &[u8], delim: &[u8]) -> Vec<Vec<u8>> {
        if delim.is_empty() || data.len() < delim.len() {
            return vec![data.to_vec()];
        }

        let mut result = Vec::new();
        let mut start = 0;
        let mut i = 0;

        while i <= data.len() - delim.len() {
            if &data[i..i + delim.len()] == delim {
                if i > start {
                    result.push(data[start..i].to_vec());
                }
                start = i + delim.len();
                i = start;
            } else {
                i += 1;
            }
        }

        if start < data.len() {
            result.push(data[start..].to_vec());
        }

        result
    }
}