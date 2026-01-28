use crate::cli::{InspectArgs, OutputFormat};
use crate::error::{CliError, Result};
use crate::output;
use crush_core::inspect;
use is_terminal::IsTerminal;
use std::fs;
use tracing::info;

pub fn run(args: &InspectArgs) -> Result<()> {
    info!(file_count = args.input.len(), format = ?args.format, "Inspecting compressed files");
    let use_colors = std::io::stdout().is_terminal();

    let mut results = Vec::new();
    for input_path in &args.input {
        let compressed_data = fs::read(input_path).map_err(CliError::Io)?;

        let result = inspect(&compressed_data)?;
        results.push((input_path.clone().to_path_buf(), result));
    }

    match args.format {
        OutputFormat::Human => {
            if args.summary {
                output::format_inspect_summary(&results, use_colors);
            } else {
                for (path, result) in results {
                    output::format_inspect_result(&path, &result, use_colors);
                    println!(); // Add a newline between results
                }
            }
        }
        OutputFormat::Json => {
            output::format_inspect_json(&results, use_colors);
        }
        OutputFormat::Csv => {
            output::format_inspect_csv(&results, use_colors);
        }
    }
    Ok(())
}
