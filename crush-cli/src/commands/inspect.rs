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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Write a real compressed archive to `dir` and return its path.
    fn compressed_file(dir: &tempfile::TempDir, name: &str) -> PathBuf {
        crush_core::init_plugins().expect("init_plugins");
        let payload = b"inspect me, please".repeat(64);
        let compressed = crush_core::compress(&payload).expect("compress");
        let path = dir.path().join(name);
        fs::write(&path, compressed).expect("write archive");
        path
    }

    fn args(input: Vec<PathBuf>, format: OutputFormat, summary: bool) -> InspectArgs {
        InspectArgs {
            input,
            format,
            summary,
        }
    }

    #[test]
    fn inspects_a_single_file_in_each_output_format() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = compressed_file(&dir, "one.crush");

        for format in [OutputFormat::Human, OutputFormat::Json, OutputFormat::Csv] {
            run(&args(vec![path.clone()], format, false)).expect("inspect");
        }
    }

    #[test]
    fn inspects_multiple_files_and_summarises_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        let inputs = vec![
            compressed_file(&dir, "a.crush"),
            compressed_file(&dir, "b.crush"),
        ];

        // The summary branch is only reachable with Human formatting.
        run(&args(inputs.clone(), OutputFormat::Human, true)).expect("summary");
        run(&args(inputs, OutputFormat::Human, false)).expect("per-file");
    }

    #[test]
    fn missing_file_surfaces_an_io_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("does-not-exist.crush");

        let result = run(&args(vec![missing], OutputFormat::Human, false));

        assert!(
            matches!(result, Err(CliError::Io(_))),
            "expected an Io error, got {result:?}"
        );
    }

    #[test]
    fn garbage_input_is_rejected_rather_than_inspected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("garbage.crush");
        fs::write(&path, b"this is definitely not a crush archive").expect("write");

        let result = run(&args(vec![path], OutputFormat::Human, false));

        assert!(result.is_err(), "malformed archive should not inspect ok");
    }

    #[test]
    fn one_bad_file_fails_the_whole_run() {
        let dir = tempfile::tempdir().expect("tempdir");
        let good = compressed_file(&dir, "good.crush");
        let bad = dir.path().join("nope.crush");

        // Reading is done up front for every input, so a later bad path must
        // abort the run rather than silently reporting only the good file.
        let result = run(&args(vec![good, bad], OutputFormat::Human, false));

        assert!(result.is_err());
    }
}
