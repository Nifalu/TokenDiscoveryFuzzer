use std::collections::HashSet;
use std::time::Instant;
use libsais::SuffixArrayConstruction;

pub fn find_common_substrings(
    corpus: &[Vec<u8>],
    min_len: usize,
    max_len: usize,
    min_docs: usize,
) -> Vec<Vec<u8>> {
    if corpus.len() < min_docs {
        return Vec::new();
    }

    let total_start = Instant::now();

    // 1. Concatenate all documents, track document boundaries
    let start = Instant::now();
    let mut concat: Vec<u8> = Vec::new();
    let mut doc_id: Vec<usize> = Vec::new();

    for (id, entry) in corpus.iter().enumerate() {
        for &byte in entry {
            concat.push(byte);
            doc_id.push(id);
        }
    }

    let concatenation_time = start.elapsed();

    if concat.is_empty() {
        return Vec::new();
    }

    // 2. Build suffix array -> plcp -> lcp (chained API)
    let start = Instant::now();
    let sa_result = match SuffixArrayConstruction::for_text(&concat)
        .in_owned_buffer32()
        .single_threaded()
        .run()
    {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let sa_construction_time = start.elapsed();

    let start = Instant::now();
    let plcp_result = match sa_result.plcp_construction().single_threaded().run() {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let plcp_construction_time = start.elapsed();

    let start = Instant::now();
    let lcp_result = match plcp_result.lcp_construction().single_threaded().run() {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let lcp_construction_time = start.elapsed();

    let (sa, lcp, _, _) = lcp_result.into_parts();

    // 3. Scan LCP array to find common substrings
    let start = Instant::now();
    let mut tokens: HashSet<Vec<u8>> = HashSet::new();
    let n = sa.len();

    let mut i = 1;
    while i < n {
        let current_lcp = lcp[i] as usize;
        if current_lcp < min_len {
            i += 1;
            continue;
        }

        let mut group_docs: HashSet<usize> = HashSet::new();
        group_docs.insert(doc_id[sa[i - 1] as usize]);

        let mut group_min_lcp = current_lcp;
        let group_start = i - 1;

        while i < n && (lcp[i] as usize) >= min_len {
            group_docs.insert(doc_id[sa[i] as usize]);
            group_min_lcp = group_min_lcp.min(lcp[i] as usize);
            i += 1;
        }

        if group_docs.len() >= min_docs {
            let pos = sa[group_start] as usize;
            let len = group_min_lcp.min(max_len);
            if pos + len <= concat.len() {
                let token = concat[pos..pos + len].to_vec();
                tokens.insert(token);
            }
        }
    }

    let lcp_scan_time = start.elapsed();

    // print all timings in one line
    println!(
        "Common Substring Discovery Metrics:\n\
        Concatenation: {:?} ({} bytes), SA: {:?}, PLCP: {:?}, LCP: {:?}, LCP Scan: {:?}",
        concatenation_time,
        concat.len(),
        sa_construction_time,
        plcp_construction_time,
        lcp_construction_time,
        lcp_scan_time
    );
    println!("Total: {:?}", total_start.elapsed());

    tokens.into_iter().collect()
}