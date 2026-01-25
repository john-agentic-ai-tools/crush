mod common;

use common::*;
use predicates::prelude::*;
use serde_json;

/// T058: Basic file inspection
#[test]
fn test_inspect_basic() {
    let dir = test_dir();
    let test_data = b"Hello, world! This is a test file for inspection.".repeat(20);
    let input = create_test_file(dir.path(), "test.txt", &test_data);
    let output = dir.path().join("test.txt.crush");

    // Compress the file first
    crush_cmd()
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Now inspect it
    crush_cmd()
        .arg("inspect")
        .arg(&output)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Original size:")
                .and(predicate::str::contains("Compressed size:"))
                .and(predicate::str::contains("Compression ratio:"))
                .and(predicate::str::contains("Plugin:"))
                .and(predicate::str::contains("CRC32: VALID")),
        );
}

#[test]
fn test_inspect_crc_invalid() {
    let dir = test_dir();
    let test_data = b"Hello, world! This is a test file for inspection.".repeat(20);
    let input = create_test_file(dir.path(), "test.txt", &test_data);
    let output = dir.path().join("test.txt.crush");

    // Compress the file first
    crush_cmd()
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Corrupt the file
    let mut compressed_data = std::fs::read(&output).unwrap();
    if !compressed_data.is_empty() {
        let last_byte_index = compressed_data.len() - 1;
        compressed_data[last_byte_index] ^= 0xFF; // Flip the bits of the last byte
        std::fs::write(&output, &compressed_data).unwrap();
    }

    // Now inspect it
    crush_cmd()
        .arg("inspect")
        .arg(&output)
        .assert()
        .success()
        .stdout(predicate::str::contains("CRC32: INVALID"));
}

#[test]
fn test_inspect_invalid_header() {
    let dir = test_dir();
    let invalid_file = create_test_file(dir.path(), "invalid.crush", b"this is not a crush file");

    crush_cmd()
        .arg("inspect")
        .arg(&invalid_file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid magic number"));
}

#[test]
fn test_inspect_multiple_files_summary() {
    let dir = test_dir();

    // Create first compressed file
    let input1 = create_test_file(dir.path(), "file1.txt", b"data one");
    let output1 = dir.path().join("file1.txt.crush");
    crush_cmd().arg("compress").arg(&input1).assert().success();

    // Create second compressed file
    let input2 = create_test_file(dir.path(), "file2.txt", b"data two two");
    let output2 = dir.path().join("file2.txt.crush");
    crush_cmd().arg("compress").arg(&input2).assert().success();

    // Now inspect both with summary
    crush_cmd()
        .arg("inspect")
        .arg("--summary")
        .arg(&output1)
        .arg(&output2)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("--- Summary ---")
                .and(predicate::str::contains("File: "))
                .and(predicate::str::contains("Total Files: 2"))
                .and(predicate::str::contains("Total Original Size:"))
                .and(predicate::str::contains("Total Compressed Size:"))
                .and(predicate::str::contains("Overall Ratio:"))
                .and(predicate::str::contains("All CRC Valid: true")),
        );
}

#[test]
fn test_inspect_json_output() {
    let dir = test_dir();
    let test_data = b"json test data";
    let input = create_test_file(dir.path(), "test.txt", test_data);
    let output = dir.path().join("test.txt.crush");

    // Compress the file first
    crush_cmd()
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Now inspect it with JSON format
    let output_assert = crush_cmd()
        .arg("inspect")
        .arg("--format")
        .arg("json")
        .arg(&output)
        .assert()
        .success();

    // Check if the output is valid JSON and contains expected fields
    let stdout = String::from_utf8(output_assert.get_output().stdout.clone()).unwrap();
    let json_output: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("Invalid JSON output");

    assert_eq!(json_output.len(), 1);
    let item = &json_output[0];

    assert!(item["original_size"].is_number());
    assert!(item["compressed_size"].is_number());
    assert!(item["plugin_name"].is_string());
    assert!(item["crc_valid"].is_boolean());
    assert!(item["metadata"]["mtime"].is_number()); // mtime might be null if not set
}

