use crate::print_stats;
use super::Processor;
pub struct FilterNullBytes {
    pub max_ratio: f64,
}

impl Processor for FilterNullBytes {
    fn process(&self, inputs: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>> {
        let token_len = inputs.len();
        let result: Vec<Vec<u8>> = inputs.into_iter()
            .filter(|t| {
                let null_count = t.iter().filter(|&&b| b == 0).count();
                (null_count as f64 / t.len() as f64) <= self.max_ratio
            })
            .collect();

        print_stats!(self.name(),"Removed {} tokens with too many null bytes.", token_len - result.len());
        if result.is_empty() { None } else { Some(result) }
    }

    fn name(&self) -> &'static str { "filter_null_bytes" }
}