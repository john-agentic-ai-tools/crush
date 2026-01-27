use crate::cli::DecompressArgs;
use crate::error::{CliError, Result};
use crate::output::{self, DecompressionResult};
use crush_core::decompress;
use filetime::{set_file_mtime, FileTime};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use indicatif::{ProgressBar, ProgressStyle};
use is_terminal::IsTerminal;

pub fn run(args: &DecompressArgs, interrupted: Arc<AtomicBool>) -> Result<()> {
    // Process each input file
    for input_path in &args.input {
        decompress_file(input_path, args, interrupted.clone())?;
    }
    Ok(())
}

fn decompress_file(input_path: &Path, args: &DecompressArgs, interrupted: Arc<AtomicBool>) -> Result<()> {
    // Check for interrupt before starting
    if interrupted.load(Ordering::SeqCst) {
        return Err(CliError::Interrupted);
    }

    // Validate input file
    validate_input(input_path)?;

    // Determine output path
    let output_path = determine_output_path(input_path, &args.output)?;

    // Validate output path
    validate_output(&output_path, args.force)?;

    // Get file size for statistics
    let input_size = fs::metadata(input_path)?.len();

    // Create progress indicator for larger files
    let show_progress = std::io::stderr().is_terminal() && !args.stdout;
    let spinner = if show_progress && input_size > 1024 * 1024 {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} Decompressing {msg}...")
                .expect("Invalid spinner template")
        );
        pb.set_message(input_path.display().to_string());
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        None
    };

    // Read compressed file
    let compressed_data = fs::read(input_path)?;

    // Check for interrupt after reading
    if interrupted.load(Ordering::SeqCst) {
        return Err(CliError::Interrupted);
    }

    // Start timing
    let start = Instant::now();

    // Decompress
    let result = decompress(&compressed_data)?;
    let decompressed_data = result.data;
    let metadata = result.metadata;

    // Stop timing
    let duration = start.elapsed();

    // Clear spinner
    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    // Check for interrupt before writing
    if interrupted.load(Ordering::SeqCst) {
        return Err(CliError::Interrupted);
    }

    // Write output
    if args.stdout {
        // Write to stdout
        use std::io::Write;
        std::io::stdout().write_all(&decompressed_data)?;
    } else {
        // Write to file (with cleanup on failure/interrupt)
        if let Err(e) = fs::write(&output_path, &decompressed_data) {
            // If write failed, ensure no partial file remains
            let _ = fs::remove_file(&output_path);
            return Err(e.into());
        }

        // Check for interrupt after writing (cleanup partial file if interrupted)
        if interrupted.load(Ordering::SeqCst) {
            // Remove the output file we just wrote
            let _ = fs::remove_file(&output_path);
            return Err(CliError::Interrupted);
        }

        // Restore mtime if available
        if let Some(mtime_secs) = metadata.mtime {
            let mtime = FileTime::from_unix_time(mtime_secs, 0);
            if let Err(e) = set_file_mtime(&output_path, mtime) {
                // Log a warning, but don't fail the operation
                output::format_warning(&format!(
                    "Could not set modification time for {}: {}",
                    output_path.display(),
                    e
                ), true);
            }
        }

        // Restore Unix permissions if available (T056)
        #[cfg(unix)]
        if let Some(permissions_mode) = metadata.permissions {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(permissions_mode);
            if let Err(e) = std::fs::set_permissions(&output_path, permissions) {
                // Log a warning, but don't fail the operation
                output::format_warning(&format!(
                    "Could not restore Unix permissions for {}: {}",
                    output_path.display(),
                    e
                ), true);
            }
        }

        // Calculate statistics
        let output_size = decompressed_data.len() as u64;
        let throughput_mbps = if duration.as_secs_f64() > 0.0 {
            (output_size as f64 / (1024.0 * 1024.0)) / duration.as_secs_f64()
        } else {
            0.0
        };

        // Create and display result
        let decomp_result = DecompressionResult {
            input_path: input_path.to_path_buf(),
            output_path: output_path.clone(),
            input_size,
            output_size,
            duration,
            throughput_mbps,
            crc_valid: true, // If decompression succeeded, CRC is valid
        };

        output::format_decompression_result(&decomp_result, show_progress);

        // NOTE: Compressed files are kept by default (safe behavior)
        // The --keep flag is retained for compatibility but is now the default behavior
        // To delete compressed files after decompression, users should manually delete them
        // TODO: Consider adding a --remove or --delete flag in the future if needed
    }

    Ok(())
}

/// Validate that input file exists and is readable
fn validate_input(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(CliError::InvalidInput(format!(
            "Compressed file not found: {}",
            path.display()
        )));
    }

    if !path.is_file() {
        return Err(CliError::InvalidInput(format!(
            "Input path is not a file: {}",
            path.display()
        )));
    }

    // Check if file is readable
    File::open(path).map_err(|e| {
        CliError::InvalidInput(format!(
            "Cannot read compressed file {}: {}",
            path.display(),
            e
        ))
    })?;

    Ok(())
}

/// Determine the output file path
fn determine_output_path(input: &Path, output_arg: &Option<PathBuf>) -> Result<PathBuf> {
    if let Some(output) = output_arg {
        // User specified output path
        if output.is_dir() {
            // Output is a directory - use input filename without .crush extension
            let filename = strip_crush_extension(input)?;
            Ok(output.join(filename))
        } else {
            // Output is a file path
            Ok(output.clone())
        }
    } else {
        // Default: strip .crush extension from input filename
        let output_filename = strip_crush_extension(input)?;
        if let Some(parent) = input.parent() {
            Ok(parent.join(output_filename))
        } else {
            Ok(PathBuf::from(output_filename))
        }
    }
}

/// Strip .crush extension from filename
fn strip_crush_extension(path: &Path) -> Result<PathBuf> {
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| CliError::InvalidInput("Invalid filename".to_string()))?;

    // Check if filename ends with .crush
    if filename.ends_with(".crush") {
        let base_name = &filename[..filename.len() - 6]; // Remove ".crush"
        Ok(PathBuf::from(base_name))
    } else {
        // If it doesn't end with .crush, just remove the last extension
        path.file_stem()
            .map(PathBuf::from)
            .ok_or_else(|| CliError::InvalidInput(format!(
                "Cannot determine output filename for {}",
                path.display()
            )))
    }
}

/// Validate output path and check for conflicts
fn validate_output(path: &Path, force: bool) -> Result<()> {
    // Check if output file already exists
    if path.exists() && !force {
        return Err(CliError::InvalidInput(format!(
            "Output file already exists: {} (use --force to overwrite)",
            path.display()
        )));
    }

    // Check that parent directory exists
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
