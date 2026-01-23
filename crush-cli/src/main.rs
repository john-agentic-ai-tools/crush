mod cli;
mod commands;
mod config;
mod error;
mod logging;
mod output;
mod signal;

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

    // Initialize logging
    logging::init_logging();

    // Setup signal handler
    let _interrupted = signal::setup_handler()
        .map_err(|e| error::CliError::Config(format!("Failed to set up signal handler: {}", e)))?;

    // Dispatch to appropriate command
    match &cli.command {
        Commands::Compress(args) => commands::compress::run(args),
        Commands::Decompress(args) => commands::decompress::run(args),
        Commands::Inspect(args) => commands::inspect::run(args),
        Commands::Config(args) => commands::config::run(args),
        Commands::Plugins(args) => commands::plugins::run(args),
    }
}
