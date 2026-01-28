mod common;

use common::*;
use std::io::Write;
use std::process::{Command, Stdio};

/// Helper to get the crush binary path for std::process::Command
fn crush_bin_path() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("crush")
}

/// T151: Test stdin to file compression
#[test]
fn test_pipeline_stdin_to_file() {
    let dir = test_dir();
    let test_data = b"Hello from stdin! This is test data for compression. ".repeat(20);
    let output = dir.path().join("stdin_output.crush");

    // Run compress with stdin input and --output flag
    let mut child = Command::new(crush_bin_path())
        .arg("compress")
        .arg("--output")
        .arg(&output)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn compress command");

    // Write test data to stdin
    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(&test_data)
            .expect("Failed to write to stdin");
    }

    // Wait for completion
    let output_result = child
        .wait_with_output()
        .expect("Failed to wait for command");
    assert!(
        output_result.status.success(),
        "Command failed: {:?}",
        output_result
    );

    // Verify output file exists
    assert_file_exists(&output);

    // Decompress and verify roundtrip
    let decompressed = dir.path().join("decompressed.txt");
    crush_cmd()
        .arg("decompress")
        .arg(&output)
        .arg("--output")
        .arg(&decompressed)
        .assert()
        .success();

    let result = read_file(&decompressed);
    assert_eq!(
        result, test_data,
        "Decompressed data does not match original"
    );
}

/// T152: Test stdin to stdout compression
#[test]
fn test_pipeline_stdin_to_stdout() {
    let test_data = b"Hello from stdin to stdout! ".repeat(20);

    // Run compress with stdin and --stdout flag
    let mut child = Command::new(crush_bin_path())
        .arg("compress")
        .arg("--stdout")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn compress command");

    // Write test data to stdin
    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(&test_data)
            .expect("Failed to write to stdin");
    }

    // Read compressed data from stdout
    let output = child
        .wait_with_output()
        .expect("Failed to wait for command");
    assert!(output.status.success(), "Command failed");
    assert!(!output.stdout.is_empty(), "No data written to stdout");

    // Verify we got compressed data
    assert!(
        output.stdout.len() < test_data.len(),
        "Output should be compressed (smaller than input)"
    );
}

/// T153: Test file to stdout decompression
#[test]
fn test_pipeline_file_to_stdout() {
    let dir = test_dir();
    let test_data = b"Hello for file to stdout decompression! ".repeat(20);
    let input = create_test_file(dir.path(), "test.txt", &test_data);
    let compressed = dir.path().join("test.txt.crush");

    // First compress the file
    crush_cmd().arg("compress").arg(&input).assert().success();

    assert_file_exists(&compressed);

    // Decompress to stdout
    let output = crush_cmd()
        .arg("decompress")
        .arg(&compressed)
        .arg("--stdout")
        .output()
        .expect("Failed to run decompress command");

    assert!(output.status.success(), "Decompress command failed");
    assert_eq!(
        output.stdout, test_data,
        "Decompressed stdout data does not match original"
    );
}

/// T154: Test full pipeline (compress stdin | decompress stdout)
#[test]
fn test_pipeline_full_roundtrip() {
    let test_data = b"Full pipeline test data! ".repeat(20);

    // Compress: stdin to stdout
    let mut compress_child = Command::new(crush_bin_path())
        .arg("compress")
        .arg("--stdout")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn compress command");

    // Write test data to compress stdin
    {
        let stdin = compress_child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(&test_data)
            .expect("Failed to write to stdin");
    }

    // Get compressed output
    let compress_output = compress_child
        .wait_with_output()
        .expect("Failed to wait for compress");
    assert!(compress_output.status.success(), "Compress failed");

    // Decompress: stdin to stdout
    let mut decompress_child = Command::new(crush_bin_path())
        .arg("decompress")
        .arg("--stdout")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn decompress command");

    // Write compressed data to decompress stdin
    {
        let stdin = decompress_child
            .stdin
            .as_mut()
            .expect("Failed to open stdin");
        stdin
            .write_all(&compress_output.stdout)
            .expect("Failed to write to stdin");
    }

    // Get decompressed output
    let decompress_output = decompress_child
        .wait_with_output()
        .expect("Failed to wait for decompress");
    assert!(decompress_output.status.success(), "Decompress failed");
    assert_eq!(
        decompress_output.stdout, test_data,
        "Full pipeline roundtrip failed"
    );
}

/// T155: Test progress bars are hidden when using stdin
#[test]
fn test_pipeline_no_progress_bars_on_stdin() {
    let test_data = b"Test data for progress bar check. ".repeat(20);

    // Run compress with stdin - should not show progress bars
    let mut child = Command::new(crush_bin_path())
        .arg("compress")
        .arg("--stdout")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn compress command");

    // Write test data to stdin
    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");
        stdin
            .write_all(&test_data)
            .expect("Failed to write to stdin");
    }

    // Check output
    let output = child
        .wait_with_output()
        .expect("Failed to wait for command");
    assert!(output.status.success(), "Command failed");

    // Convert stderr to string
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should not contain progress bar indicators
    // Progress bars typically contain characters like: [, ], =, >, %
    // But we need to be careful not to flag normal log messages
    // So we check for typical progress bar patterns
    assert!(
        !stderr.contains(" MB/s]"),
        "Progress bar detected in stderr: {}",
        stderr
    );
    assert!(
        !stderr.contains("[===="),
        "Progress bar detected in stderr: {}",
        stderr
    );
}
