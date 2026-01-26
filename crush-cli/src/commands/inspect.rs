use crate::cli::{InspectArgs, OutputFormat};
use crate::error::{Result, CliError};
use crate::output;
use crush_core::{inspect, InspectResult};
use std::fs;
use std::path::PathBuf;
use is_terminal::IsTerminal;

pub fn run(args: &InspectArgs) -> Result<()> {
    let use_colors = std::io::stdout().is_terminal();

    let mut results = Vec::new();
    for input_path in &args.input {
        let compressed_data = fs::read(input_path)
            .map_err(|e| CliError::Io(e))?;
        
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
        },
        OutputFormat::Json => {
            output::format_inspect_json(&results, use_colors);
        },
        OutputFormat::Csv => {
            output::format_inspect_csv(&results, use_colors);
        }
    }
    Ok(())
}