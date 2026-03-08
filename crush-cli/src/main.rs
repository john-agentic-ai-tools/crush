mod algorithm;
mod cli;
mod commands;
mod config;
mod error;
mod feedback;
mod logging;
mod output;
mod signal;

// Force crush-parallel and crush-gpu to be linked into the binary so the linkme
// distributed-slice plugin registration runs at startup.
use crush_gpu as _;
use crush_parallel as _;

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
    // If verbose flag is set, it overrides config log level
    let log_level = if cli.verbose > 0 {
        logging::verbose_to_level(cli.verbose)
    } else {
        &config.logging.level
    };

    let log_file_path = if !config.logging.file.is_empty() {
        Some(std::path::Path::new(&config.logging.file))
    } else {
        None
    };
    logging::init_logging(log_level, &config.logging.format, log_file_path);

    // Setup signal handler
    let interrupted = signal::setup_handler()
        .map_err(|e| error::CliError::Config(format!("Failed to set up signal handler: {}", e)))?;

    // Configure GPU plugin: merge config file values with CLI flags (CLI wins)
    {
        let (cli_force_cpu, cli_gpu_device, cli_gpu_backend) = match &cli.command {
            Commands::Compress(args) => (false, args.gpu_device, args.gpu_backend),
            Commands::Decompress(args) => (args.force_cpu, args.gpu_device, args.gpu_backend),
            _ => (false, None, cli::GpuBackend::Auto),
        };
        let backend = match cli_gpu_backend {
            cli::GpuBackend::Auto => crush_gpu::BackendPreference::Auto,
            cli::GpuBackend::Cuda => crush_gpu::BackendPreference::Cuda,
            cli::GpuBackend::Wgpu => crush_gpu::BackendPreference::Wgpu,
        };
        crush_gpu::configure(crush_gpu::GpuPluginConfig {
            force_cpu: cli_force_cpu || config.gpu.force_cpu,
            device_index: cli_gpu_device.or(config.gpu.device),
            backend,
        });
    }

    // Dispatch to appropriate command
    match &cli.command {
        Commands::Compress(args) => commands::compress::run(args, interrupted, config.gpu.enabled),
        Commands::Decompress(args) => commands::decompress::run(args, interrupted),
        Commands::Inspect(args) => commands::inspect::run(args),
        Commands::Config(args) => commands::config::run(args),
        Commands::Plugins(args) => commands::plugins::run(args),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
