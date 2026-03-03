//! Eligibility scorer and entropy threshold tests

use crush_gpu::entropy::calculate_entropy;
use crush_gpu::scorer::{EligibilityInput, EligibilityResult};
use crush_gpu::vectorize::should_use_vectorized;

// T013: Shannon entropy calculator tests

#[test]
fn test_entropy_all_zeros() {
    let data = vec![0u8; 10000];
    let entropy = calculate_entropy(&data);
    assert!(
        entropy.abs() < 0.001,
        "All-zero data should have entropy ~0.0, got {entropy}"
    );
}

#[test]
fn test_entropy_single_byte_repeated() {
    let data = vec![42u8; 10000];
    let entropy = calculate_entropy(&data);
    assert!(
        entropy.abs() < 0.001,
        "Single repeated byte should have entropy ~0.0, got {entropy}"
    );
}

#[test]
fn test_entropy_two_equal_values() {
    // Two values with equal frequency → entropy = 1.0 bit/byte
    let mut data = Vec::with_capacity(10000);
    for i in 0..10000 {
        data.push(u8::from(i % 2 != 0));
    }
    let entropy = calculate_entropy(&data);
    assert!(
        (entropy - 1.0).abs() < 0.01,
        "Two equal-frequency values should have entropy ~1.0, got {entropy}"
    );
}

#[test]
fn test_entropy_uniform_random() {
    // Simulate uniform distribution (all 256 byte values equally likely)
    let mut data = Vec::with_capacity(256 * 100);
    for _ in 0..100 {
        for b in 0..=255u8 {
            data.push(b);
        }
    }
    let entropy = calculate_entropy(&data);
    assert!(
        (entropy - 8.0).abs() < 0.01,
        "Uniform distribution should have entropy ~8.0, got {entropy}"
    );
}

#[test]
fn test_entropy_english_text_range() {
    // English text typically has entropy between 3.5-5.5 bits/byte
    let text = b"The quick brown fox jumps over the lazy dog. \
                 This is a sample of English text that should have \
                 moderate entropy, somewhere around 4 to 5 bits per byte. \
                 Repetition of common words like the, is, a, and of \
                 reduces entropy compared to random data.";
    let repeated: Vec<u8> = text.iter().copied().cycle().take(10000).collect();
    let entropy = calculate_entropy(&repeated);
    assert!(
        (3.0..=6.0).contains(&entropy),
        "English text should have entropy 3.0-6.0, got {entropy}"
    );
}

#[test]
fn test_entropy_below_threshold_is_suitable() {
    // English text should be below 7.5 threshold
    let text = b"Compression test data with lots of repeated patterns and words. ";
    let repeated: Vec<u8> = text.iter().copied().cycle().take(10000).collect();
    let entropy = calculate_entropy(&repeated);
    assert!(
        entropy <= 7.5,
        "Compressible text should be below 7.5 threshold, got {entropy}"
    );
}

#[test]
fn test_entropy_above_threshold_is_unsuitable() {
    // Uniform random data should be above 7.5 threshold
    let mut data = Vec::with_capacity(256 * 100);
    for _ in 0..100 {
        for b in 0..=255u8 {
            data.push(b);
        }
    }
    let entropy = calculate_entropy(&data);
    assert!(
        entropy > 7.5,
        "Random data should be above 7.5 threshold, got {entropy}"
    );
}

#[test]
fn test_entropy_empty_data() {
    let data: Vec<u8> = vec![];
    let entropy = calculate_entropy(&data);
    assert!(
        entropy.abs() < 0.001,
        "Empty data should have entropy 0.0, got {entropy}"
    );
}

// T046: Eligibility scorer unit tests

#[test]
fn test_scorer_small_file_rejected() {
    // File under 100MB → score 0.0
    let input = EligibilityInput {
        file_size: 50_000_000, // 50 MB
        gpu_available: true,
        entropy: 4.5,
    };
    let result = EligibilityResult::evaluate(&input);
    assert!(
        result.score.abs() < f64::EPSILON,
        "File under 100MB should score 0.0, got {}",
        result.score
    );
    assert!(!result.file_size_ok);
}

#[test]
fn test_scorer_no_gpu_rejected() {
    // No GPU → score 0.0
    let input = EligibilityInput {
        file_size: 200_000_000,
        gpu_available: false,
        entropy: 4.5,
    };
    let result = EligibilityResult::evaluate(&input);
    assert!(
        result.score.abs() < f64::EPSILON,
        "No GPU should score 0.0, got {}",
        result.score
    );
    assert!(!result.gpu_ok);
}

#[test]
fn test_scorer_high_entropy_rejected() {
    // Entropy > 7.5 → score 0.0
    let input = EligibilityInput {
        file_size: 200_000_000,
        gpu_available: true,
        entropy: 7.9,
    };
    let result = EligibilityResult::evaluate(&input);
    assert!(
        result.score.abs() < f64::EPSILON,
        "High entropy should score 0.0, got {}",
        result.score
    );
    assert!(!result.entropy_ok);
}

#[test]
fn test_scorer_all_pass() {
    // All criteria pass → score 0.95
    let input = EligibilityInput {
        file_size: 200_000_000,
        gpu_available: true,
        entropy: 4.5,
    };
    let result = EligibilityResult::evaluate(&input);
    assert!(
        (result.score - 0.95).abs() < f64::EPSILON,
        "All pass should score 0.95, got {}",
        result.score
    );
    assert!(result.file_size_ok);
    assert!(result.gpu_ok);
    assert!(result.entropy_ok);
}

#[test]
fn test_scorer_boundary_100mb() {
    // Exactly 100MB → should fail (>100MB required, not >=)
    let input = EligibilityInput {
        file_size: 100 * 1024 * 1024,
        gpu_available: true,
        entropy: 4.5,
    };
    let result = EligibilityResult::evaluate(&input);
    assert!(
        result.score.abs() < f64::EPSILON,
        "Exactly 100MB should be rejected, got {}",
        result.score
    );
}

#[test]
fn test_scorer_boundary_entropy_7_5() {
    // Exactly 7.5 entropy → should pass (≤ 7.5)
    let input = EligibilityInput {
        file_size: 200_000_000,
        gpu_available: true,
        entropy: 7.5,
    };
    let result = EligibilityResult::evaluate(&input);
    assert!(
        (result.score - 0.95).abs() < f64::EPSILON,
        "Entropy exactly 7.5 should pass, got {}",
        result.score
    );
}

// T060: Vectorized matching activation heuristic tests

#[test]
fn test_vectorized_activates_for_high_string_density_low_entropy() {
    // CSV/log data: high printable ASCII ratio, low entropy
    let line = b"2026-02-23,INFO,user_service,login_success,user=alice,ip=10.0.0.1\n";
    let data: Vec<u8> = line.iter().copied().cycle().take(10_000).collect();
    assert!(
        should_use_vectorized(&data),
        "CSV data should activate vectorized matching"
    );
}

#[test]
#[allow(clippy::cast_possible_truncation)]
fn test_vectorized_skips_binary_data() {
    // Binary data: low printable ASCII ratio — values 0..=255 repeating
    let data: Vec<u8> = (0..10_000u16).map(|i| (i % 256) as u8).collect();
    assert!(
        !should_use_vectorized(&data),
        "Binary data should NOT activate vectorized matching"
    );
}

#[test]
#[allow(clippy::cast_possible_truncation)]
fn test_vectorized_skips_high_entropy_text() {
    // High entropy text (many unique characters, randomized)
    // Even if string density is high, high entropy means poor match potential
    let data: Vec<u8> = (0u32..10_000)
        .map(|i| {
            // All printable but pseudo-random selection across the 95 printable chars
            // Safe: result is always in 0x20..=0x7E (printable ASCII range)
            let v = (i.wrapping_mul(31).wrapping_add(17) % 95) as u8;
            0x20u8.wrapping_add(v)
        })
        .collect();
    // If entropy is high enough, vectorized should not activate
    let entropy = calculate_entropy(&data);
    if entropy >= 6.0 {
        assert!(
            !should_use_vectorized(&data),
            "High-entropy text (entropy={entropy}) should NOT activate vectorized matching"
        );
    }
}

#[test]
fn test_vectorized_activates_for_english_text() {
    // Typical English text: high string density, moderate entropy
    let text = b"The quick brown fox jumps over the lazy dog. \
                 This is a test of the vectorized matching system. \
                 Repeated patterns help compression a lot. ";
    let data: Vec<u8> = text.iter().copied().cycle().take(10_000).collect();
    assert!(
        should_use_vectorized(&data),
        "English text should activate vectorized matching"
    );
}

// T047: Entropy threshold tests for specific data types

#[test]
fn test_scorer_encrypted_file_rejected() {
    // Encrypted data has entropy ~8.0
    let input = EligibilityInput {
        file_size: 200_000_000,
        gpu_available: true,
        entropy: 7.98,
    };
    let result = EligibilityResult::evaluate(&input);
    assert!(
        result.score.abs() < f64::EPSILON,
        "Encrypted file should be rejected"
    );
}

#[test]
fn test_scorer_jpeg_rejected() {
    // JPEG files typically have entropy ~7.8
    let input = EligibilityInput {
        file_size: 200_000_000,
        gpu_available: true,
        entropy: 7.8,
    };
    let result = EligibilityResult::evaluate(&input);
    assert!(result.score.abs() < f64::EPSILON, "JPEG should be rejected");
}

#[test]
fn test_scorer_csv_accepted() {
    // CSV/log files typically have entropy ~4.5
    let input = EligibilityInput {
        file_size: 200_000_000,
        gpu_available: true,
        entropy: 4.5,
    };
    let result = EligibilityResult::evaluate(&input);
    assert!(
        (result.score - 0.95).abs() < f64::EPSILON,
        "CSV file should be accepted"
    );
}

#[test]
fn test_scorer_binary_executable_accepted() {
    // Binary executables typically have entropy ~6.5
    let input = EligibilityInput {
        file_size: 200_000_000,
        gpu_available: true,
        entropy: 6.5,
    };
    let result = EligibilityResult::evaluate(&input);
    assert!(
        (result.score - 0.95).abs() < f64::EPSILON,
        "Binary executable should be accepted"
    );
}
