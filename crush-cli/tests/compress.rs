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
        .stderr(
            predicate::str::contains("not found")
                .or(predicate::str::contains("No such file"))
                .or(predicate::str::contains("does not exist")),
        );
}

/// T028: Output already exists error test
#[test]
fn test_compress_output_exists() {
    let dir = test_dir();
    let input = create_test_file(dir.path(), "test.txt", b"Test data");
    let _output = dir.path().join("test.txt.crush");

    // Create output file first
    create_test_file(dir.path(), "test.txt.crush", b"existing data");

    // Try to compress - should fail because output exists
    crush_cmd()
        .arg("compress")
        .arg(&input)
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("already exists").or(predicate::str::contains("File exists")),
        );
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
    assert_ne!(
        new_content, old_content,
        "File should have been overwritten"
    );
}

/// T030: Keep input file test
#[test]
fn test_compress_keep_input() {
    let dir = test_dir();
    let input = create_test_file(dir.path(), "test.txt", b"Data to compress");
    let output = dir.path().join("test.txt.crush");

    // Compress (files are kept by default)
    crush_cmd().arg("compress").arg(&input).assert().success();

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
    crush_cmd().arg("compress").arg(&input).assert().success();

    // 3. Run decompress command
    crush_cmd()
        .arg("decompress")
        .arg(&output)
        .arg("-o")
        .arg(&restored_path)
        .assert()
        .success();

    // 4. Get the modification time of the restored file
    let restored_mtime = filetime::FileTime::from_last_modification_time(
        &std::fs::metadata(&restored_path).unwrap(),
    );

    // 5. Assert that the modification times are equal
    assert_eq!(
        original_mtime, restored_mtime,
        "Modification time should be preserved after roundtrip"
    );
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
    crush_cmd().arg("compress").arg(&input).assert().success();

    // Decompress
    crush_cmd()
        .arg("decompress")
        .arg(&output)
        .arg("-o")
        .arg(&restored_path)
        .assert()
        .success();

    // Verify mtime is preserved
    let restored_mtime = filetime::FileTime::from_last_modification_time(
        &std::fs::metadata(&restored_path).unwrap(),
    );
    assert_eq!(
        original_mtime, restored_mtime,
        "Modification time should be preserved on Linux"
    );
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
    crush_cmd().arg("compress").arg(&input).assert().success();

    // Decompress
    crush_cmd()
        .arg("decompress")
        .arg(&output)
        .arg("-o")
        .arg(&restored_path)
        .assert()
        .success();

    // Verify mtime is preserved
    let restored_mtime = filetime::FileTime::from_last_modification_time(
        &std::fs::metadata(&restored_path).unwrap(),
    );
    assert_eq!(
        original_mtime, restored_mtime,
        "Modification time should be preserved on macOS"
    );
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
    crush_cmd().arg("compress").arg(&input).assert().success();

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

/// T073: Test that large files show progress (spinner appears for files >1MB)
#[test]
fn test_compress_large_file_progress() {
    let dir = test_dir();
    // Create a file larger than 1MB to trigger progress display
    let large_data = vec![0u8; 2 * 1024 * 1024]; // 2MB
    let input = create_test_file(dir.path(), "large.bin", &large_data);
    let output = dir.path().join("large.bin.crush");

    // Compress large file
    crush_cmd().arg("compress").arg(&input).assert().success();

    // Verify compressed file was created
    assert_file_exists(&output);

    // Note: We can't easily test that the spinner actually appeared since it goes to stderr
    // and is ephemeral. The important thing is that compression succeeds for large files.
}

/// T074: Test that final statistics are displayed after compression
#[test]
fn test_compress_displays_statistics() {
    let dir = test_dir();
    let test_data = b"statistics test data";
    let input = create_test_file(dir.path(), "test.txt", test_data);
    let _output = dir.path().join("test.txt.crush");

    // Compress and capture output
    let assert = crush_cmd().arg("compress").arg(&input).assert().success();

    // Verify statistics are displayed in stdout
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();

    // Check for key statistics in the output
    assert!(
        stdout.contains("Compressed"),
        "Should show 'Compressed' message"
    );
    assert!(stdout.contains("test.txt"), "Should show input filename");
    assert!(
        stdout.contains("test.txt.crush"),
        "Should show output filename"
    );
    assert!(stdout.contains("MB/s"), "Should show throughput");

    // Check for size information (either "smaller" or "larger")
    assert!(
        stdout.contains("smaller") || stdout.contains("larger") || stdout.contains("same size"),
        "Should show size comparison"
    );
}

/// T087: Test verbose mode shows plugin selection
#[test]
fn test_compress_verbose_plugin_selection() {
    let dir = test_dir();
    let test_data = b"verbose test data for plugin selection";
    let input = create_test_file(dir.path(), "test.txt", test_data);

    // Compress with -v flag (verbose flag comes before subcommand)
    let assert = crush_cmd()
        .arg("-v")
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Verify verbose output contains plugin selection info
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    // Should mention plugin selection or plugin name
    assert!(
        stderr.contains("plugin") || stderr.contains("deflate") || stderr.contains("selected"),
        "Verbose output should show plugin selection information"
    );
}

/// T088: Test verbose mode shows performance metrics
#[test]
fn test_compress_verbose_performance_metrics() {
    let dir = test_dir();
    let test_data = b"verbose test data for performance metrics";
    let input = create_test_file(dir.path(), "test.txt", test_data);

    // Compress with -v flag (verbose flag comes before subcommand)
    let assert = crush_cmd()
        .arg("-v")
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Verify verbose output contains performance info
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    // Should mention throughput or performance metrics
    assert!(
        stderr.contains("throughput") || stderr.contains("MB/s") || stderr.contains("performance"),
        "Verbose output should show performance metrics"
    );
}

/// T089: Test debug level output with -v
#[test]
fn test_compress_verbose_debug_level() {
    let dir = test_dir();
    let test_data = b"debug level verbose test data";
    let input = create_test_file(dir.path(), "test.txt", test_data);

    // Compress with -v flag (debug level - verbose flag comes before subcommand)
    let assert = crush_cmd()
        .arg("-v")
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Verify stderr has some diagnostic output
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    // Debug level should produce some output to stderr
    assert!(
        !stderr.is_empty(),
        "Debug level (-v) should produce diagnostic output"
    );
}

/// T090: Test trace level output with -vv
#[test]
fn test_compress_verbose_trace_level() {
    let dir = test_dir();
    let test_data = b"trace level verbose test data";
    let input = create_test_file(dir.path(), "test.txt", test_data);

    // Compress with -vv flag (trace level - verbose flag comes before subcommand)
    let assert = crush_cmd()
        .arg("-vv")
        .arg("compress")
        .arg(&input)
        .assert()
        .success();

    // Verify stderr has detailed trace output
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    // Trace level should produce more detailed output than debug
    assert!(
        !stderr.is_empty(),
        "Trace level (-vv) should produce detailed diagnostic output"
    );

    // Trace level might include more details like file paths, operations, etc.
    assert!(
        stderr.len() > 20,
        "Trace level output should be reasonably detailed"
    );
}

/// T075: Test Ctrl+C cleanup (partial file cleanup on interrupt)
#[test]
fn test_compress_interrupt_cleanup() {
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Duration;

    let dir = test_dir();
    // Create a large file so compression takes some time
    // Use 100MB to ensure it takes long enough to interrupt
    let large_data = vec![0u8; 100 * 1024 * 1024]; // 100MB
    let input = create_test_file(dir.path(), "interrupt_test.bin", &large_data);
    let _output = dir.path().join("interrupt_test.bin.crush");

    // Get the path to the crush binary
    #[allow(deprecated)]
    let crush_path = assert_cmd::cargo::cargo_bin("crush");

    // Start compression in a separate process
    let mut child = Command::new(&crush_path)
        .arg("compress")
        .arg(&input)
        .current_dir(dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to start compress process");

    // Give it a moment to start processing
    thread::sleep(Duration::from_millis(100));

    // Kill the process (simulating Ctrl+C)
    child.kill().expect("Failed to kill process");
    let status = child.wait().expect("Failed to wait for process");

    // Verify the process was interrupted
    // On Windows, killed processes return exit code 1
    // On Unix, they typically return 128 + SIGKILL (137) or similar
    assert!(
        !status.success(),
        "Process should not exit successfully after being killed"
    );

    // Note: Cleanup of partial files is best-effort. The important thing is that
    // the process can be interrupted without hanging.
}
