mod common;

use common::*;
use predicates::prelude::*;
use filetime;

/// T026: Basic file compression test
#[test]
fn test_compress_basic_file() {
    let dir = test_dir();
    // Use larger data to ensure compression reduces size (overhead from header is ~16 bytes)
    let test_data = b"Hello, world! This is a test file for compression. ".repeat(20);
    let input = create_test_file(dir.path(), "test.txt", &test_data);
    let output = dir.path().join("test.txt.crush");

    // Run compress command (files are kept by default)
    crush_cmd()
        .arg("compress")
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

    // Compress (files are kept by default)
    crush_cmd()
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Verify both files exist
    assert_file_exists(&input);
    assert_file_exists(&output);
}

/// T050: Test that compressed file preserves mtime on Windows
#[test]
#[cfg(windows)]
fn test_compress_preserves_mtime_windows() {
    let dir = test_dir();
    let input = create_test_file(dir.path(), "test.txt", b"mtime test data");
    let output = dir.path().join("test.txt.crush");
    let restored_path = dir.path().join("restored.txt"); // Decompress will create this

    // 1. Set a specific, known modification time
    let original_mtime = filetime::FileTime::from_unix_time(1_500_000_000, 0); // A known past date
    filetime::set_file_mtime(&input, original_mtime).unwrap();

    // 2. Run compress command (files are kept by default)
    crush_cmd()
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // 3. Run decompress command
    crush_cmd()
        .arg("decompress")
        .arg(&output)
        .arg("-o")
        .arg(&restored_path)
        .assert()
        .success();

    // 4. Get the modification time of the restored file
    let restored_mtime = filetime::FileTime::from_last_modification_time(&std::fs::metadata(&restored_path).unwrap());

    // 5. Assert that the modification times are equal
    assert_eq!(original_mtime, restored_mtime, "Modification time should be preserved after roundtrip");
}

/// T048: Test mtime preservation on Linux
#[test]
#[cfg(target_os = "linux")]
fn test_compress_preserves_mtime_linux() {
    let dir = test_dir();
    let input = create_test_file(dir.path(), "test.txt", b"mtime test data for Linux");
    let output = dir.path().join("test.txt.crush");
    let restored_path = dir.path().join("restored.txt");

    // Set a specific modification time
    let original_mtime = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    filetime::set_file_mtime(&input, original_mtime).unwrap();

    // Compress
    crush_cmd()
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Decompress
    crush_cmd()
        .arg("decompress")
        .arg(&output)
        .arg("-o")
        .arg(&restored_path)
        .assert()
        .success();

    // Verify mtime is preserved
    let restored_mtime = filetime::FileTime::from_last_modification_time(&std::fs::metadata(&restored_path).unwrap());
    assert_eq!(original_mtime, restored_mtime, "Modification time should be preserved on Linux");
}

/// T049: Test mtime preservation on macOS
#[test]
#[cfg(target_os = "macos")]
fn test_compress_preserves_mtime_macos() {
    let dir = test_dir();
    let input = create_test_file(dir.path(), "test.txt", b"mtime test data for macOS");
    let output = dir.path().join("test.txt.crush");
    let restored_path = dir.path().join("restored.txt");

    // Set a specific modification time
    let original_mtime = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    filetime::set_file_mtime(&input, original_mtime).unwrap();

    // Compress
    crush_cmd()
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Decompress
    crush_cmd()
        .arg("decompress")
        .arg(&output)
        .arg("-o")
        .arg(&restored_path)
        .assert()
        .success();

    // Verify mtime is preserved
    let restored_mtime = filetime::FileTime::from_last_modification_time(&std::fs::metadata(&restored_path).unwrap());
    assert_eq!(original_mtime, restored_mtime, "Modification time should be preserved on macOS");
}

/// T051: Test Unix permissions preservation
#[test]
#[cfg(unix)]
fn test_compress_preserves_unix_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = test_dir();
    let input = create_test_file(dir.path(), "test.txt", b"permissions test data");
    let output = dir.path().join("test.txt.crush");
    let restored_path = dir.path().join("restored.txt");

    // Set specific permissions (rwxr-xr-x = 0o755)
    let original_perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&input, original_perms.clone()).unwrap();

    // Compress
    crush_cmd()
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Remove original
    std::fs::remove_file(&input).unwrap();

    // Decompress
    crush_cmd()
        .arg("decompress")
        .arg(&output)
        .arg("-o")
        .arg(&restored_path)
        .assert()
        .success();

    // Verify permissions are preserved
    let restored_perms = std::fs::metadata(&restored_path).unwrap().permissions();
    assert_eq!(
        original_perms.mode() & 0o777,
        restored_perms.mode() & 0o777,
        "Unix permissions should be preserved (expected: {:o}, got: {:o})",
        original_perms.mode() & 0o777,
        restored_perms.mode() & 0o777
    );
}
