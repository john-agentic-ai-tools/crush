mod common;

use common::*;
use predicates::prelude::*;

/// T026: Basic file compression test
#[test]
fn test_compress_basic_file() {
    let dir = test_dir();
    // Use larger data to ensure compression reduces size (overhead from header is ~16 bytes)
    let test_data = b"Hello, world! This is a test file for compression. ".repeat(20);
    let input = create_test_file(dir.path(), "test.txt", &test_data);
    let output = dir.path().join("test.txt.crush");

    // Run compress command with --keep to preserve input file for testing
    crush_cmd()
        .arg("compress")
        .arg("--keep")
        .arg(&input)
        .assert()
        .success()
        .stdout(predicate::str::contains("Compressed"));

    // Verify output file exists
    assert_file_exists(&output);

    // Verify output is smaller than input (compression worked)
    assert_compressed(&input, &output);
}

/// T027: File not found error test
#[test]
fn test_compress_file_not_found() {
    crush_cmd()
        .arg("compress")
        .arg("nonexistent.txt")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found")
            .or(predicate::str::contains("No such file"))
            .or(predicate::str::contains("does not exist")));
}

/// T028: Output already exists error test
#[test]
fn test_compress_output_exists() {
    let dir = test_dir();
    let input = create_test_file(dir.path(), "test.txt", b"Test data");
    let output = dir.path().join("test.txt.crush");

    // Create output file first
    create_test_file(dir.path(), "test.txt.crush", b"existing data");

    // Try to compress - should fail because output exists
    crush_cmd()
        .arg("compress")
        .arg(&input)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists")
            .or(predicate::str::contains("File exists")));
}

/// T029: Force overwrite test
#[test]
fn test_compress_force_overwrite() {
    let dir = test_dir();
    let input = create_test_file(dir.path(), "test.txt", b"New test data for compression");
    let output = dir.path().join("test.txt.crush");

    // Create existing output file
    let old_content = b"old compressed data";
    create_test_file(dir.path(), "test.txt.crush", old_content);

    // Compress with --force flag
    crush_cmd()
        .arg("compress")
        .arg("--force")
        .arg(&input)
        .assert()
        .success();

    // Verify output exists
    assert_file_exists(&output);

    // Verify output has changed (not the old content)
    let new_content = read_file(&output);
    assert_ne!(new_content, old_content, "File should have been overwritten");
}

/// T030: Keep input file test
#[test]
fn test_compress_keep_input() {
    let dir = test_dir();
    let input = create_test_file(dir.path(), "test.txt", b"Data to compress");
    let output = dir.path().join("test.txt.crush");

    // Compress with --keep flag
    crush_cmd()
        .arg("compress")
        .arg("--keep")
        .arg(&input)
        .assert()
        .success();

    // Verify both files exist
    assert_file_exists(&input);
    assert_file_exists(&output);
}
