use crate::print_stats;
use super::Processor;
pub struct StripBytes {
    pub bytes_to_strip: Vec<u8>,
    pub min_length: usize,
}

impl Processor for StripBytes {
    fn process(&self, inputs: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>> {
        let original_count = inputs.len();
        let mut stripped_count = 0;

        let result: Vec<Vec<u8>> = inputs.into_iter()
            .filter_map(|token| {
                let should_strip = |b: &u8| self.bytes_to_strip.contains(b);
                let start = token.iter().position(|b| !should_strip(b))?;
                let end = token.iter().rposition(|b| !should_strip(b)).map(|i| i + 1)?;

                if start >= end {
                    return None;
                }

                let stripped = token[start..end].to_vec();
                if stripped.len() != token.len() {
                    stripped_count += 1;
                }

                if stripped.len() >= self.min_length {
                    Some(stripped)
                } else {
                    None
                }
            })
            .collect();

        let removed_count = original_count - result.len();
        print_stats!(self.name(),"Stripped {} tokens, removed {} below min length {}.",
            stripped_count,
            removed_count,
            self.min_length
        );

        if result.is_empty() { None } else { Some(result) }
    }

    fn name(&self) -> &'static str { "strip_bytes" }
}