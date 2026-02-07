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
}
