use crate::cli::{OutputFormat, PluginsAction, PluginsArgs};
use crate::error::{CliError, Result};
use crate::output;
use crush_core::{CompressionOptions, compress_with_options, decompress, list_plugins};
use tracing::info;

pub fn run(args: &PluginsArgs) -> Result<()> {
    match &args.action {
        PluginsAction::List { format } => {
            // Get all registered plugins
            let plugins = list_plugins();
            info!(plugin_count = plugins.len(), format = ?format, "Listing plugins");

            // Format and display
            match format {
                OutputFormat::Human => {
                    output::format_plugin_list_human(&plugins);
                }
                OutputFormat::Json => {
                    output::format_plugin_list_json(&plugins)?;
                }
                OutputFormat::Csv => {
                    return Err(CliError::InvalidInput(
                        "CSV format not supported for plugins list".to_string(),
                    ));
                }
            }

            Ok(())
        }

        PluginsAction::Info { name } => {
            // Get all plugins and find the requested one
            let plugins = list_plugins();
            let plugin = plugins
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| CliError::InvalidInput(format!("Plugin '{}' not found", name)))?;

            // Display detailed information
            output::format_plugin_info(plugin);

            // For gpu-deflate, show GPU device details
            if plugin.name.eq_ignore_ascii_case("gpu-deflate") {
                println!();
                match crush_gpu::discover_gpu() {
                    Ok(Some(backend)) => {
                        output::format_gpu_device_info(backend.gpu_info());
                    }
                    Ok(None) => {
                        println!("  GPU Device:  Not available (no compatible GPU detected)");
                        println!(
                            "  Note:        CPU fallback will be used for decompression of GPU-compressed files."
                        );
                    }
                    Err(e) => {
                        println!("  GPU Device:  Error during detection: {}", e);
                    }
                }
            }

            Ok(())
        }

        PluginsAction::Test { name } => {
            // Get all plugins and find the requested one
            let plugins = list_plugins();
            let plugin = plugins
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| CliError::InvalidInput(format!("Plugin '{}' not found", name)))?;

            println!("Testing plugin '{}'...", plugin.name);

            // Create test data
            let test_data = b"This is test data for plugin validation. \
                              It should compress and decompress correctly. \
                              The quick brown fox jumps over the lazy dog. \
                              1234567890 ABCDEFGHIJKLMNOPQRSTUVWXYZ";

            // Compress with the plugin
            let options = CompressionOptions::default().with_plugin(plugin.name);
            let compressed = compress_with_options(test_data, &options)?;

            println!(
                "  Compressed: {} bytes -> {} bytes",
                test_data.len(),
                compressed.len()
            );

            // Decompress
            let result = decompress(&compressed)?;

            println!("  Decompressed: {} bytes", result.data.len());

            // Verify roundtrip
            if result.data == test_data {
                println!("  ✓ Roundtrip validation: PASSED");
                println!("\nPlugin '{}' is working correctly", plugin.name);
                Ok(())
            } else {
                Err(CliError::InvalidInput(format!(
                    "Roundtrip validation failed: data mismatch (expected {} bytes, got {} bytes)",
                    test_data.len(),
                    result.data.len()
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{OutputFormat, PluginsAction, PluginsArgs};

    fn init_plugins() {
        // Ensure plugin registry is initialized (idempotent)
        let _ = crush_core::init_plugins();
    }

    #[test]
    fn test_plugins_list_human() {
        init_plugins();
        let args = PluginsArgs {
            action: PluginsAction::List {
                format: OutputFormat::Human,
            },
        };
        let result = run(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_plugins_list_json() {
        init_plugins();
        let args = PluginsArgs {
            action: PluginsAction::List {
                format: OutputFormat::Json,
            },
        };
        let result = run(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_plugins_list_csv_unsupported() {
        init_plugins();
        let args = PluginsArgs {
            action: PluginsAction::List {
                format: OutputFormat::Csv,
            },
        };
        let result = run(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_plugins_info_deflate() {
        init_plugins();
        let args = PluginsArgs {
            action: PluginsAction::Info {
                name: "deflate".to_string(),
            },
        };
        let result = run(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_plugins_info_not_found() {
        init_plugins();
        let args = PluginsArgs {
            action: PluginsAction::Info {
                name: "nonexistent-plugin-xyz".to_string(),
            },
        };
        let result = run(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_plugins_test_deflate() {
        init_plugins();
        let args = PluginsArgs {
            action: PluginsAction::Test {
                name: "deflate".to_string(),
            },
        };
        let result = run(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_plugins_test_not_found() {
        init_plugins();
        let args = PluginsArgs {
            action: PluginsAction::Test {
                name: "nonexistent-plugin-xyz".to_string(),
            },
        };
        let result = run(&args);
        assert!(result.is_err());
    }
}
