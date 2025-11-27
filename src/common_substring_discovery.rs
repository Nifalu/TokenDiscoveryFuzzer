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

pub fn find_common_substrings(
    corpus: &[Vec<u8>],
    min_len: usize,
    max_len: usize,
    mode: TokenSelectionMode,
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

    // 4. Select tokens based on mode
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

    // print all timings in one line
    println!(
        "Token Discovery: concat {:?} ({} bytes), SA {:?}, PLCP {:?}, LCP {:?}, scan {:?}, total {:?}",
        concatenation_time,
        concat.len(),
        sa_construction_time,
        plcp_construction_time,
        lcp_construction_time,
        lcp_scan_time,
        total_start.elapsed()
    );
    println!(
        "Selected {} tokens (threshold: {:.1}%, min {} of {} inputs)",
        tokens.len(),
        threshold_percentage * 100.0,
        threshold_absolute,
        corpus.len()
    );

    Some(TokenDiscoveryResult {
        tokens: tokens.into_iter().collect(),
        threshold_percentage,
        threshold_absolute,
    })
}