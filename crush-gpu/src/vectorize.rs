//! Vectorized (SIMD) string matching for improved compression ratios
//!
//! Provides an enhanced LZ77 matcher that uses longer hash chains and
//! wider match scanning for text-heavy data.  Activation is gated by a
//! heuristic that checks string density (printable ASCII ratio > 70%)
//! and entropy (< 6.0 bits/byte).

use crate::entropy::calculate_entropy;

/// Minimum string density (fraction of printable ASCII bytes) to activate
/// vectorized matching.
const MIN_STRING_DENSITY: f64 = 0.70;

/// Maximum entropy (bits/byte) to activate vectorized matching.
const MAX_ENTROPY_FOR_VECTORIZED: f64 = 6.0;

/// Sample size in bytes for the activation heuristic.
const SAMPLE_SIZE: usize = 1_048_576; // 1 MB

/// Decide whether vectorized matching should be used for the given data.
///
/// Returns `true` when the data has high printable ASCII density (>70%)
/// and low-enough entropy (<6.0 bits/byte), indicating text-heavy content
/// that benefits from deeper match searching.
#[must_use]
pub fn should_use_vectorized(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }

    let sample = if data.len() > SAMPLE_SIZE {
        &data[..SAMPLE_SIZE]
    } else {
        data
    };

    let density = string_density(sample);
    if density < MIN_STRING_DENSITY {
        return false;
    }

    let entropy = calculate_entropy(sample);
    entropy < MAX_ENTROPY_FOR_VECTORIZED
}

/// Compute the fraction of printable ASCII bytes (0x20..=0x7E plus
/// common whitespace: `\t`, `\n`, `\r`) in `data`.
#[allow(clippy::cast_precision_loss)]
fn string_density(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let printable = data
        .iter()
        .filter(|&&b| b == b'\t' || b == b'\n' || b == b'\r' || (0x20..=0x7E).contains(&b))
        .count();
    printable as f64 / data.len() as f64
}
