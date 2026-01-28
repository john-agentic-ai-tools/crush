mod common;

use common::*;
use predicates::prelude::*;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

// Global counter for unique test config files
static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Helper function to set up isolated config file for a test
/// Returns (temp_dir, config_path_string) that must be kept alive for the test duration
fn setup_test_config() -> (tempfile::TempDir, String) {
    let counter = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join(format!("config-{}.toml", counter));
    let config_path_str = config_path.to_str().expect("Invalid path").to_string();

    (temp_dir, config_path_str)
}

/// Helper to create a crush command with the test config environment variable
fn crush_cmd_with_config(config_path: &str) -> assert_cmd::Command {
    let mut cmd = crush_cmd();
    cmd.env("CRUSH_TEST_CONFIG_FILE", config_path);
    cmd
}

/// T099: Test config set and get
#[test]
fn test_config_set_and_get() {
    let (_temp_dir, config_path) = setup_test_config();

    // Set a config value
    crush_cmd_with_config(&config_path)
        .arg("config")
        .arg("set")
        .arg("compression.level")
        .arg("fast")
        .assert()
        .success();

    // Get the value back
    let assert = crush_cmd_with_config(&config_path)
        .arg("config")
        .arg("get")
        .arg("compression.level")
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("fast"), "Should show the set value");
}

/// T100: Test config list shows all settings
#[test]
fn test_config_list() {
    let (_temp_dir, config_path) = setup_test_config();

    // List all config settings
    let assert = crush_cmd_with_config(&config_path)
        .arg("config")
        .arg("list")
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    // Should contain all major sections
    assert!(
        stdout.contains("[compression]"),
        "Should show compression section"
    );
    assert!(stdout.contains("[output]"), "Should show output section");
    assert!(stdout.contains("[logging]"), "Should show logging section");

    // Should show default values
    assert!(
        stdout.contains("balanced") || stdout.contains("default-plugin"),
        "Should show compression settings"
    );
}

/// T101: Test config reset restores defaults
#[test]
fn test_config_reset() {
    let (_temp_dir, config_path) = setup_test_config();
    // Set a non-default value
    crush_cmd_with_config(&config_path)
        .arg("config")
        .arg("set")
        .arg("compression.level")
        .arg("best")
        .assert()
        .success();

    // Reset config (with --yes to skip confirmation)
    crush_cmd_with_config(&config_path)
        .arg("config")
        .arg("reset")
        .arg("--yes")
        .assert()
        .success();

    // Verify default value is restored
    let assert = crush_cmd_with_config(&config_path)
        .arg("config")
        .arg("get")
        .arg("compression.level")
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("balanced"),
        "Should restore default value 'balanced'"
    );
}

/// T102: Test invalid key error
#[test]
fn test_config_invalid_key() {
    let (_temp_dir, config_path) = setup_test_config();
    crush_cmd_with_config(&config_path)
        .arg("config")
        .arg("get")
        .arg("invalid.key.path")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Invalid config key")
                .or(predicate::str::contains("unknown"))
                .or(predicate::str::contains("not found")),
        );
}

/// T103: Test invalid value error
#[test]
fn test_config_invalid_value() {
    let (_temp_dir, config_path) = setup_test_config();
    crush_cmd_with_config(&config_path)
        .arg("config")
        .arg("set")
        .arg("compression.level")
        .arg("invalid_level")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid").or(predicate::str::contains("must be")));
}

/// T104: Test config affects compression
#[test]
fn test_config_affects_compression() {
    let (_temp_dir, config_path) = setup_test_config();
    let dir = test_dir();
    let test_data = b"config test data";
    let input = create_test_file(dir.path(), "test.txt", test_data);

    // Set config to use fast level
    crush_cmd_with_config(&config_path)
        .arg("config")
        .arg("set")
        .arg("compression.level")
        .arg("fast")
        .assert()
        .success();

    // Compress a file (should use the configured level)
    crush_cmd_with_config(&config_path)
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Verify compressed file was created
    let output = dir.path().join("test.txt.crush");
    assert_file_exists(&output);
}
