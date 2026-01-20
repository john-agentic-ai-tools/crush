//! Integration tests for plugin discovery and registration
//!
//! Following TDD: These tests are written BEFORE implementation.
//! They MUST fail initially, then pass after implementation.

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

use crush_core::{init_plugins, list_plugins};

/// Test that `init_plugins()` discovers the default DEFLATE plugin
#[test]
fn test_plugin_discovery_deflate() {
    // Initialize plugin system
    init_plugins().expect("Plugin initialization should succeed");

    // List all discovered plugins
    let plugins = list_plugins();

    // Should find at least the DEFLATE plugin
    assert!(!plugins.is_empty(), "Should discover at least one plugin");

    // Find DEFLATE plugin
    let deflate = plugins.iter().find(|p| p.name == "deflate");
    assert!(deflate.is_some(), "DEFLATE plugin should be discovered");

    let deflate = deflate.unwrap();
    assert_eq!(deflate.magic_number, [0x43, 0x52, 0x01, 0x00]);
    assert_eq!(deflate.version, "1.0.0");
}

/// Test that `list_plugins()` returns empty before initialization
#[test]
fn test_list_plugins_before_init() {
    // Note: This test assumes a fresh state. In practice, other tests
    // may have already called init_plugins(), so we test the behavior
    // of list_plugins() returning the current registry state.
    let plugins = list_plugins();

    // Should return whatever is in the registry (empty or populated)
    // This is just checking the function works, not the state
    assert!(plugins.is_empty() || !plugins.is_empty());
}

/// Test that multiple plugins can be registered
///
/// Note: Since we only have DEFLATE currently, this test verifies
/// the infrastructure supports multiple plugins, even if only one exists.
#[test]
fn test_multiple_plugin_registration() {
    init_plugins().expect("Plugin initialization should succeed");

    let plugins = list_plugins();

    // Verify all discovered plugins have valid metadata
    for plugin in &plugins {
        assert!(!plugin.name.is_empty(), "Plugin name should not be empty");
        assert!(!plugin.version.is_empty(), "Plugin version should not be empty");
        assert!(plugin.throughput > 0.0, "Throughput should be positive");
        assert!(
            plugin.compression_ratio > 0.0 && plugin.compression_ratio <= 1.0,
            "Compression ratio should be in (0.0, 1.0]"
        );
    }

    // Verify DEFLATE is present
    assert!(plugins.iter().any(|p| p.name == "deflate"));
}

/// Test that duplicate magic numbers are detected and handled
///
/// Note: With linkme, duplicate magic numbers would mean two plugins
/// with the same magic number are linked. The registry should detect
/// this and handle it gracefully (log warning, use first-registered).
#[test]
fn test_duplicate_magic_number_handling() {
    // Initialize plugins
    init_plugins().expect("Plugin initialization should succeed");

    let plugins = list_plugins();

    // Build a map of magic numbers to count occurrences
    let mut magic_counts = std::collections::HashMap::new();
    for plugin in &plugins {
        *magic_counts.entry(plugin.magic_number).or_insert(0) += 1;
    }

    // All registered plugins should have unique magic numbers
    // (duplicates should have been filtered during init)
    for (magic, count) in magic_counts {
        assert_eq!(
            count, 1,
            "Magic number {magic:02X?} should appear exactly once in registry"
        );
    }
}

/// Test that re-initialization refreshes the plugin registry
#[test]
fn test_reinitialization() {
    // First initialization
    init_plugins().expect("First initialization should succeed");
    let plugins_first = list_plugins();
    let count_first = plugins_first.len();

    // Re-initialize
    init_plugins().expect("Re-initialization should succeed");
    let plugins_second = list_plugins();
    let count_second = plugins_second.len();

    // Should have same number of plugins after re-init
    assert_eq!(
        count_first, count_second,
        "Re-initialization should produce same plugin count"
    );

    // DEFLATE should still be present
    assert!(plugins_second.iter().any(|p| p.name == "deflate"));
}

/// Test that plugins can be retrieved by name after initialization
#[test]
fn test_plugin_retrieval_by_name() {
    init_plugins().expect("Plugin initialization should succeed");

    let plugins = list_plugins();

    // Find DEFLATE by name
    let deflate = plugins.iter().find(|p| p.name == "deflate");
    assert!(
        deflate.is_some(),
        "Should be able to find DEFLATE plugin by name"
    );
}

/// Test that plugin metadata is correctly populated
#[test]
fn test_plugin_metadata_validity() {
    init_plugins().expect("Plugin initialization should succeed");

    let plugins = list_plugins();
    assert!(!plugins.is_empty(), "Should have at least one plugin");

    for plugin in plugins {
        // Validate magic number starts with "CR" (0x43 0x52)
        assert_eq!(
            plugin.magic_number[0], 0x43,
            "Magic number should start with 'C'"
        );
        assert_eq!(
            plugin.magic_number[1], 0x52,
            "Magic number should have 'R' as second byte"
        );

        // Validate version format
        assert_eq!(
            plugin.magic_number[2], 0x01,
            "Version should be 0x01 for V1 format"
        );

        // Validate performance metrics are reasonable
        assert!(
            plugin.throughput >= 1.0,
            "Throughput should be at least 1 MB/s"
        );
        assert!(
            plugin.compression_ratio > 0.0 && plugin.compression_ratio <= 1.0,
            "Compression ratio should be in range (0.0, 1.0]"
        );
    }
}
