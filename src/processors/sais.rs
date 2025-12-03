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


        // 3. Scan LCP array using stack-based grouping
        let mut candidates: Vec<(Vec<u8>, usize)> = Vec::new();
        let n = sa.len();

        // Stack: (lcp_level, start_pos_in_sa, input_ids)
        let mut stack: Vec<(usize, usize, HashSet<usize>)> = Vec::new();

        for i in 1..n {
            let lcp = lcp[i] as usize;
            let current_input = input_id[sa[i] as usize];
            let prev_input = input_id[sa[i - 1] as usize];

            // Pop and emit groups closed by this lower LCP
            while let Some((level, _, _)) = stack.last() {
                if lcp < *level {
                    let (level, start, inputs) = stack.pop().unwrap();
                    if inputs.len() >= 2 {
                        let pos = sa[start] as usize;
                        let len = level.min(self.max_len);
                        if pos + len <= concat.len() {
                            candidates.push((concat[pos..pos + len].to_vec(), inputs.len()));
                        }
                    }
                } else {
                    break;
                }
            }

            if lcp < self.min_len {
                continue;
            }

            // Add to existing group at same level, or push new group
            if let Some((level, _, inputs)) = stack.last_mut() {
                if lcp == *level {
                    inputs.insert(current_input);
                } else {
                    // Rise: push new nested group
                    let mut new_inputs = HashSet::new();
                    new_inputs.insert(prev_input);
                    new_inputs.insert(current_input);
                    stack.push((lcp, i - 1, new_inputs));
                }
            } else {
                // Stack empty: start new group
                let mut new_inputs = HashSet::new();
                new_inputs.insert(prev_input);
                new_inputs.insert(current_input);
                stack.push((lcp, i - 1, new_inputs));
            }
        }

        // Flush remaining stack
        while let Some((level, start, inputs)) = stack.pop() {
            if inputs.len() >= 2 {
                let pos = sa[start] as usize;
                let len = level.min(self.max_len);
                if pos + len <= concat.len() {
                    candidates.push((concat[pos..pos + len].to_vec(), inputs.len()));
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