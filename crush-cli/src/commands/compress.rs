use crate::cli::CompressArgs;
use crate::error::{CliError, Result};
use crate::output::{self, CompressionResult};
use crush_core::{compress_with_options, CompressionOptions};
use crush_core::plugin::FileMetadata;
use filetime::FileTime;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use indicatif::{ProgressBar, ProgressStyle};
use is_terminal::IsTerminal;

pub fn run(args: &CompressArgs) -> Result<()> {
    // Process each input file
    for input_path in &args.input {
        compress_file(input_path, args)?;
    }
    Ok(())
}

fn compress_file(input_path: &Path, args: &CompressArgs) -> Result<()> {
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

    // Create progress indicator for larger files
    let show_progress = std::io::stderr().is_terminal();
    let spinner = if show_progress && input_size > 1024 * 1024 {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} Compressing {msg}...")
                .expect("Invalid spinner template")
        );
        pb.set_message(input_path.display().to_string());
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        None
    };

    // Prepare compression options with metadata
    let mut file_meta = FileMetadata {
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
        options = options.with_plugin(plugin);
    }

    if let Some(timeout_secs) = args.timeout {
        options = options.with_timeout(Duration::from_secs(timeout_secs));
    }

    // Read input file
    let input_data = fs::read(input_path)?;

    // Start timing
    let start = Instant::now();

    // Compress
    let compressed_data = compress_with_options(&input_data, &options)?;

    // Stop timing
    let duration = start.elapsed();

    // Clear spinner
    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    // Write output file
    fs::write(&output_path, &compressed_data)?;

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

    // Create and display result
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

    // NOTE: Original files are kept by default (safe behavior)
    // The --keep flag is retained for compatibility but is now the default behavior
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
        CliError::InvalidInput(format!(
            "Cannot read input file {}: {}",
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
