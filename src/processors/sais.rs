use std::collections::HashSet;
use std::time::Instant;
use libsais::SuffixArrayConstruction;
use crate::print_stats;
use super::Processor;

pub enum SelectionMode {
    Threshold(f64),
    MinTokenCount(usize),
}

pub struct Sais {
    pub min_len: usize,
    pub max_len: usize,
    pub mode: SelectionMode,
}

impl Processor for Sais {
    fn process(&self, inputs: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>> {
        if inputs.is_empty() {
            return None;
        }

        let total_start = Instant::now();
        let corpus_size = inputs.len();

        // 1. Concatenate all inputs, track input boundaries
        let mut concat: Vec<u8> = Vec::new();
        let mut input_id: Vec<usize> = Vec::new();

        for (id, entry) in inputs.iter().enumerate() {
            for &byte in entry {
                concat.push(byte);
                input_id.push(id);
            }
        }

        if concat.is_empty() {
            return None;
        }

        // 2. Build suffix array -> plcp -> lcp
        let sa_result = SuffixArrayConstruction::for_text(&concat)
            .in_owned_buffer32()
            .single_threaded()
            .run()
            .ok()?;


        let plcp_result = sa_result.plcp_construction().single_threaded().run().ok()?;
        let lcp_result = plcp_result.lcp_construction().single_threaded().run().ok()?;
        let (sa, lcp, _, _) = lcp_result.into_parts();

        // 3. Scan LCP array
        let mut candidates: Vec<(Vec<u8>, usize)> = Vec::new();
        let n = sa.len();
        let mut i = 1;

        while i < n {
            let current_lcp = lcp[i] as usize;
            if current_lcp < self.min_len {
                i += 1;
                continue;
            }

            let mut group_inputs: HashSet<usize> = HashSet::new();
            group_inputs.insert(input_id[sa[i - 1] as usize]);

            let mut group_min_lcp = current_lcp;
            let group_start = i - 1;

            while i < n && (lcp[i] as usize) >= self.min_len {
                group_inputs.insert(input_id[sa[i] as usize]);
                group_min_lcp = group_min_lcp.min(lcp[i] as usize);
                i += 1;
            }

            if group_inputs.len() >= 2 {
                let pos = sa[group_start] as usize;
                let len = group_min_lcp.min(self.max_len);
                if pos + len <= concat.len() {
                    candidates.push((concat[pos..pos + len].to_vec(), group_inputs.len()));
                }
            }
        }

        // 4. Select tokens
        let (tokens, threshold): (HashSet<Vec<u8>>, usize) = match &self.mode {
            SelectionMode::Threshold(t) => {
                let min_inputs = ((corpus_size as f64) * t).ceil() as usize;
                let tokens = candidates.into_iter()
                    .filter(|(_, count)| *count >= min_inputs)
                    .map(|(token, _)| token)
                    .collect();
                (tokens, min_inputs)
            }
            SelectionMode::MinTokenCount(target) => {
                candidates.sort_by(|a, b| b.1.cmp(&a.1));
                candidates.dedup_by(|a, b| a.0 == b.0);

                if candidates.is_empty() {
                    (HashSet::new(), 0)
                } else if candidates.len() <= *target {
                    let min_count = candidates.last().map(|(_, c)| *c).unwrap_or(0);
                    (candidates.into_iter().map(|(t, _)| t).collect(), min_count)
                } else {
                    let cutoff = candidates[target.saturating_sub(1)].1;
                    (candidates.into_iter()
                         .filter(|(_, count)| *count >= cutoff)
                         .map(|(t, _)| t)
                         .collect(), cutoff)
                }
            }
        };

        print_stats!(self.name(),
            "{} inputs ({} bytes) pattern matched to {} tokens in {:.3}s | threshold {}/{} ({:.1}%)",
            corpus_size,
            concat.len(),
            tokens.len(),
            total_start.elapsed().as_secs_f64(),
            threshold,
            corpus_size,
            (threshold as f64 / corpus_size as f64) * 100.0,
        );

        let result: Vec<Vec<u8>> = tokens.into_iter().collect();
        if result.is_empty() { None } else { Some(result) }
    }

    fn name(&self) -> &'static str { "sais" }
}