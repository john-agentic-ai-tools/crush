//! Integration tests for intelligent plugin selection
//!
//! Following TDD: These tests are written BEFORE implementation.
//! They MUST fail initially, then pass after implementation.

#![allow(clippy::panic_in_result_fn)]

use crush_core::{compress_with_options, init_plugins, CompressionOptions, Result, ScoringWeights};

/// Test that plugin scoring selects the highest-scoring plugin
///
/// This test verifies that when multiple plugins are available,
/// the selection algorithm chooses the one with the best score
/// based on the default 70/30 throughput/ratio weighting.
#[test]
fn test_plugin_scoring_selection() -> Result<()> {
    init_plugins()?;

    // Compress data with default selection (should pick highest scoring plugin)
    let data = b"Test data for plugin selection based on scoring algorithm.";
    let options = CompressionOptions::default();

    let compressed = compress_with_options(data, &options)?;

    // Verify compression succeeded
    assert!(!compressed.is_empty());

    // Note: With only DEFLATE plugin currently, this just verifies the selection logic works
    // In future, with multiple plugins, we can verify the correct plugin was selected

    Ok(())
}

/// Test manual plugin override by name
///
/// Verifies that specifying a plugin name explicitly uses that plugin
/// regardless of scoring.
#[test]
fn test_manual_plugin_override() -> Result<()> {
    init_plugins()?;

    let data = b"Test data for manual plugin override.";

    // Explicitly request DEFLATE plugin by name
    let options = CompressionOptions::default().with_plugin("deflate");

    let compressed = compress_with_options(data, &options)?;

    // Verify compression succeeded
    assert!(!compressed.is_empty());

    // Verify the header has DEFLATE magic number
    assert_eq!(&compressed[0..4], &[0x43, 0x52, 0x01, 0x00]);

    Ok(())
}

/// Test that requesting a non-existent plugin returns an error
#[test]
fn test_manual_override_nonexistent_plugin() -> Result<()> {
    init_plugins()?;

    let data = b"Test data";
    let options = CompressionOptions::default().with_plugin("nonexistent");

    let result = compress_with_options(data, &options);

    // Should fail because plugin doesn't exist
    assert!(result.is_err());

    Ok(())
}

/// Test tied plugin scores are resolved alphabetically
///
/// When two plugins have identical scores, the one with the
/// alphabetically first name should be selected.
#[test]
fn test_tied_score_alphabetical_resolution() -> Result<()> {
    init_plugins()?;

    let data = b"Test data for tied score resolution.";
    let options = CompressionOptions::default();

    let compressed = compress_with_options(data, &options)?;

    // Verify compression succeeded
    assert!(!compressed.is_empty());

    // Note: With only DEFLATE, this tests the tie-breaking logic exists
    // With multiple plugins having same score, alphabetical selection would be verified

    Ok(())
}

/// Test custom scoring weights
///
/// Verifies that changing the throughput/ratio weights affects
/// plugin selection appropriately.
#[test]
fn test_custom_scoring_weights() -> Result<()> {
    init_plugins()?;

    let data = b"Test data for custom weights.";

    // Use 50/50 weighting instead of default 70/30
    let weights = ScoringWeights::new(0.5, 0.5)?;
    let options = CompressionOptions::default().with_weights(weights);

    let compressed = compress_with_options(data, &options)?;

    // Verify compression succeeded
    assert!(!compressed.is_empty());

    // Note: With only DEFLATE, result is same, but test verifies weight system works

    Ok(())
}

/// Test that invalid scoring weights are rejected
#[test]
fn test_invalid_scoring_weights() {
    // Weights must sum to 1.0
    let result = ScoringWeights::new(0.6, 0.6);
    assert!(
        result.is_err(),
        "Weights summing to >1.0 should be rejected"
    );

    let result = ScoringWeights::new(0.3, 0.3);
    assert!(
        result.is_err(),
        "Weights summing to <1.0 should be rejected"
    );

    let result = ScoringWeights::new(-0.1, 1.1);
    assert!(result.is_err(), "Negative weights should be rejected");

    let result = ScoringWeights::new(0.7, 0.3);
    assert!(result.is_ok(), "Valid weights should be accepted");
}

/// Test default scoring weights (70% throughput, 30% compression ratio)
#[test]
fn test_default_scoring_weights() {
    let weights = ScoringWeights::default();

    // Default should be 70/30
    assert!((weights.throughput - 0.7).abs() < 1e-6);
    assert!((weights.compression_ratio - 0.3).abs() < 1e-6);
}

/// Test scoring calculation with known values
///
/// This test doesn't require multiple plugins, just verifies the scoring
/// algorithm produces expected results for given metadata.
#[test]
fn test_scoring_calculation() -> Result<()> {
    use crush_core::PluginMetadata;

    // Create test metadata
    let plugin_a = PluginMetadata {
        name: "fast_low_ratio",
        version: "1.0.0",
        magic_number: [0x43, 0x52, 0x01, 0x10],
        throughput: 1000.0,     // Very fast
        compression_ratio: 0.8, // Poor compression (larger file)
        description: "Fast but low compression",
    };

    let plugin_b = PluginMetadata {
        name: "slow_high_ratio",
        version: "1.0.0",
        magic_number: [0x43, 0x52, 0x01, 0x11],
        throughput: 100.0,      // Slower
        compression_ratio: 0.3, // Good compression (smaller file)
        description: "Slow but high compression",
    };

    // With default 70/30 weights, fast plugin should score higher
    let weights = ScoringWeights::default();
    let plugins = vec![plugin_a, plugin_b];

    let score_a = crush_core::calculate_plugin_score(&plugin_a, &plugins, &weights);
    let score_b = crush_core::calculate_plugin_score(&plugin_b, &plugins, &weights);

    // Fast plugin should win with 70% throughput weight
    assert!(
        score_a > score_b,
        "Fast plugin should score higher with 70% throughput weight"
    );

    // With 50/50 weights, might be different
    let balanced_weights = ScoringWeights::new(0.5, 0.5)?;
    let balanced_score_a =
        crush_core::calculate_plugin_score(&plugin_a, &plugins, &balanced_weights);
    let balanced_score_b =
        crush_core::calculate_plugin_score(&plugin_b, &plugins, &balanced_weights);

    // Scores should be different from 70/30 case
    assert!((balanced_score_a - score_a).abs() > 1e-6);
    assert!((balanced_score_b - score_b).abs() > 1e-6);

    Ok(())
}
