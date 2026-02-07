use crate::cli::DecompressArgs;
use crate::commands::utils;
use crate::error::{CliError, Result};
use crate::output::{self, DecompressionResult};
use crush_core::cancel::CancellationToken;
use crush_core::decompress;
use filetime::{set_file_mtime, FileTime};
use indicatif::{ProgressBar, ProgressStyle};
use is_terminal::IsTerminal;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, instrument, trace};

pub fn run(args: &DecompressArgs, interrupted: Arc<dyn CancellationToken>) -> Result<()> {
    // Check if reading from stdin (no input files and stdout mode)
    if args.input.is_empty() {
        if args.stdout {
            decompress_stdin(args, interrupted)?;
        } else {
            return Err(CliError::InvalidInput(
                "No input files specified. Use --stdout with stdin, or provide file paths."
                    .to_string(),
            ));
        }
    } else {
        // Process each input file
        for input_path in &args.input {
            decompress_file(input_path, args, interrupted.clone())?;
        }
    }
    Ok(())
}

/// Decompress data from stdin
#[instrument(skip(_args, interrupted))]
fn decompress_stdin(_args: &DecompressArgs, interrupted: Arc<dyn CancellationToken>) -> Result<()> {
    info!("Decompressing from stdin");

    // Check for interrupt before starting
    utils::check_cancelled(&interrupted)?;

    // Read all data from stdin
    trace!("Reading from stdin");
    let mut compressed_data = Vec::new();
    io::stdin().read_to_end(&mut compressed_data)?;
    let input_size = compressed_data.len() as u64;
    debug!("Read {} bytes from stdin", input_size);

    // Check for interrupt after reading
    utils::check_cancelled(&interrupted)?;

    // Start timing
    let start = Instant::now();

    // Decompress
    trace!("Starting decompression operation");
    let result = decompress(&compressed_data)?;
    let decompressed_data = result.data;

    // Stop timing
    let duration = start.elapsed();
    debug!(
        "Decompression completed in {:.3}s, output size: {} bytes",
        duration.as_secs_f64(),
        decompressed_data.len()
    );

    // Check for interrupt before writing
    utils::check_cancelled(&interrupted)?;

    // Write to stdout
    trace!("Writing decompressed data to stdout");
    utils::write_to_stdout(&decompressed_data)?;

    // Calculate statistics
    let output_size = decompressed_data.len() as u64;
    let throughput_mbps = utils::calculate_throughput_mbps(output_size, duration);

    // Log performance metrics
    debug!(
        input_size,
        output_size, throughput_mbps, "Stdin decompression: throughput {:.2} MB/s", throughput_mbps
    );

    info!(
        input_size,
        output_size,
        throughput_mbps,
        duration_secs = duration.as_secs_f64(),
        "Decompressed stdin: {} bytes -> {} bytes in {:.3}s at {:.2} MB/s",
        input_size,
        output_size,
        duration.as_secs_f64(),
        throughput_mbps
    );

    Ok(())
}

#[instrument(skip(args, interrupted), fields(file = %input_path.display()))]
fn decompress_file(
    input_path: &Path,
    args: &DecompressArgs,
    interrupted: Arc<dyn CancellationToken>,
) -> Result<()> {
    info!("Starting decompression of {}", input_path.display());
    // Check for interrupt before starting
    utils::check_cancelled(&interrupted)?;

    // Validate input file
    utils::validate_input(input_path)?;

    // Determine and validate output path (only if not writing to stdout)
    let output_path = if !args.stdout {
        let path = determine_output_path(input_path, &args.output)?;
        utils::validate_output(&path, args.force)?;
        path
    } else {
        // Dummy path when writing to stdout (won't be used)
        PathBuf::new()
    };

    // Get file size for statistics
    let input_size = fs::metadata(input_path)?.len();

    // Show cancel hint for large files (>1MB)
    if !args.stdout {
        crate::feedback::show_cancel_hint(crate::feedback::should_show_hint(input_size));
    }

    // Create progress indicator for larger files
    let show_progress = std::io::stderr().is_terminal() && !args.stdout;
    let spinner = if show_progress && input_size > 1024 * 1024 {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} Decompressing {msg}...")
                .expect("Invalid spinner template"),
        );
        pb.set_message(input_path.display().to_string());
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        None
    };

    // Read compressed file
    trace!("Reading compressed file: {}", input_path.display());
    let compressed_data = fs::read(input_path)?;
    debug!("Read {} bytes from compressed file", compressed_data.len());

    // Check for interrupt after reading
    utils::check_cancelled(&interrupted)?;

    // Start timing
    let start = Instant::now();

    // Decompress
    trace!("Starting decompression operation");
    let result = decompress(&compressed_data)?;
    let decompressed_data = result.data;
    let metadata = result.metadata;

    // Stop timing
    let duration = start.elapsed();
    debug!(
        "Decompression completed in {:.3}s, output size: {} bytes",
        duration.as_secs_f64(),
        decompressed_data.len()
    );

    // Clear spinner
    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    // Check for interrupt before writing
    utils::check_cancelled(&interrupted)?;

    // Write output
    if args.stdout {
        // Write to stdout
        utils::write_to_stdout(&decompressed_data)?;
    } else {
        // Write to file (with cleanup on failure/interrupt)
        utils::write_with_cleanup(&output_path, &decompressed_data)?;

        // Check for interrupt after writing (cleanup partial file if interrupted)
        utils::check_cancelled_with_cleanup(&interrupted, &output_path)?;

        // Restore mtime if available
        if let Some(mtime_secs) = metadata.mtime {
            trace!("Restoring modification time: {}", mtime_secs);
            let mtime = FileTime::from_unix_time(mtime_secs, 0);
            if let Err(e) = set_file_mtime(&output_path, mtime) {
                // Log a warning, but don't fail the operation
                debug!("Could not set modification time: {}", e);
                output::format_warning(
                    &format!(
                        "Could not set modification time for {}: {}",
                        output_path.display(),
                        e
                    ),
                    true,
                );
            } else {
                debug!("Successfully restored modification time");
            }
        }

        // Restore Unix permissions if available (T056)
        #[cfg(unix)]
        if let Some(permissions_mode) = metadata.permissions {
            trace!("Restoring Unix permissions: {:o}", permissions_mode);
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(permissions_mode);
            if let Err(e) = std::fs::set_permissions(&output_path, permissions) {
                // Log a warning, but don't fail the operation
                debug!("Could not restore Unix permissions: {}", e);
                output::format_warning(
                    &format!(
                        "Could not restore Unix permissions for {}: {}",
                        output_path.display(),
                        e
                    ),
                    true,
                );
            } else {
                debug!(
                    "Successfully restored Unix permissions: {:o}",
                    permissions_mode
                );
            }
        }

        // Calculate statistics
        let output_size = decompressed_data.len() as u64;
        let throughput_mbps = utils::calculate_throughput_mbps(output_size, duration);

        // Log performance metrics with structured fields
        debug!(
            input_size,
            output_size,
            throughput_mbps,
            "Performance metrics - throughput: {:.2} MB/s",
            throughput_mbps
        );

        info!(
            input_path = %input_path.display(),
            output_path = %output_path.display(),
            input_size,
            output_size,
            throughput_mbps,
            duration_secs = duration.as_secs_f64(),
            crc_valid = true,
            "Decompressed {} -> {} in {:.3}s at {:.2} MB/s",
            input_path.display(),
            output_path.display(),
            duration.as_secs_f64(),
            throughput_mbps
        );

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
            Ok(output_filename)
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
    if let Some(base_name) = filename.strip_suffix(".crush") {
        Ok(PathBuf::from(base_name))
    } else {
        // If it doesn't end with .crush, just remove the last extension
        path.file_stem().map(PathBuf::from).ok_or_else(|| {
            CliError::InvalidInput(format!(
                "Cannot determine output filename for {}",
                path.display()
            ))
        })
    }
}
