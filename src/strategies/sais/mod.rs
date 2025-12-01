mod common_substring_discovery;
mod suffix_array;

pub use suffix_array::SuffixArrayConfig;
pub use common_substring_discovery::{find_common_substrings, TokenSelectionMode, TokenDiscoveryResult};