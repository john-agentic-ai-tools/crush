mod cli;
mod commands;
mod config;
mod error;
mod logging;
mod output;
mod signal;
mod state;

use clap::Parser;
use cli::{Cli, Commands};
use error::Result;

fn main() {
    let exit_code = match run() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Error: {}", e);
            e.exit_code()
        }
    };
    std::process::exit(exit_code);
}

fn run() -> Result<()> {
    // Initialize plugin registry
    crush_core::init_plugins()?;

    // Parse CLI arguments
    let cli = Cli::parse();

    // Load and merge configuration
    let mut config = config::load_config()?;
    config = config::merge_env_vars(config)?;
    config = config::merge_cli_args(config, &cli)?;
    config.validate()?;

    // Initialize logging with config
    let log_file_path = if !config.logging.file.is_empty() {
        Some(std::path::Path::new(&config.logging.file))
    } else {
        None
    };
    logging::init_logging(&config.logging.level, &config.logging.format, log_file_path);

    // Setup signal handler
    let interrupted = signal::setup_handler()
        .map_err(|e| error::CliError::Config(format!("Failed to set up signal handler: {}", e)))?;

    // Dispatch to appropriate command
    match &cli.command {
        Commands::Compress(args) => commands::compress::run(args, interrupted),
        Commands::Decompress(args) => commands::decompress::run(args, interrupted),
        Commands::Inspect(args) => commands::inspect::run(args),
        Commands::Config(args) => commands::config::run(args),
        Commands::Plugins(args) => commands::plugins::run(args),
    }
}
