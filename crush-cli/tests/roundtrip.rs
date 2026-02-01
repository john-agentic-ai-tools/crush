mod common;

use common::*;

/// T034: Roundtrip test - compress then decompress preserves data exactly
#[test]
fn test_compress_decompress_roundtrip() -> std::io::Result<()> {
    let dir = test_dir();
    // Use larger data to ensure compression reduces size
    let original_content =
        b"This is test data that should survive a compression/decompression roundtrip perfectly. "
            .repeat(10);
    let original = create_test_file(dir.path(), "original.txt", &original_content);
    let compressed = dir.path().join("original.txt.crush");
    let decompressed = dir.path().join("original.txt");

    // Step 1: Compress (files are kept by default)
    crush_cmd()
        .arg("compress")
        .arg(&original)
        .assert()
        .success();

    // Verify compressed file exists and is smaller
    assert_file_exists(&compressed);
    assert_compressed(&original, &compressed);

    // Now remove original to test decompression
    std::fs::remove_file(&original)?;

    // Step 2: Decompress
    crush_cmd()
        .arg("decompress")
        .arg(&compressed)
        .assert()
        .success();

    // Verify decompressed file exists
    assert_file_exists(&decompressed);

    // Verify content matches exactly
    let decompressed_content = read_file(&decompressed);
    assert_eq!(
        decompressed_content.as_slice(),
        original_content.as_slice(),
        "Roundtrip should preserve data exactly"
    );

    Ok(())
}

/// Roundtrip test with larger random data
#[test]
fn test_roundtrip_large_random_file() -> std::io::Result<()> {
    let dir = test_dir();
    let original = create_random_file(dir.path(), "large.bin", 10 * 1024); // 10KB
    let original_content = read_file(&original);
    let compressed = dir.path().join("large.bin.crush");
    let decompressed = dir.path().join("large.bin");

    // Compress (files are kept by default now)
    crush_cmd()
        .arg("compress")
        .arg(&original)
        .assert()
        .success();

    assert_file_exists(&compressed);

    // Remove original to avoid conflict during decompression
    // (we already have original_content saved for verification)
    std::fs::remove_file(&original)?;

    // Decompress
    crush_cmd()
        .arg("decompress")
        .arg(&compressed)
        .assert()
        .success();

    // Verify exact match
    let decompressed_content = read_file(&decompressed);
    assert_eq!(
        decompressed_content, original_content,
        "Large file roundtrip should preserve all bytes"
    );

    Ok(())
}
