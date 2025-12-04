use std::collections::HashSet;
use std::time::Instant;
use libsais::SuffixArrayConstruction;
use crate::print_stats;
use super::Processor;

use crate::config::ThresholdFunction;

pub enum SelectionMode {
    Threshold(f64),
    ThresholdFn(ThresholdFunction),
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
        let tokens: HashSet<Vec<u8>> = match &self.mode {
            SelectionMode::Threshold(t) => {
                let min_inputs = ((corpus_size as f64) * t).ceil() as usize;
                let tokens = candidates.into_iter()
                    .filter(|(_, count)| *count >= min_inputs)
                    .map(|(token, _)| token)
                    .collect();
                tokens
            }
            SelectionMode::ThresholdFn(f) => {
                let tokens: HashSet<Vec<u8>> = candidates.into_iter()
                    .filter(|(token, count)| {
                        let min_inputs = ((corpus_size as f64) * f.compute(token.len(), self.min_len, self.max_len)).ceil() as usize;
                        *count >= min_inputs.max(2)  // at least 2 inputs
                    })
                    .map(|(token, _)| token)
                    .collect();
                self.print_threshold_curve(corpus_size, f);
                tokens
            }
            SelectionMode::MinTokenCount(target) => {
                candidates.sort_by(|a, b| b.1.cmp(&a.1));
                candidates.dedup_by(|a, b| a.0 == b.0);

                if candidates.is_empty() {
                    HashSet::new()
                } else if candidates.len() <= *target {
                    candidates.into_iter().map(|(t, _)| t).collect()
                } else {
                    let cutoff = candidates[target.saturating_sub(1)].1;
                    candidates.into_iter()
                         .filter(|(_, count)| *count >= cutoff)
                         .map(|(t, _)| t)
                         .collect()
                }
            }
        };

        print_stats!(self.name(),
            "{} inputs ({} bytes) pattern matched to {} tokens in {:.3}s",
            corpus_size,
            concat.len(),
            tokens.len(),
            total_start.elapsed().as_secs_f64(),
        );

        let result: Vec<Vec<u8>> = tokens.into_iter().collect();
        if result.is_empty() { None } else { Some(result) }
    }
    fn name(&self) -> &'static str { "sais" }
}

impl Sais {
    fn print_threshold_curve(&self, corpus_size: usize, f: &ThresholdFunction) {
        let points = [0.0, 0.25, 0.5, 0.75, 1.0];
        let values: Vec<String> = points.iter().map(|&p| {
            let len = self.min_len + ((self.max_len - self.min_len) as f64 * p) as usize;
            let thresh = f.compute(len, self.min_len, self.max_len);
            let count = ((corpus_size as f64) * thresh).ceil() as usize;
            format!("{}→{}", len, count)
        }).collect();

        print_stats!(self.name(), "Threshold curve: {} (len→min_inputs)", values.join(" | "));
    }
}