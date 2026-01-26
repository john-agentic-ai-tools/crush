use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use std::io::Write;
use std::time::Duration;
use std::path::PathBuf;
use crush_core::InspectResult;
use indicatif::{ProgressBar, ProgressStyle};
use is_terminal::IsTerminal;

/// Format and print a success message
pub fn format_success(message: &str, use_colors: bool) {
    let mut stdout = if use_colors {
        StandardStream::stdout(ColorChoice::Auto)
    } else {
        StandardStream::stdout(ColorChoice::Never)
    };

    let mut color_spec = ColorSpec::new();
    color_spec.set_fg(Some(Color::Green));

    let _ = stdout.set_color(&color_spec);
    let _ = writeln!(&mut stdout, "{}", message);
    let _ = stdout.reset();
}

/// Format and print a warning message
pub fn format_warning(message: &str, use_colors: bool) {
    let mut stderr = if use_colors {
        StandardStream::stderr(ColorChoice::Always)
    } else {
        StandardStream::stderr(ColorChoice::Never)
    };

    let mut color_spec = ColorSpec::new();
    color_spec.set_fg(Some(Color::Yellow));

    let _ = stderr.set_color(&color_spec);
    let _ = writeln!(&mut stderr, "Warning: {}", message);
    let _ = stderr.reset();
}

pub fn format_inspect_result(path: &std::path::Path, result: &InspectResult, use_colors: bool) {
    let mut stdout = if use_colors {
        StandardStream::stdout(ColorChoice::Auto)
    } else {
        StandardStream::stdout(ColorChoice::Never)
    };

    let _ = writeln!(&mut stdout, "File: {}", path.display());

    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(&mut stdout, "  Original size: ");
    let _ = stdout.reset();
    let _ = writeln!(&mut stdout, "{}", result.original_size);

    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(&mut stdout, "  Compressed size: ");
    let _ = stdout.reset();
    let _ = writeln!(&mut stdout, "{}", result.compressed_size);

    let ratio = if result.original_size > 0 {
        (result.compressed_size as f64 / result.original_size as f64) * 100.0
    } else {
        0.0
    };
    let size_reduction = 100.0 - ratio;
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(&mut stdout, "  Size reduction: ");
    let _ = stdout.reset();
    if size_reduction > 0.0 {
        let _ = writeln!(&mut stdout, "{:.1}% (compressed to {:.1}% of original)", size_reduction, ratio);
    } else if size_reduction < 0.0 {
        let _ = writeln!(&mut stdout, "{:.1}% (expanded to {:.1}% of original)", size_reduction, ratio);
    } else {
        let _ = writeln!(&mut stdout, "0.0% (same size)");
    }

    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(&mut stdout, "  Plugin: ");
    let _ = stdout.reset();
    let _ = writeln!(&mut stdout, "{}", result.plugin_name);

    let crc_status_color = if result.crc_valid { Color::Green } else { Color::Red };
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(&mut stdout, "  CRC32: ");
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(crc_status_color)));
    let _ = writeln!(&mut stdout, "{}", if result.crc_valid { "VALID" } else { "INVALID" });
    let _ = stdout.reset();

    if let Some(mtime) = result.metadata.mtime {
        let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
        let _ = write!(&mut stdout, "  Modification time: ");
        let _ = stdout.reset();
        let _ = writeln!(&mut stdout, "{}", mtime);
    }
}

pub fn format_inspect_summary(results: &[(std::path::PathBuf, InspectResult)], use_colors: bool) {
    let mut stdout = if use_colors {
        StandardStream::stdout(ColorChoice::Auto)
    } else {
        StandardStream::stdout(ColorChoice::Never)
    };

    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)));
    let _ = writeln!(&mut stdout, "\n--- Summary ---");
    let _ = stdout.reset();

    let mut total_original_size = 0;
    let mut total_compressed_size = 0;
    let mut all_crc_valid = true;

    for (path, result) in results {
        total_original_size += result.original_size;
        total_compressed_size += result.compressed_size;
        if !result.crc_valid {
            all_crc_valid = false;
        }

        let _ = writeln!(
            &mut stdout,
            "  File: {} | Original: {} | Compressed: {} | Plugin: {} | CRC: {}",
            path.display(),
            result.original_size,
            result.compressed_size,
            result.plugin_name,
            if result.crc_valid { "VALID" } else { "INVALID" }
        );
    }

    let overall_ratio = if total_original_size > 0 {
        (total_compressed_size as f64 / total_original_size as f64) * 100.0
    } else {
        0.0
    };
    let overall_reduction = 100.0 - overall_ratio;

    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)));
    let _ = writeln!(&mut stdout, "-----------------");
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)));
    let _ = writeln!(&mut stdout, "  Total Files: {}", results.len());
    let _ = writeln!(&mut stdout, "  Total Original Size: {}", total_original_size);
    let _ = writeln!(&mut stdout, "  Total Compressed Size: {}", total_compressed_size);
    let _ = writeln!(&mut stdout, "  Overall Size Reduction: {:.1}%", overall_reduction);
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(if all_crc_valid { Color::Green } else { Color::Red })));
    let _ = writeln!(&mut stdout, "  All CRC Valid: {}", all_crc_valid);
    let _ = stdout.reset();
}

pub fn format_inspect_json(results: &[(std::path::PathBuf, InspectResult)], _use_colors: bool) {
    // When outputting JSON, we don't include the path in the serialized object itself,
    // as it's typically used in an array context where the path might be implied or handled by the caller.
    // We only serialize the InspectResult part.
    let serialized_results: Vec<&InspectResult> = results.iter().map(|(_, res)| res).collect();
    let serialized = serde_json::to_string_pretty(&serialized_results)
        .expect("Failed to serialize inspect results to JSON");
    println!("{}", serialized);
}

pub fn format_inspect_csv(results: &[(std::path::PathBuf, InspectResult)], _use_colors: bool) {
    // Print CSV header
    println!("file_path,original_size,compressed_size,compression_ratio,plugin,crc_valid");

    // Print each result as a CSV row
    for (path, result) in results {
        let ratio = if result.original_size > 0 {
            (result.compressed_size as f64 / result.original_size as f64) * 100.0
        } else {
            0.0
        };

        println!(
            "{},{},{},{:.1},{},{}",
            path.display(),
            result.original_size,
            result.compressed_size,
            ratio,
            result.plugin_name,
            result.crc_valid
        );
    }
}

/// Result of a compression operation
#[derive(Debug, Clone)]
pub struct CompressionResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub input_size: u64,
    pub output_size: u64,
    pub compression_ratio: f64,
    pub duration: Duration,
    pub throughput_mbps: f64,
    pub plugin_used: String,
}

/// Result of a decompression operation
#[derive(Debug, Clone)]
pub struct DecompressionResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub input_size: u64,
    pub output_size: u64,
    pub duration: Duration,
    pub throughput_mbps: f64,
    pub crc_valid: bool,
}

/// Create a progress bar for file operations
/// Returns None if:
/// - File size is less than 1MB (to avoid flicker)
/// - Not running in a TTY
/// - Progress bars are disabled
pub fn create_progress_bar(file_size: u64, show_progress: bool) -> Option<ProgressBar> {
    const MIN_SIZE_FOR_PROGRESS: u64 = 1024 * 1024; // 1MB

    if !show_progress || file_size < MIN_SIZE_FOR_PROGRESS || !std::io::stderr().is_terminal() {
        return None;
    }

    let pb = ProgressBar::new(file_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .expect("Invalid progress bar template")
            .progress_chars("#>-"),
    );

    // Update at 10Hz (every 100ms)
    pb.set_draw_target(indicatif::ProgressDrawTarget::stderr_with_hz(10));

    Some(pb)
}

/// Format and display compression results
pub fn format_compression_result(result: &CompressionResult, use_colors: bool) {
    let mut stdout = if use_colors {
        StandardStream::stdout(ColorChoice::Auto)
    } else {
        StandardStream::stdout(ColorChoice::Never)
    };

    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)));
    let _ = write!(&mut stdout, "Compressed ");
    let _ = stdout.reset();
    let _ = write!(&mut stdout, "{} -> {} ", result.input_path.display(), result.output_path.display());

    // Calculate actual size reduction (negative means file grew)
    let size_reduction = 100.0 - result.compression_ratio;
    let reduction_text = if size_reduction > 0.0 {
        format!("{:.1}% smaller", size_reduction)
    } else if size_reduction < 0.0 {
        format!("{:.1}% larger", -size_reduction)
    } else {
        "same size".to_string()
    };

    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(&mut stdout, "({}, {:.1} MB/s, {})",
        reduction_text,
        result.throughput_mbps,
        result.plugin_used
    );
    let _ = stdout.reset();
    let _ = writeln!(&mut stdout);
}

/// Format and display decompression results
pub fn format_decompression_result(result: &DecompressionResult, use_colors: bool) {
    let mut stdout = if use_colors {
        StandardStream::stdout(ColorChoice::Auto)
    } else {
        StandardStream::stdout(ColorChoice::Never)
    };

    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)));
    let _ = write!(&mut stdout, "Decompressed ");
    let _ = stdout.reset();
    let _ = write!(&mut stdout, "{} -> {} ", result.input_path.display(), result.output_path.display());

    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(&mut stdout, "({:.1} MB/s)", result.throughput_mbps);
    let _ = stdout.reset();
    let _ = writeln!(&mut stdout);
}
