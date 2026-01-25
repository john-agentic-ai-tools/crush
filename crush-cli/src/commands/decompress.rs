use crate::cli::DecompressArgs;
use crate::error::{CliError, Result};
use crate::output;
use crush_core::decompress;
use filetime::{set_file_mtime, FileTime};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub fn run(args: &DecompressArgs) -> Result<()> {
    // Process each input file
    for input_path in &args.input {
        decompress_file(input_path, args)?;
    }
    Ok(())
}

fn decompress_file(input_path: &Path, args: &DecompressArgs) -> Result<()> {
    // Validate input file
    validate_input(input_path)?;

    // Determine output path
    let output_path = determine_output_path(input_path, &args.output)?;

    // Validate output path
    validate_output(&output_path, args.force)?;

    // Read compressed file
    let compressed_data = fs::read(input_path)?;

    // Decompress
    let result = decompress(&compressed_data)?;
    let decompressed_data = result.data;
    let metadata = result.metadata;

    // Write output
    if args.stdout {
        // Write to stdout
        use std::io::Write;
        std::io::stdout().write_all(&decompressed_data)?;
    } else {
        // Write to file
        fs::write(&output_path, &decompressed_data)?;

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

        // Output success message
        output::format_success(
            &format!("Decompressed {} → {}", input_path.display(), output_path.display()),
            true
        );

        // Cleanup: remove compressed file if --keep is not specified
        if !args.keep {
            fs::remove_file(input_path)?;
        }
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
        if !parent.exists() {
            return Err(CliError::InvalidInput(format!(
                "Output directory does not exist: {}",
                parent.display()
            )));
        }
    }

    Ok(())
}
