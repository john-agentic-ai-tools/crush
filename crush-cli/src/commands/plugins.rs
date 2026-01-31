use crate::cli::{OutputFormat, PluginsAction, PluginsArgs};
use crate::error::{CliError, Result};
use crate::output;
use crush_core::{compress_with_options, decompress, list_plugins, CompressionOptions};
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
