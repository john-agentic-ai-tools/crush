//! Shannon entropy calculation for data suitability assessment
//!
//! Computes the Shannon entropy of a byte stream over a 256-bucket
//! frequency distribution.  The result is in **bits per byte** and
//! ranges from 0.0 (all identical bytes) to 8.0 (perfectly uniform
//! distribution).
//!
//! The GPU plugin uses entropy as one of three eligibility gates.
//! Data whose entropy exceeds 7.5 bits/byte is considered
//! incompressible and routed to a different plugin.

/// Maximum number of bytes to sample for entropy calculation.
/// Sampling more than 1 MB gives negligible accuracy gain.
const MAX_SAMPLE: usize = 1_048_576;

/// Calculate the Shannon entropy (bits/byte) of `data`.
///
/// If `data` is larger than 1 MB only the first 1 MB is sampled.
/// Empty input returns 0.0.
#[must_use]
pub fn calculate_entropy(data: &[u8]) -> f64 {
    let sample = if data.len() > MAX_SAMPLE {
        &data[..MAX_SAMPLE]
    } else {
        data
    };

    let n = sample.len();
    if n == 0 {
        return 0.0;
    }

    // Count byte frequencies in 256 buckets.
    let mut counts = [0u64; 256];
    for &b in sample {
        counts[b as usize] += 1;
    }

    #[allow(clippy::cast_precision_loss)]
    let n_f64 = n as f64;
    let mut entropy = 0.0_f64;

    for &c in &counts {
        if c == 0 {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let p = c as f64 / n_f64;
        entropy -= p * p.log2();
    }

    entropy
}

/// Default entropy threshold for GPU eligibility.
///
/// Data whose entropy is **at or below** this value is considered
/// compressible enough to benefit from GPU-accelerated compression.
pub const ENTROPY_THRESHOLD: f64 = 7.5;

/// Return `true` if `data` is below the entropy threshold.
#[must_use]
pub fn is_compressible(data: &[u8]) -> bool {
    calculate_entropy(data) <= ENTROPY_THRESHOLD
}
