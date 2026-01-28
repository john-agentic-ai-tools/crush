use crate::cli::CompressArgs;
use crate::error::{CliError, Result};
use crate::output::{self, CompressionResult};
use crush_core::plugin::FileMetadata;
use crush_core::{compress_with_options, CompressionOptions};
use filetime::FileTime;
use indicatif::{ProgressBar, ProgressStyle};
use is_terminal::IsTerminal;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, instrument, trace};

pub fn run(args: &CompressArgs, interrupted: Arc<AtomicBool>) -> Result<()> {
    // Check if reading from stdin (no input files provided)
    if args.input.is_empty() {
        compress_stdin(args, interrupted)?;
    } else {
        // Process each input file
        for input_path in &args.input {
            compress_file(input_path, args, interrupted.clone())?;
        }
    }
    Ok(())
}

/// Compress data from stdin
#[instrument(skip(args, interrupted))]
fn compress_stdin(args: &CompressArgs, interrupted: Arc<AtomicBool>) -> Result<()> {
    info!("Compressing from stdin");

    // Check for interrupt before starting
    if interrupted.load(Ordering::SeqCst) {
        return Err(CliError::Interrupted);
    }

    // Validate output path if not writing to stdout
    if !args.stdout && args.output.is_none() {
        return Err(CliError::InvalidInput(
            "When reading from stdin, either --output or --stdout must be specified".to_string(),
        ));
    }

    // Read all data from stdin
    trace!("Reading from stdin");
    let mut input_data = Vec::new();
    io::stdin().read_to_end(&mut input_data)?;
    let input_size = input_data.len() as u64;
    debug!("Read {} bytes from stdin", input_size);

    // Check for interrupt after reading
    if interrupted.load(Ordering::SeqCst) {
        return Err(CliError::Interrupted);
    }

    // Prepare compression options (no file metadata for stdin)
    let mut options = CompressionOptions::default().with_weights(args.level.to_weights());

    if let Some(ref plugin) = args.plugin {
        debug!("Using manually selected plugin: {}", plugin);
        options = options.with_plugin(plugin);
    } else {
        debug!(
            "Using automatic plugin selection with level: {:?}",
            args.level
        );
    }

    if let Some(timeout_secs) = args.timeout {
        debug!("Setting compression timeout: {} seconds", timeout_secs);
        options = options.with_timeout(Duration::from_secs(timeout_secs));
    }

    // Start timing
    let start = Instant::now();

    // Compress
    trace!("Starting compression operation");
    let compressed_data = compress_with_options(&input_data, &options)?;

    // Stop timing
    let duration = start.elapsed();
    debug!(
        "Compression completed in {:.3}s, output size: {} bytes",
        duration.as_secs_f64(),
        compressed_data.len()
    );

    // Check for interrupt before writing
    if interrupted.load(Ordering::SeqCst) {
        return Err(CliError::Interrupted);
    }

    // Write output
    if args.stdout {
        // Write to stdout
        trace!("Writing compressed data to stdout");
        io::stdout().write_all(&compressed_data)?;
        io::stdout().flush()?;
    } else if let Some(ref output_path) = args.output {
        // Write to file
        trace!("Writing compressed data to {}", output_path.display());
        validate_output(output_path, args.force)?;

        if let Err(e) = fs::write(output_path, &compressed_data) {
            // If write failed, ensure no partial file remains
            let _ = fs::remove_file(output_path);
            return Err(e.into());
        }

        // Check for interrupt after writing (cleanup partial file if interrupted)
        if interrupted.load(Ordering::SeqCst) {
            let _ = fs::remove_file(output_path);
            return Err(CliError::Interrupted);
        }
    }

    // Calculate statistics
    let output_size = compressed_data.len() as u64;
    let compression_ratio = if input_size > 0 {
        (output_size as f64 / input_size as f64) * 100.0
    } else {
        0.0
    };

    let throughput_mbps = if duration.as_secs_f64() > 0.0 {
        (input_size as f64 / (1024.0 * 1024.0)) / duration.as_secs_f64()
    } else {
        0.0
    };

    let plugin_used = args.plugin.clone().unwrap_or_else(|| "auto".to_string());

    // Log performance metrics (but don't print to stdout/stderr if using stdout mode)
    debug!(
        input_size,
        output_size,
        compression_ratio,
        throughput_mbps,
        plugin = %plugin_used,
        "Stdin compression: throughput {:.2} MB/s, ratio {:.1}%",
        throughput_mbps,
        compression_ratio
    );

    info!(
        output_size,
        compression_ratio,
        throughput_mbps,
        duration_secs = duration.as_secs_f64(),
        plugin = %plugin_used,
        "Compressed stdin: {} bytes -> {} bytes ({:.1}% reduction) in {:.3}s at {:.2} MB/s",
        input_size,
        output_size,
        100.0 - compression_ratio,
        duration.as_secs_f64(),
        throughput_mbps
    );

    Ok(())
}

#[instrument(skip(args, interrupted), fields(file = %input_path.display()))]
fn compress_file(
    input_path: &Path,
    args: &CompressArgs,
    interrupted: Arc<AtomicBool>,
) -> Result<()> {
    info!("Starting compression of {}", input_path.display());
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

    // Get file metadata for mtime and size
    let file_metadata = fs::metadata(input_path)?;
    let mtime = FileTime::from_last_modification_time(&file_metadata);
    let input_size = file_metadata.len();

    // Create progress indicator for larger files (but not when writing to stdout)
    let show_progress = std::io::stderr().is_terminal() && !args.stdout;
    let spinner = if show_progress && input_size > 1024 * 1024 {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} Compressing {msg}...")
                .expect("Invalid spinner template"),
        );
        pb.set_message(input_path.display().to_string());
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        None
    };

    // Prepare compression options with metadata
    let file_meta = FileMetadata {
        mtime: Some(mtime.unix_seconds()),
        #[cfg(unix)]
        permissions: {
            use std::os::unix::fs::PermissionsExt;
            Some(file_metadata.permissions().mode())
        },
    };

    let mut options = CompressionOptions::default()
        .with_weights(args.level.to_weights())
        .with_file_metadata(file_meta);

    if let Some(ref plugin) = args.plugin {
        debug!("Using manually selected plugin: {}", plugin);
        options = options.with_plugin(plugin);
    } else {
        debug!(
            "Using automatic plugin selection with level: {:?}",
            args.level
        );
    }

    if let Some(timeout_secs) = args.timeout {
        debug!("Setting compression timeout: {} seconds", timeout_secs);
        options = options.with_timeout(Duration::from_secs(timeout_secs));
    }

    // Read input file
    trace!("Reading input file: {}", input_path.display());
    let input_data = fs::read(input_path)?;
    debug!("Read {} bytes from input file", input_data.len());

    // Check for interrupt after reading
    if interrupted.load(Ordering::SeqCst) {
        return Err(CliError::Interrupted);
    }

    // Start timing
    let start = Instant::now();

    // Compress
    trace!("Starting compression operation");
    let compressed_data = compress_with_options(&input_data, &options)?;

    // Stop timing
    let duration = start.elapsed();
    debug!(
        "Compression completed in {:.3}s, output size: {} bytes",
        duration.as_secs_f64(),
        compressed_data.len()
    );

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
        trace!("Writing compressed data to stdout");
        io::stdout().write_all(&compressed_data)?;
        io::stdout().flush()?;
    } else {
        // Write output file (T085: cleanup on failure/interrupt)
        if let Err(e) = fs::write(&output_path, &compressed_data) {
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
    }

    // Calculate statistics
    let output_size = compressed_data.len() as u64;
    let compression_ratio = if input_size > 0 {
        (output_size as f64 / input_size as f64) * 100.0
    } else {
        0.0
    };

    let throughput_mbps = if duration.as_secs_f64() > 0.0 {
        (input_size as f64 / (1024.0 * 1024.0)) / duration.as_secs_f64()
    } else {
        0.0
    };

    // Get plugin name from options (default to "auto" if not specified)
    let plugin_used = args.plugin.clone().unwrap_or_else(|| "auto".to_string());

    // Log performance metrics with structured fields
    debug!(
        input_size,
        output_size,
        compression_ratio,
        throughput_mbps,
        plugin = %plugin_used,
        "Performance metrics - throughput: {:.2} MB/s, compression ratio: {:.1}%, plugin: {}",
        throughput_mbps,
        compression_ratio,
        plugin_used
    );

    let size_reduction = 100.0 - compression_ratio;
    info!(
        input_path = %input_path.display(),
        output_path = %output_path.display(),
        input_size,
        output_size,
        compression_ratio,
        throughput_mbps,
        duration_secs = duration.as_secs_f64(),
        plugin = %plugin_used,
        "Compressed {} -> {} ({:.1}% {}) in {:.3}s at {:.2} MB/s",
        input_path.display(),
        output_path.display(),
        size_reduction.abs(),
        if size_reduction > 0.0 { "smaller" } else { "larger" },
        duration.as_secs_f64(),
        throughput_mbps
    );

    // Create and display result (but not when writing to stdout)
    if !args.stdout {
        let result = CompressionResult {
            input_path: input_path.to_path_buf(),
            output_path: output_path.clone(),
            input_size,
            output_size,
            compression_ratio,
            duration,
            throughput_mbps,
            plugin_used,
        };

        output::format_compression_result(&result, show_progress);
    }

    // NOTE: Original files are kept by default (safe behavior)
    // To delete originals after compression, users should manually delete them
    // TODO: Consider adding a --remove or --delete flag in the future if needed

    Ok(())
}

/// Validate that input file exists and is readable
fn validate_input(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(CliError::InvalidInput(format!(
            "Input file not found: {}",
            path.display()
        )));
    }

    if !path.is_file() {
        return Err(CliError::InvalidInput(format!(
            "Input path is not a file: {}",
            path.display()
        )));
    }

    // Check if file is readable by attempting to open it
    File::open(path).map_err(|e| {
        CliError::InvalidInput(format!("Cannot read input file {}: {}", path.display(), e))
    })?;

    Ok(())
}

/// Determine the output file path
fn determine_output_path(input: &Path, output_arg: &Option<PathBuf>) -> Result<PathBuf> {
    if let Some(output) = output_arg {
        // User specified output path
        if output.is_dir() {
            // Output is a directory - use input filename with .crush extension
            let filename = input
                .file_name()
                .ok_or_else(|| CliError::InvalidInput("Invalid input filename".to_string()))?;
            Ok(output.join(filename).with_extension("crush"))
        } else {
            // Output is a file path
            Ok(output.clone())
        }
    } else {
        // Default: add .crush extension to input filename
        let mut output = input.to_path_buf();
        let current_ext = output.extension().and_then(|s| s.to_str()).unwrap_or("");
        let new_ext = if current_ext.is_empty() {
            "crush".to_string()
        } else {
            format!("{}.crush", current_ext)
        };
        output.set_extension(new_ext);
        Ok(output)
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
