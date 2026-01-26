mod common;

use common::*;
use predicates::prelude::*;

/// T031: Basic file decompression test
#[test]
fn test_decompress_basic_file() {
    let dir = test_dir();
    let original = create_test_file(dir.path(), "original.txt", b"Hello, world! This is test data.");
    let compressed = dir.path().join("original.txt.crush");
    let decompressed = dir.path().join("original.txt");

    // First compress the file (files are kept by default)
    crush_cmd()
        .arg("compress")
        .arg(&original)
        .assert()
        .success();

    // Remove original file (simulates real scenario where input is gone)
    std::fs::remove_file(&original).unwrap();

    // Now decompress
    crush_cmd()
        .arg("decompress")
        .arg(&compressed)
        .assert()
        .success()
        .stdout(predicate::str::contains("Decompressed"));

    // Verify decompressed file exists
    assert_file_exists(&decompressed);

    // Verify content matches original
    let decompressed_content = read_file(&decompressed);
    assert_eq!(
        decompressed_content,
        b"Hello, world! This is test data.",
        "Decompressed content should match original"
    );
}

/// T032: CRC32 validation failure test
#[test]
fn test_decompress_crc32_failure() {
    let dir = test_dir();
    let original = create_test_file(dir.path(), "test.txt", b"Test data for CRC validation");
    let compressed = dir.path().join("test.txt.crush");

    // Compress the file first (files are kept by default)
    crush_cmd()
        .arg("compress")
        .arg(&original)
        .assert()
        .success();

    // Corrupt the compressed file by modifying a byte in the middle
    let mut data = read_file(&compressed);
    if data.len() > 20 {
        let mid = data.len() / 2;
        data[mid] ^= 0xFF; // Flip bits in the middle
        std::fs::write(&compressed, &data).unwrap();
    }

    // Remove original so decompression is the only way to get data back
    std::fs::remove_file(&original).unwrap();

    // Try to decompress - should fail CRC check
    crush_cmd()
        .arg("decompress")
        .arg(&compressed)
        .assert()
        .failure()
        .stderr(predicate::str::contains("CRC")
            .or(predicate::str::contains("checksum"))
            .or(predicate::str::contains("corrupt"))
            .or(predicate::str::contains("integrity")));
}

/// T052: Test that decompression handles missing metadata gracefully
#[test]
fn test_decompress_handles_missing_metadata_gracefully() {
    let dir = test_dir();
    let original = create_test_file(dir.path(), "original.txt", b"test data for metadata handling");
    let compressed = dir.path().join("original.txt.crush");
    let decompressed = dir.path().join("decompressed.txt");

    // Compress the file
    crush_cmd()
        .arg("compress")
        .arg(&original)
        .assert()
        .success();

    // Remove original
    std::fs::remove_file(&original).unwrap();

    // Decompress (should succeed even if metadata restoration fails)
    crush_cmd()
        .arg("decompress")
        .arg(&compressed)
        .arg("-o")
        .arg(&decompressed)
        .assert()
        .success();

    // Verify file was decompressed correctly
    assert_file_exists(&decompressed);
    let decompressed_content = read_file(&decompressed);
    assert_eq!(decompressed_content.as_slice(), b"test data for metadata handling");

    // Note: If metadata restoration fails, a warning should be logged,
    // but the operation should still succeed. The warning isn't checked here
    // because it goes to stderr and the test focuses on success behavior.
}

/// T033: Invalid header error test
#[test]
fn test_decompress_invalid_header() {
    let dir = test_dir();
    let invalid = create_test_file(dir.path(), "invalid.crush", b"Not a valid crush file header");

    crush_cmd()
        .arg("decompress")
        .arg(&invalid)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid")
            .or(predicate::str::contains("header"))
            .or(predicate::str::contains("format")));
}
