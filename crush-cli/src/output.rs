use crush_core::InspectResult;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

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
        let _ = writeln!(
            &mut stdout,
            "{:.1}% (compressed to {:.1}% of original)",
            size_reduction, ratio
        );
    } else if size_reduction < 0.0 {
        let _ = writeln!(
            &mut stdout,
            "{:.1}% (expanded to {:.1}% of original)",
            size_reduction, ratio
        );
    } else {
        let _ = writeln!(&mut stdout, "0.0% (same size)");
    }

    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(&mut stdout, "  Plugin: ");
    let _ = stdout.reset();
    let _ = writeln!(&mut stdout, "{}", result.plugin_name);

    let crc_status_color = if result.crc_valid {
        Color::Green
    } else {
        Color::Red
    };
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(&mut stdout, "  CRC32: ");
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(crc_status_color)));
    let _ = writeln!(
        &mut stdout,
        "{}",
        if result.crc_valid { "VALID" } else { "INVALID" }
    );
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
    let _ = writeln!(
        &mut stdout,
        "  Total Original Size: {}",
        total_original_size
    );
    let _ = writeln!(
        &mut stdout,
        "  Total Compressed Size: {}",
        total_compressed_size
    );
    let _ = writeln!(
        &mut stdout,
        "  Overall Size Reduction: {:.1}%",
        overall_reduction
    );
    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(if all_crc_valid {
        Color::Green
    } else {
        Color::Red
    })));
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
    #[allow(dead_code)]
    pub input_size: u64,
    #[allow(dead_code)]
    pub output_size: u64,
    pub compression_ratio: f64,
    #[allow(dead_code)]
    pub duration: Duration,
    pub throughput_mbps: f64,
    pub plugin_used: String,
}

/// Result of a decompression operation
#[derive(Debug, Clone)]
pub struct DecompressionResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    #[allow(dead_code)]
    pub input_size: u64,
    #[allow(dead_code)]
    pub output_size: u64,
    #[allow(dead_code)]
    pub duration: Duration,
    pub throughput_mbps: f64,
    #[allow(dead_code)]
    pub crc_valid: bool,
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
    let _ = write!(
        &mut stdout,
        "{} -> {} ",
        result.input_path.display(),
        result.output_path.display()
    );

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
    let _ = write!(
        &mut stdout,
        "({}, {:.1} MB/s, {})",
        reduction_text, result.throughput_mbps, result.plugin_used
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
    let _ = write!(
        &mut stdout,
        "{} -> {} ",
        result.input_path.display(),
        result.output_path.display()
    );

    let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)));
    let _ = write!(&mut stdout, "({:.1} MB/s)", result.throughput_mbps);
    let _ = stdout.reset();
    let _ = writeln!(&mut stdout);
}

/// Format and print plugin list in human-readable format
pub fn format_plugin_list_human(plugins: &[crush_core::plugin::PluginMetadata]) {
    if plugins.is_empty() {
        println!("No plugins registered");
        return;
    }

    println!(
        "{:<15} {:<10} {:<15} {:<15} Description",
        "Name", "Version", "Throughput", "Compression"
    );
    println!("{}", "-".repeat(80));

    for plugin in plugins {
        let compression_pct = plugin.compression_ratio * 100.0;
        println!(
            "{:<15} {:<10} {:<13.1} MB/s {:<13.1}% {}",
            plugin.name, plugin.version, plugin.throughput, compression_pct, plugin.description
        );
    }

    println!("\nTotal plugins: {}", plugins.len());
}

/// Format and print plugin list in JSON format
pub fn format_plugin_list_json(
    plugins: &[crush_core::plugin::PluginMetadata],
) -> crate::error::Result<()> {
    // Create a serializable version of plugin data
    let json_plugins: Vec<serde_json::Value> = plugins
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "version": p.version,
                "magic_number": format!("0x{:02X}{:02X}{:02X}{:02X}",
                    p.magic_number[0], p.magic_number[1],
                    p.magic_number[2], p.magic_number[3]),
                "throughput_mbps": p.throughput,
                "compression_ratio": p.compression_ratio,
                "description": p.description,
            })
        })
        .collect();

    let json_output = serde_json::json!({
        "plugins": json_plugins,
        "count": plugins.len(),
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&json_output).map_err(|e| {
            crate::error::CliError::InvalidInput(format!("JSON serialization failed: {}", e))
        })?
    );

    Ok(())
}

/// Format and print GPU device information for `plugins info gpu-deflate`
pub fn format_gpu_device_info(info: &crush_gpu::GpuInfo) {
    println!("  GPU Device:");
    println!("    Name:      {}", info.name);
    println!("    Vendor:    {}", info.vendor);
    println!("    VRAM:      {} MB", info.vram_bytes / (1024 * 1024));
    println!("    Backend:   {}", info.api_backend);
}

/// Format and print detailed plugin information
pub fn format_plugin_info(plugin: &crush_core::plugin::PluginMetadata) {
    println!("Plugin: {}", plugin.name);
    println!("Version: {}", plugin.version);
    println!(
        "Magic Number: 0x{:02X}{:02X}{:02X}{:02X}",
        plugin.magic_number[0],
        plugin.magic_number[1],
        plugin.magic_number[2],
        plugin.magic_number[3]
    );
    println!();
    println!("Performance Characteristics:");
    println!("  Throughput: {:.1} MB/s", plugin.throughput);
    println!(
        "  Compression Ratio: {:.1}%",
        plugin.compression_ratio * 100.0
    );
    println!();
    println!("Description:");
    println!("  {}", plugin.description);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crush_core::InspectResult;
    use crush_core::plugin::{FileMetadata as CoreFileMetadata, PluginMetadata};
    use std::path::PathBuf;
    use std::time::Duration;

    fn sample_inspect_result() -> InspectResult {
        InspectResult {
            original_size: 1000,
            compressed_size: 400,
            plugin_name: "deflate".to_string(),
            crc_valid: true,
            metadata: CoreFileMetadata {
                mtime: Some(1700000000),
                #[cfg(unix)]
                permissions: Some(0o644),
            },
        }
    }

    fn sample_plugin() -> PluginMetadata {
        PluginMetadata {
            name: "deflate",
            version: "1.0.0",
            magic_number: [0x43, 0x52, 0x53, 0x48],
            throughput: 500.0,
            compression_ratio: 0.35,
            description: "DEFLATE compression",
        }
    }

    // -----------------------------------------------------------------------
    // format_warning
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_warning_does_not_panic() {
        format_warning("test warning message", true);
        format_warning("test warning message", false);
    }

    // -----------------------------------------------------------------------
    // format_inspect_result
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_inspect_result_does_not_panic() {
        let result = sample_inspect_result();
        let path = PathBuf::from("test.crush");
        format_inspect_result(&path, &result, false);
        format_inspect_result(&path, &result, true);
    }

    #[test]
    fn test_format_inspect_result_zero_original_size() {
        let result = InspectResult {
            original_size: 0,
            compressed_size: 0,
            plugin_name: "deflate".to_string(),
            crc_valid: true,
            metadata: CoreFileMetadata::default(),
        };
        let path = PathBuf::from("empty.crush");
        // Should handle zero-size gracefully
        format_inspect_result(&path, &result, false);
    }

    #[test]
    fn test_format_inspect_result_expansion() {
        // File grew (negative reduction)
        let result = InspectResult {
            original_size: 100,
            compressed_size: 150,
            plugin_name: "deflate".to_string(),
            crc_valid: false,
            metadata: CoreFileMetadata::default(),
        };
        let path = PathBuf::from("expanded.crush");
        format_inspect_result(&path, &result, false);
    }

    #[test]
    fn test_format_inspect_result_same_size() {
        let result = InspectResult {
            original_size: 100,
            compressed_size: 100,
            plugin_name: "deflate".to_string(),
            crc_valid: true,
            metadata: CoreFileMetadata::default(),
        };
        let path = PathBuf::from("same.crush");
        format_inspect_result(&path, &result, false);
    }

    #[test]
    fn test_format_inspect_result_no_mtime() {
        let result = InspectResult {
            original_size: 1000,
            compressed_size: 500,
            plugin_name: "deflate".to_string(),
            crc_valid: true,
            metadata: CoreFileMetadata::default(),
        };
        let path = PathBuf::from("no_mtime.crush");
        format_inspect_result(&path, &result, false);
    }

    // -----------------------------------------------------------------------
    // format_inspect_summary
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_inspect_summary_does_not_panic() {
        let results = vec![
            (PathBuf::from("a.crush"), sample_inspect_result()),
            (PathBuf::from("b.crush"), sample_inspect_result()),
        ];
        format_inspect_summary(&results, false);
        format_inspect_summary(&results, true);
    }

    #[test]
    fn test_format_inspect_summary_empty() {
        let results: Vec<(PathBuf, InspectResult)> = vec![];
        format_inspect_summary(&results, false);
    }

    #[test]
    fn test_format_inspect_summary_invalid_crc() {
        let mut bad = sample_inspect_result();
        bad.crc_valid = false;
        let results = vec![
            (PathBuf::from("good.crush"), sample_inspect_result()),
            (PathBuf::from("bad.crush"), bad),
        ];
        format_inspect_summary(&results, false);
    }

    // -----------------------------------------------------------------------
    // format_inspect_json
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_inspect_json_does_not_panic() {
        let results = vec![(PathBuf::from("a.crush"), sample_inspect_result())];
        format_inspect_json(&results, false);
    }

    // -----------------------------------------------------------------------
    // format_inspect_csv
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_inspect_csv_does_not_panic() {
        let results = vec![
            (PathBuf::from("a.crush"), sample_inspect_result()),
            (PathBuf::from("b.crush"), sample_inspect_result()),
        ];
        format_inspect_csv(&results, false);
    }

    #[test]
    fn test_format_inspect_csv_zero_original() {
        let result = InspectResult {
            original_size: 0,
            compressed_size: 0,
            plugin_name: "deflate".to_string(),
            crc_valid: true,
            metadata: CoreFileMetadata::default(),
        };
        let results = vec![(PathBuf::from("zero.crush"), result)];
        format_inspect_csv(&results, false);
    }

    // -----------------------------------------------------------------------
    // format_compression_result
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_compression_result_does_not_panic() {
        let result = CompressionResult {
            input_path: PathBuf::from("input.txt"),
            output_path: PathBuf::from("input.txt.crush"),
            input_size: 1000,
            output_size: 400,
            compression_ratio: 40.0,
            duration: Duration::from_millis(100),
            throughput_mbps: 9.5,
            plugin_used: "deflate".to_string(),
        };
        format_compression_result(&result, false);
        format_compression_result(&result, true);
    }

    #[test]
    fn test_format_compression_result_expansion() {
        let result = CompressionResult {
            input_path: PathBuf::from("input.bin"),
            output_path: PathBuf::from("input.bin.crush"),
            input_size: 100,
            output_size: 120,
            compression_ratio: 120.0,
            duration: Duration::from_millis(50),
            throughput_mbps: 1.9,
            plugin_used: "deflate".to_string(),
        };
        format_compression_result(&result, false);
    }

    #[test]
    fn test_format_compression_result_same_size() {
        let result = CompressionResult {
            input_path: PathBuf::from("input.bin"),
            output_path: PathBuf::from("input.bin.crush"),
            input_size: 100,
            output_size: 100,
            compression_ratio: 100.0,
            duration: Duration::from_millis(50),
            throughput_mbps: 1.9,
            plugin_used: "deflate".to_string(),
        };
        format_compression_result(&result, false);
    }

    // -----------------------------------------------------------------------
    // format_decompression_result
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_decompression_result_does_not_panic() {
        let result = DecompressionResult {
            input_path: PathBuf::from("input.crush"),
            output_path: PathBuf::from("input.txt"),
            input_size: 400,
            output_size: 1000,
            duration: Duration::from_millis(50),
            throughput_mbps: 19.0,
            crc_valid: true,
        };
        format_decompression_result(&result, false);
        format_decompression_result(&result, true);
    }

    // -----------------------------------------------------------------------
    // format_plugin_list_human
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_plugin_list_human_empty() {
        format_plugin_list_human(&[]);
    }

    #[test]
    fn test_format_plugin_list_human_with_plugins() {
        let plugins = vec![sample_plugin()];
        format_plugin_list_human(&plugins);
    }

    // -----------------------------------------------------------------------
    // format_plugin_list_json
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_plugin_list_json_empty() {
        let result = format_plugin_list_json(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_plugin_list_json_with_plugins() {
        let plugins = vec![sample_plugin()];
        let result = format_plugin_list_json(&plugins);
        assert!(result.is_ok());
    }

    // -----------------------------------------------------------------------
    // format_plugin_info
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_plugin_info_does_not_panic() {
        let plugin = sample_plugin();
        format_plugin_info(&plugin);
    }

    // -----------------------------------------------------------------------
    // format_gpu_device_info
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_gpu_device_info_does_not_panic() {
        let info = crush_gpu::GpuInfo {
            name: "Test GPU".to_string(),
            vendor: crush_gpu::GpuVendor::Nvidia,
            vram_bytes: 8 * 1024 * 1024 * 1024,
            api_backend: "Vulkan".to_string(),
        };
        format_gpu_device_info(&info);
    }
}
