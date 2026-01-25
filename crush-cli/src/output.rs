use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use std::io::Write;
use crush_core::InspectResult;

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
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(&mut stdout, "  Compression ratio: ");
    let _ = stdout.reset();
    let _ = writeln!(&mut stdout, "{:.1}%", ratio);

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

    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)));
    let _ = writeln!(&mut stdout, "-----------------");
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)));
    let _ = writeln!(&mut stdout, "  Total Files: {}", results.len());
    let _ = writeln!(&mut stdout, "  Total Original Size: {}", total_original_size);
    let _ = writeln!(&mut stdout, "  Total Compressed Size: {}", total_compressed_size);
    let _ = writeln!(&mut stdout, "  Overall Ratio: {:.1}%", overall_ratio);
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
