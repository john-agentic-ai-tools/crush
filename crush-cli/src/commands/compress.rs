use crate::cli::CompressArgs;
use crate::error::{CliError, Result};
use crate::output;
use crush_core::{compress_with_options, CompressionOptions};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::Duration;

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

    // Prepare compression options
    let mut options = CompressionOptions::default()
        .with_weights(args.level.to_weights());

    if let Some(ref plugin) = args.plugin {
        options = options.with_plugin(plugin);
    }

    if let Some(timeout_secs) = args.timeout {
        options = options.with_timeout(Duration::from_secs(timeout_secs));
    }

    // Read input file
    let input_data = fs::read(input_path)?;

    // Compress
    let compressed_data = compress_with_options(&input_data, &options)?;

    // Write output file
    fs::write(&output_path, &compressed_data)?;

    // Output success message
    output::format_success(
        &format!("Compressed {} → {}", input_path.display(), output_path.display()),
        true
    );

    // Cleanup: remove input file if --keep is not specified
    if !args.keep {
        fs::remove_file(input_path)?;
    }

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
        if !parent.exists() {
            return Err(CliError::InvalidInput(format!(
                "Output directory does not exist: {}",
                parent.display()
            )));
        }
    }

    Ok(())
}
