//! Eligibility scoring for GPU compression plugin selection
//!
//! Evaluates three criteria to decide if the GPU plugin should claim a file:
//! 1. File size > 100 MB
//! 2. Compatible GPU available
//! 3. Shannon entropy ≤ 7.5 bits/byte
//!
//! If all three pass, score = 0.95 (high priority for GPU).
//! If any fails, score = 0.0 (decline — let another plugin handle it).

use crate::entropy::ENTROPY_THRESHOLD;

/// Minimum file size in bytes for GPU eligibility (100 MB).
pub const MIN_FILE_SIZE: u64 = 100 * 1024 * 1024;

/// Score assigned when all eligibility criteria pass.
pub const ELIGIBLE_SCORE: f64 = 0.95;

/// Input data for the eligibility scorer.
#[derive(Debug, Clone)]
pub struct EligibilityInput {
    /// File size in bytes.
    pub file_size: u64,
    /// Whether a compatible GPU was discovered.
    pub gpu_available: bool,
    /// Shannon entropy of the file header sample (bits/byte).
    pub entropy: f64,
}

/// Result of the eligibility evaluation.
#[derive(Debug, Clone)]
pub struct EligibilityResult {
    /// Overall eligibility score (0.0 or 0.95).
    pub score: f64,
    /// Whether the file size criterion passed.
    pub file_size_ok: bool,
    /// Whether a compatible GPU was found.
    pub gpu_ok: bool,
    /// Whether the entropy criterion passed.
    pub entropy_ok: bool,
}

impl EligibilityResult {
    /// Evaluate eligibility from the given input.
    #[must_use]
    pub fn evaluate(input: &EligibilityInput) -> Self {
        let file_size_ok = input.file_size > MIN_FILE_SIZE;
        let gpu_ok = input.gpu_available;
        let entropy_ok = input.entropy <= ENTROPY_THRESHOLD;

        let score = if file_size_ok && gpu_ok && entropy_ok {
            ELIGIBLE_SCORE
        } else {
            0.0
        };

        Self {
            score,
            file_size_ok,
            gpu_ok,
            entropy_ok,
        }
    }
}
