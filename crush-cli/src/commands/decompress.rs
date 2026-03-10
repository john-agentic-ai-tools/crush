use crate::cli::DecompressArgs;
use crate::commands::utils;
use crate::error::{CliError, Result};
use crate::output::{self, DecompressionResult};
use crush_core::cancel::CancellationToken;
use crush_core::decompress_with_cancel;
use crush_core::plugin::FileMetadata;
use filetime::{set_file_mtime, FileTime};
use indicatif::{ProgressBar, ProgressStyle};
use is_terminal::IsTerminal;
use std::fs;
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, instrument, trace};

/// Decompress a single block using random access.
///
/// Returns the decompressed block data and empty metadata (random access doesn't preserve metadata).
fn decompress_single_block(
    compressed_data: &[u8],
    block_n: u64,
) -> Result<(Vec<u8>, FileMetadata)> {
    // Detect if this is a parallel-deflate CRSH file
    if compressed_data.len() >= 4 && &compressed_data[0..4] == b"CRSH" {
        // Use crush_parallel for random access
        let mut cursor = Cursor::new(compressed_data);
        let index = crush_parallel::load_index(&mut cursor)?;

        // Check if block exists
        if block_n >= index.len() {
            return Err(CliError::InvalidInput(format!(
                "Block {} does not exist (file has {} blocks)",
                block_n,
                index.len()
            )));
        }

        let config = crush_parallel::EngineConfiguration::default();
        let block_data = crush_parallel::decompress_block(&mut cursor, &index, block_n, &config)?;

        debug!(
            "Decompressed block {} ({} bytes)",
            block_n,
            block_data.len()
        );
        Ok((block_data, FileMetadata::default()))
    } else {
        Err(CliError::InvalidInput(
            "Random access (--block) is only supported for parallel-deflate (.crsh) files"
                .to_string(),
        ))
    }
}

pub fn run(
    args: &DecompressArgs,
    interrupted: Arc<dyn CancellationToken>,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    // Check if reading from stdin (no input files and stdout mode)
    if args.input.is_empty() {
        if args.stdout {
            decompress_stdin(args, interrupted, cancel_flag)?;
        } else {
            return Err(CliError::InvalidInput(
                "No input files specified. Use --stdout with stdin, or provide file paths."
                    .to_string(),
            ));
        }
    } else {
        // Process each input file
        for input_path in &args.input {
            decompress_file(input_path, args, interrupted.clone(), cancel_flag.clone())?;
        }
    }
    Ok(())
}

/// Decompress data from stdin
#[instrument(skip(_args, interrupted, cancel_flag))]
fn decompress_stdin(
    _args: &DecompressArgs,
    interrupted: Arc<dyn CancellationToken>,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
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

    // Detect plugin from header magic bytes for logging
    let detected_plugin = if compressed_data.len() >= 4 {
        let magic = [
            compressed_data[0],
            compressed_data[1],
            compressed_data[2],
            compressed_data[3],
        ];
        crush_core::list_plugins()
            .iter()
            .find(|p| p.magic_number == magic)
            .map(|p| p.name.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        "unknown".to_string()
    };
    info!(
        plugin = %detected_plugin,
        input_size,
        "Decompressing stdin with plugin '{}' ({} bytes)",
        detected_plugin,
        input_size
    );

    // Start timing
    let start = Instant::now();

    // Decompress (with cancel flag connected to Ctrl+C)
    trace!("Starting decompression operation");
    let result = decompress_with_cancel(&compressed_data, cancel_flag)?;
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

#[instrument(skip(args, interrupted, cancel_flag), fields(file = %input_path.display()))]
fn decompress_file(
    input_path: &Path,
    args: &DecompressArgs,
    interrupted: Arc<dyn CancellationToken>,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
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

    // Detect plugin from header magic bytes for logging
    let detected_plugin = if compressed_data.len() >= 4 {
        let magic = [
            compressed_data[0],
            compressed_data[1],
            compressed_data[2],
            compressed_data[3],
        ];
        crush_core::list_plugins()
            .iter()
            .find(|p| p.magic_number == magic)
            .map(|p| p.name.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        "unknown".to_string()
    };
    info!(
        plugin = %detected_plugin,
        input_size,
        "Decompressing with plugin '{}' ({} bytes)",
        detected_plugin,
        input_size
    );

    // Log when --force-cpu is active for GPU-compressed files
    if args.force_cpu && detected_plugin == "gpu-deflate" {
        info!("--force-cpu active: using CPU fallback for GPU-compressed file");
    }

    // Decompress (either full file or single block)
    let (decompressed_data, metadata) = if let Some(block_n) = args.block {
        // Random access: decompress only the specified block
        trace!("Starting random access decompression for block {}", block_n);
        decompress_single_block(&compressed_data, block_n)?
    } else {
        // Full decompression (with cancel flag connected to Ctrl+C)
        trace!("Starting full decompression operation");
        let result = decompress_with_cancel(&compressed_data, cancel_flag)?;
        (result.data, result.metadata)
    };

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // -----------------------------------------------------------------------
    // strip_crush_extension
    // -----------------------------------------------------------------------

    #[test]
    fn test_strip_crush_extension_dot_crush() {
        let result = strip_crush_extension(Path::new("data.txt.crush")).expect("ok");
        assert_eq!(result, PathBuf::from("data.txt"));
    }

    #[test]
    fn test_strip_crush_extension_no_crush() {
        // Falls back to stripping last extension
        let result = strip_crush_extension(Path::new("data.txt.gz")).expect("ok");
        assert_eq!(result, PathBuf::from("data.txt"));
    }

    #[test]
    fn test_strip_crush_extension_just_crush() {
        let result = strip_crush_extension(Path::new("archive.crush")).expect("ok");
        assert_eq!(result, PathBuf::from("archive"));
    }

    #[test]
    fn test_strip_crush_extension_no_extension() {
        let result = strip_crush_extension(Path::new("data")).expect("ok");
        assert_eq!(result, PathBuf::from("data"));
    }

    // -----------------------------------------------------------------------
    // determine_output_path
    // -----------------------------------------------------------------------

    #[test]
    fn test_determine_output_path_default_strips_crush() {
        let result = determine_output_path(Path::new("data.txt.crush"), &None).expect("ok");
        assert_eq!(result, PathBuf::from("data.txt"));
    }

    #[test]
    fn test_determine_output_path_with_parent() {
        let result = determine_output_path(Path::new("/tmp/archive.crush"), &None).expect("ok");
        assert_eq!(result, PathBuf::from("/tmp/archive"));
    }

    #[test]
    fn test_determine_output_path_explicit_file() {
        let output = Some(PathBuf::from("restored.txt"));
        let result = determine_output_path(Path::new("data.txt.crush"), &output).expect("ok");
        assert_eq!(result, PathBuf::from("restored.txt"));
    }

    #[test]
    fn test_determine_output_path_to_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = Some(dir.path().to_path_buf());
        let result = determine_output_path(Path::new("data.txt.crush"), &output).expect("ok");
        assert_eq!(result, dir.path().join("data.txt"));
    }

    #[test]
    fn test_determine_output_path_non_crush_ext() {
        // If the input doesn't have .crush, it strips the last extension
        let result = determine_output_path(Path::new("data.bin"), &None).expect("ok");
        assert_eq!(result, PathBuf::from("data"));
    }

    // -----------------------------------------------------------------------
    // decompress_single_block
    // -----------------------------------------------------------------------

    #[test]
    fn test_decompress_single_block_non_crsh_format() {
        let data = b"NOT_CRSH_data";
        let result = decompress_single_block(data, 0);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Random access"));
    }

    #[test]
    fn test_decompress_single_block_too_short() {
        let data = b"CR"; // too short for magic detection
        let result = decompress_single_block(data, 0);
        assert!(result.is_err());
    }
}
