mod common;

use common::*;
use std::fs;

/// T137: Test JSON format output
#[test]
fn test_logging_json_format() {
    let dir = test_dir();
    let test_data = b"json logging test data";
    let input = create_test_file(dir.path(), "test.txt", test_data);
    let log_file = dir.path().join("test.log");

    // Compress with JSON logging
    let assert = crush_cmd()
        .arg("--log-format")
        .arg("json")
        .arg("--log-file")
        .arg(&log_file)
        .arg("-v")
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Verify log file was created
    assert!(log_file.exists(), "Log file should be created");

    // Read log file
    let log_contents = fs::read_to_string(&log_file).expect("Should read log file");

    // Verify JSON format (should have opening brace)
    assert!(log_contents.contains("{"), "Should contain JSON objects");
    assert!(
        log_contents.contains("\"timestamp\"") || log_contents.contains("\"time\""),
        "Should have timestamp field"
    );
    assert!(
        log_contents.contains("\"level\"") || log_contents.contains("\"severity\""),
        "Should have level field"
    );
    assert!(
        log_contents.contains("\"message\"") || log_contents.contains("\"msg\""),
        "Should have message field"
    );
}

/// T138: Test error context in logs
#[test]
fn test_logging_error_context() {
    let dir = test_dir();
    let log_file = dir.path().join("error.log");

    // Try to compress non-existent file (will error)
    let assert = crush_cmd()
        .arg("--log-file")
        .arg(&log_file)
        .arg("-v")
        .arg("compress")
        .arg("nonexistent_file_that_does_not_exist.txt")
        .assert()
        .failure();

    // Verify stderr contains error
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("not found") || stderr.contains("does not exist"),
        "Should show error message"
    );

    // If log file was created, verify it contains error context
    if log_file.exists() {
        let log_contents = fs::read_to_string(&log_file).expect("Should read log file");
        // Log should have some context (filename, error type, etc.)
        assert!(
            log_contents.contains("nonexistent") || log_contents.contains("Starting"),
            "Log should contain context about the operation"
        );
    }
}

/// T139: Test log file creation
#[test]
fn test_logging_file_creation() {
    let dir = test_dir();
    let test_data = b"log file test data";
    let input = create_test_file(dir.path(), "test.txt", test_data);
    let log_file = dir.path().join("compression.log");

    // Ensure log file doesn't exist initially
    assert!(!log_file.exists(), "Log file should not exist before test");

    // Compress with log file
    crush_cmd()
        .arg("--log-file")
        .arg(&log_file)
        .arg("-v")
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Verify log file was created
    assert!(log_file.exists(), "Log file should be created");

    // Verify log file contains log entries
    let log_contents = fs::read_to_string(&log_file).expect("Should read log file");
    assert!(!log_contents.is_empty(), "Log file should not be empty");

    // Should contain operation information
    assert!(
        log_contents.contains("compress")
            || log_contents.contains("Starting")
            || log_contents.contains("Compressed"),
        "Log should contain compression operation info"
    );
}
