use std::collections::HashSet;
use std::time::Instant;
use libsais::SuffixArrayConstruction;

pub enum TokenSelectionMode {
    /// Return tokens occurring in at least this fraction of inputs (0.0 - 1.0)
    Threshold(f64),
    /// Return at least N tokens by automatically adjusting threshold
    MinTokenCount(usize),
}

pub struct TokenDiscoveryResult {
    pub tokens: Vec<Vec<u8>>,
    pub threshold_percentage: f64,
    pub threshold_absolute: usize,
}

fn clean_token(token: &[u8], strip_bytes: &[u8]) -> Option<Vec<u8>> {
    let should_strip = |b: &u8| strip_bytes.contains(b);
    let start = token.iter().position(|b| !should_strip(b))?;
    let end = token.iter().rposition(|b| !should_strip(b)).map(|i| i + 1)?;
    if start < end {
        Some(token[start..end].to_vec())
    } else {
        None
    }
}

fn has_too_many_nulls(token: &[u8], max_ratio: f64) -> bool {
    let null_count = token.iter().filter(|&&b| b == 0).count();
    (null_count as f64 / token.len() as f64) > max_ratio
}

fn remove_substrings(tokens: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>> {
    if tokens.is_empty() {
        return None;
    }

    let original_len = tokens.len();
    let mut sorted = tokens;
    sorted.sort_by(|a, b| b.len().cmp(&a.len()));

    let mut result: Vec<Vec<u8>> = Vec::new();
    for token in sorted {
        let is_substring = result.iter().any(|existing|
            existing.windows(token.len()).any(|w| w == token.as_slice())
        );
        if !is_substring {
            result.push(token);
        }
    }

    println!("Removed {} substrings from tokens.", original_len - result.len());

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

pub fn find_common_substrings(
    corpus: &[Vec<u8>],
    min_len: usize,
    max_len: usize,
    mode: TokenSelectionMode,
    strip_bytes: &[u8],
    max_null_ratio: Option<f64>,
    remove_subs: bool,
) -> Option<TokenDiscoveryResult> {
    if corpus.is_empty() {
        return None;
    }

    let total_start = Instant::now();

    // 1. Concatenate all inputs, track input boundaries
    let start = Instant::now();
    let mut concat: Vec<u8> = Vec::new();
    let mut input_id: Vec<usize> = Vec::new();

    for (id, entry) in corpus.iter().enumerate() {
        for &byte in entry {
            concat.push(byte);
            input_id.push(id);
        }
    }

    let concatenation_time = start.elapsed();

    if concat.is_empty() {
        return None;
    }

    // 2. Build suffix array -> plcp -> lcp (chained API)
    let start = Instant::now();
    let sa_result = match SuffixArrayConstruction::for_text(&concat)
        .in_owned_buffer32()
        .single_threaded()
        .run()
    {
        Ok(r) => r,
        Err(_) => return None,
    };

    let sa_construction_time = start.elapsed();

    let start = Instant::now();
    let plcp_result = match sa_result.plcp_construction().single_threaded().run() {
        Ok(r) => r,
        Err(_) => return None,
    };

    let plcp_construction_time = start.elapsed();

    let start = Instant::now();
    let lcp_result = match plcp_result.lcp_construction().single_threaded().run() {
        Ok(r) => r,
        Err(_) => return None,
    };

    let lcp_construction_time = start.elapsed();

    let (sa, lcp, _, _) = lcp_result.into_parts();

    // 3. Scan LCP array to find common substrings with input counts
    let start = Instant::now();
    let mut token_candidates: Vec<(Vec<u8>, usize)> = Vec::new(); // (token, input_count)
    let n = sa.len();

    let mut i = 1;
    while i < n {
        let current_lcp = lcp[i] as usize;
        if current_lcp < min_len {
            i += 1;
            continue;
        }

        let mut group_inputs: HashSet<usize> = HashSet::new();
        group_inputs.insert(input_id[sa[i - 1] as usize]);

        let mut group_min_lcp = current_lcp;
        let group_start = i - 1;

        while i < n && (lcp[i] as usize) >= min_len {
            group_inputs.insert(input_id[sa[i] as usize]);
            group_min_lcp = group_min_lcp.min(lcp[i] as usize);
            i += 1;
        }

        // Collect all candidates with their input counts (filter later based on mode)
        if group_inputs.len() >= 2 {
            let pos = sa[group_start] as usize;
            let len = group_min_lcp.min(max_len);
            if pos + len <= concat.len() {
                let token = concat[pos..pos + len].to_vec();
                token_candidates.push((token, group_inputs.len()));
            }
        }
    }

    let lcp_scan_time = start.elapsed();

    // 4. Optionally clean tokens before selection
    let start = Instant::now();
    let mut token_candidates: Vec<(Vec<u8>, usize)> = token_candidates
        .into_iter()
        .filter_map(|(t, count)| {
            let cleaned = if strip_bytes.is_empty() {
                t
            } else {
                clean_token(&t, strip_bytes)?
            };

            if cleaned.len() < min_len {
                return None;
            }

            if let Some(max) = max_null_ratio {
                if has_too_many_nulls(&cleaned, max) {
                    return None;
                }
            }

            Some((cleaned, count))
        })
        .collect();

    if remove_subs {
        let tokens_only: Vec<Vec<u8>> = token_candidates.iter().map(|(t, _)| t.clone()).collect();
        if let Some(deduped) = remove_substrings(tokens_only) {
            let deduped_set: std::collections::HashSet<&[u8]> = deduped.iter().map(|t| t.as_slice()).collect();
            token_candidates.retain(|(t, _)| deduped_set.contains(t.as_slice()));
        }
    }

    let cleaning_time = start.elapsed();

    // 5. Select tokens based on mode
    let start = Instant::now();
    let (tokens, threshold_absolute): (HashSet<Vec<u8>>, usize) = match mode {
        TokenSelectionMode::Threshold(t) => {
            let min_inputs = ((corpus.len() as f64) * t).ceil() as usize;
            let tokens = token_candidates
                .into_iter()
                .filter(|(_, count)| *count >= min_inputs)
                .map(|(token, _)| token)
                .collect();
            (tokens, min_inputs)
        }
        TokenSelectionMode::MinTokenCount(target) => {
            token_candidates.sort_by(|a, b| b.1.cmp(&a.1));
            token_candidates.dedup_by(|a, b| a.0 == b.0);

            if token_candidates.is_empty() {
                (HashSet::new(), 0)
            } else if token_candidates.len() <= target {
                let min_count = token_candidates.last().map(|(_, c)| *c).unwrap_or(0);
                (token_candidates.into_iter().map(|(t, _)| t).collect(), min_count)
            } else {
                let cutoff_count = token_candidates[target.saturating_sub(1)].1;
                let tokens = token_candidates
                    .into_iter()
                    .filter(|(_, count)| *count >= cutoff_count)
                    .map(|(token, _)| token)
                    .collect();
                (tokens, cutoff_count)
            }
        }
    };

    let threshold_percentage = threshold_absolute as f64 / corpus.len() as f64;
    let selection_time = start.elapsed();

    // print all timings in one line
    println!(
        "[SAIS Timings]: concat {:?} ({} bytes), SA {:?}, PLCP {:?}, LCP {:?}, scan {:?}, clean {:?}, select {:?}, total {:?}",
        concatenation_time,
        concat.len(),
        sa_construction_time,
        plcp_construction_time,
        lcp_construction_time,
        lcp_scan_time,
        cleaning_time,
        selection_time,
        total_start.elapsed()
    );

    Some(TokenDiscoveryResult {
        tokens: tokens.into_iter().collect(),
        threshold_percentage,
        threshold_absolute,
    })
}