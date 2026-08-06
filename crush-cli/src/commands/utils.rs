//! Common utility functions for compress and decompress commands

use crate::error::{CliError, Result};
use crush_core::cancel::CancellationToken;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Check if cancellation has been requested
///
/// # Errors
///
/// Returns `CliError::Interrupted` if cancellation was requested
pub fn check_cancelled(token: &Arc<dyn CancellationToken>) -> Result<()> {
    if token.is_cancelled() {
        Err(CliError::Interrupted)
    } else {
        Ok(())
    }
}

/// Check for cancellation and clean up output file if cancelled
///
/// # Errors
///
/// Returns `CliError::Interrupted` if cancellation was requested
pub fn check_cancelled_with_cleanup(
    token: &Arc<dyn CancellationToken>,
    output_path: &Path,
) -> Result<()> {
    if token.is_cancelled() {
        let _ = fs::remove_file(output_path);
        Err(CliError::Interrupted)
    } else {
        Ok(())
    }
}

/// Validate that the input file exists and is readable
///
/// # Errors
///
/// Returns an error if the file doesn't exist, is a directory, or has invalid permissions
pub fn validate_input(path: &Path) -> Result<()> {
    // Check if file exists
    if !path.exists() {
        return Err(CliError::InvalidInput(format!(
            "Input file does not exist: {}",
            path.display()
        )));
    }

    // Check if it's a file (not a directory)
    if !path.is_file() {
        return Err(CliError::InvalidInput(format!(
            "Input path is not a file: {}",
            path.display()
        )));
    }

    // Check if file is readable
    match fs::metadata(path) {
        Ok(metadata) => {
            if metadata.len() == 0 {
                return Err(CliError::InvalidInput(format!(
                    "Input file is empty: {}",
                    path.display()
                )));
            }
        }
        Err(e) => {
            return Err(CliError::InvalidInput(format!(
                "Cannot read input file {}: {}",
                path.display(),
                e
            )));
        }
    }

    Ok(())
}

/// Validate that the output file can be written
///
/// # Errors
///
/// Returns an error if the output file already exists (and force is false)
/// or if the parent directory doesn't exist
pub fn validate_output(path: &Path, force: bool) -> Result<()> {
    // Check if output file already exists
    if path.exists() && !force {
        return Err(CliError::InvalidInput(format!(
            "Output file already exists: {}. Use --force to overwrite.",
            path.display()
        )));
    }

    // Check if parent directory exists
    if let Some(parent) = path.parent() {
        // parent() returns "" for relative paths in the current directory, which is valid
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(CliError::InvalidInput(format!(
                "Output directory does not exist: {}",
                parent.display()
            )));
        }
    }

    Ok(())
}

/// Calculate throughput in MB/s
#[must_use]
pub fn calculate_throughput_mbps(size_bytes: u64, duration: Duration) -> f64 {
    if duration.as_secs_f64() > 0.0 {
        (size_bytes as f64 / (1024.0 * 1024.0)) / duration.as_secs_f64()
    } else {
        0.0
    }
}

/// Calculate compression ratio as a percentage
#[must_use]
pub fn calculate_compression_ratio(input_size: u64, output_size: u64) -> f64 {
    if input_size > 0 {
        (output_size as f64 / input_size as f64) * 100.0
    } else {
        0.0
    }
}

/// Write data to stdout and flush
///
/// # Errors
///
/// Returns an error if writing or flushing fails
pub fn write_to_stdout(data: &[u8]) -> Result<()> {
    io::stdout().write_all(data)?;
    io::stdout().flush()?;
    Ok(())
}

/// Write data to file with automatic cleanup on error
///
/// If the write fails, attempts to remove the partial file before returning the error.
///
/// # Errors
///
/// Returns an error if the write operation fails
pub fn write_with_cleanup(path: &Path, data: &[u8]) -> Result<()> {
    if let Err(e) = fs::write(path, data) {
        let _ = fs::remove_file(path);
        Err(e.into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crush_core::cancel::AtomicCancellationToken;
    use std::time::Duration;
    use tempfile::NamedTempFile;

    #[test]
    fn test_check_cancelled_not_cancelled() {
        let token: Arc<dyn CancellationToken> = Arc::new(AtomicCancellationToken::new());
        assert!(check_cancelled(&token).is_ok());
    }

    #[test]
    fn test_check_cancelled_is_cancelled() {
        let token: Arc<dyn CancellationToken> = Arc::new(AtomicCancellationToken::new());
        token.cancel();
        assert!(matches!(
            check_cancelled(&token),
            Err(CliError::Interrupted)
        ));
    }

    #[test]
    fn test_calculate_throughput() {
        let throughput = calculate_throughput_mbps(1024 * 1024, Duration::from_secs(1));
        assert!((throughput - 1.0).abs() < 0.01); // ~1 MB/s

        let throughput = calculate_throughput_mbps(0, Duration::from_secs(1));
        assert_eq!(throughput, 0.0);

        let throughput = calculate_throughput_mbps(1024, Duration::from_secs(0));
        assert_eq!(throughput, 0.0);
    }

    #[test]
    fn test_calculate_compression_ratio() {
        let ratio = calculate_compression_ratio(100, 50);
        assert_eq!(ratio, 50.0);

        let ratio = calculate_compression_ratio(100, 100);
        assert_eq!(ratio, 100.0);

        let ratio = calculate_compression_ratio(0, 50);
        assert_eq!(ratio, 0.0);
    }

    #[test]
    fn test_validate_input_nonexistent() {
        let result = validate_input(Path::new("/nonexistent/file.txt"));
        assert!(matches!(result, Err(CliError::InvalidInput(_))));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_write_with_cleanup() {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path();

        // Successful write
        assert!(write_with_cleanup(path, b"test data").is_ok());

        // Verify data was written
        let content = fs::read(path).unwrap();
        assert_eq!(content, b"test data");
    }

    // -----------------------------------------------------------------------
    // cleanup-on-cancel, output validation, remaining input-validation branches
    // -----------------------------------------------------------------------

    fn token(cancelled: bool) -> Arc<dyn CancellationToken> {
        let t = Arc::new(AtomicCancellationToken::new());
        if cancelled {
            t.cancel();
        }
        t
    }

    #[test]
    fn cleanup_variant_leaves_the_output_alone_when_not_cancelled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("out.crush");
        fs::write(&output, b"partial").expect("seed");

        assert!(check_cancelled_with_cleanup(&token(false), &output).is_ok());
        assert!(output.exists(), "output must survive when not cancelled");
    }

    #[test]
    fn cleanup_variant_removes_the_partial_output_when_cancelled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("out.crush");
        fs::write(&output, b"partial").expect("seed");

        let result = check_cancelled_with_cleanup(&token(true), &output);

        assert!(matches!(result, Err(CliError::Interrupted)));
        assert!(
            !output.exists(),
            "a cancelled run must not leave a truncated archive behind"
        );
    }

    #[test]
    fn cleanup_variant_tolerates_a_missing_output_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("never-created.crush");

        // Cancellation can land before the file is created; the remove failure
        // is deliberately swallowed so the caller still sees Interrupted.
        let result = check_cancelled_with_cleanup(&token(true), &missing);
        assert!(matches!(result, Err(CliError::Interrupted)));
    }

    #[test]
    fn validate_input_accepts_a_normal_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("in.txt");
        fs::write(&path, b"content").expect("write");
        assert!(validate_input(&path).is_ok());
    }

    #[test]
    fn validate_input_rejects_a_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let result = validate_input(dir.path());
        assert!(matches!(result, Err(CliError::InvalidInput(_))));
    }

    #[test]
    fn validate_input_rejects_an_empty_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("empty.txt");
        fs::write(&path, b"").expect("write");

        let result = validate_input(&path);

        match result {
            Err(CliError::InvalidInput(msg)) => assert!(
                msg.contains("empty"),
                "message should name the problem, got: {msg}"
            ),
            other => panic!("expected InvalidInput about emptiness, got {other:?}"),
        }
    }

    #[test]
    fn validate_output_accepts_a_new_path_in_an_existing_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(validate_output(&dir.path().join("new.crush"), false).is_ok());
    }

    #[test]
    fn validate_output_rejects_an_existing_file_without_force() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("exists.crush");
        fs::write(&path, b"old").expect("write");

        match validate_output(&path, false) {
            Err(CliError::InvalidInput(msg)) => {
                assert!(msg.contains("--force"), "should suggest --force: {msg}");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn validate_output_allows_overwrite_with_force() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("exists.crush");
        fs::write(&path, b"old").expect("write");

        assert!(validate_output(&path, true).is_ok());
    }

    #[test]
    fn validate_output_rejects_a_missing_parent_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("no-such-dir").join("out.crush");

        match validate_output(&path, false) {
            Err(CliError::InvalidInput(msg)) => {
                assert!(
                    msg.contains("directory"),
                    "should name the directory: {msg}"
                );
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn validate_output_accepts_a_bare_relative_filename() {
        // `Path::parent()` yields "" here, which is the current directory and
        // must not be mistaken for a missing parent.
        assert!(validate_output(Path::new("relative-output.crush"), false).is_ok());
    }

    #[test]
    fn write_with_cleanup_removes_nothing_it_did_not_create() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Parent does not exist, so `fs::write` fails and the cleanup path runs.
        let path = dir.path().join("missing-dir").join("out.bin");

        let result = write_with_cleanup(&path, b"data");

        assert!(
            result.is_err(),
            "write into a missing directory should fail"
        );
        assert!(!path.exists());
    }

    #[test]
    fn write_to_stdout_succeeds() {
        // Captured by the test harness; this exercises the write+flush pair.
        assert!(write_to_stdout(b"utils stdout probe\n").is_ok());
        assert!(write_to_stdout(b"").is_ok());
    }
}
